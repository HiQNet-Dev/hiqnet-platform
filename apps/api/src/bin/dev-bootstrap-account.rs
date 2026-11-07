//! Crea una cuenta comercial sintética para desarrollo local.
//!
//! Requiere `HIQNET_PLATFORM_ENV=development` y `HIQNET_PLATFORM_DEV_BOOTSTRAP=true`.
//! La contraseña se lee por entrada estándar y nunca se pasa por argumentos.
//!
//! Ejemplo:
//! `HIQNET_PLATFORM_DEV_BOOTSTRAP=true cargo run -p hiqnet-platform-api --bin dev-bootstrap-account -- cuenta@example.test "Cuenta DEV"`

use platform_application::{
    auth::AuthRepository, commercial::CommercialService, security::hash_password,
};
use platform_core::auth::CommercialCredential;
use platform_storage_postgres::{PgAuthRepository, PgCommercialRepository, open};
use std::{
    env,
    io::{self, Write},
};
use time::OffsetDateTime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if env::var("HIQNET_PLATFORM_ENV").as_deref() != Ok("development") {
        anyhow::bail!("este comando sólo está permitido con HIQNET_PLATFORM_ENV=development");
    }
    if env::var("HIQNET_PLATFORM_DEV_BOOTSTRAP").as_deref() != Ok("true") {
        anyhow::bail!("define HIQNET_PLATFORM_DEV_BOOTSTRAP=true para ejecutar este comando");
    }
    let database_url = env::var("DATABASE_URL")?;
    let email = env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("falta el correo"))?;
    let name = env::args()
        .nth(2)
        .unwrap_or_else(|| "Cuenta DEV".to_owned());

    print!("Contraseña DEV (entrada estándar, no se pasa por argumentos): ");
    io::stdout().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    let password = password.trim_end_matches(['\r', '\n']).to_owned();
    if password.len() < 12 {
        anyhow::bail!("la contraseña DEV debe tener al menos 12 caracteres");
    }

    let pool = open(&database_url).await?;
    let now = OffsetDateTime::now_utc();
    let account = CommercialService::new(PgCommercialRepository::new(pool.clone()))
        .create_account(&email, &name, now)
        .await?;
    let hash = hash_password(password).await?;
    PgAuthRepository::new(pool)
        .create_credential(&CommercialCredential::new(account.id, hash, now))
        .await?;
    println!("Cuenta DEV creada: {} ({})", account.email, account.id);
    Ok(())
}
