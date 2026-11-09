//! Crea una cuenta de personal interno sintética para desarrollo local.
//!
//! Requiere `HIQNET_PLATFORM_ENV=development` y `HIQNET_PLATFORM_DEV_STAFF_BOOTSTRAP=true`.
//! La contraseña se lee por entrada estándar y nunca se pasa por argumentos.
//!
//! Ejemplo:
//! `HIQNET_PLATFORM_DEV_STAFF_BOOTSTRAP=true cargo run -p hiqnet-platform-api --bin dev-bootstrap-staff -- staff@hiqnet.test "Staff DEV" super_admin`

use platform_application::security::hash_password;
use platform_core::staff::StaffRole;
use platform_storage_postgres::{PgStaffRepository, open};
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
    if env::var("HIQNET_PLATFORM_DEV_STAFF_BOOTSTRAP").as_deref() != Ok("true") {
        anyhow::bail!("define HIQNET_PLATFORM_DEV_STAFF_BOOTSTRAP=true para ejecutar este comando");
    }
    let database_url = env::var("DATABASE_URL")?;
    let email = env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("falta el correo"))?;
    let name = env::args().nth(2).unwrap_or_else(|| "Staff DEV".to_owned());
    let role: StaffRole = env::args()
        .nth(3)
        .unwrap_or_else(|| "super_admin".to_owned())
        .parse()
        .map_err(|_| anyhow::anyhow!("rol inválido; usa uno de StaffRole::ALL"))?;

    print!("Contraseña staff DEV (entrada estándar, no se pasa por argumentos): ");
    io::stdout().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    let password = password.trim_end_matches(['\r', '\n']).to_owned();
    if password.len() < 12 {
        anyhow::bail!("la contraseña staff DEV debe tener al menos 12 caracteres");
    }

    let pool = open(&database_url).await?;
    let now = OffsetDateTime::now_utc();
    let hash = hash_password(password).await?;
    let account_id = PgStaffRepository::new(pool)
        .create_staff(&email, &name, role, &hash, now)
        .await?;
    println!("Cuenta staff DEV creada: {email} ({account_id}, rol {role})");
    Ok(())
}
