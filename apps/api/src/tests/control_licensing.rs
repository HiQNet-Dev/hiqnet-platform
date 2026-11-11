//! Emisión controlada de licencias: elegibilidad, auditoría, firma y duplicados.
//!
//! Usa un firmante sintético efímero y una base de pruebas aislada. Ninguna
//! clave real ni emisión productiva participa aquí.

use super::common::*;
use super::*;
use ed25519_dalek::Verifier;
use sqlx::Row;

const ORIGIN: Option<&str> = Some("http://127.0.0.1:8080");
const FIN_VIGENCIA: &str = "2030-01-01T00:00:00Z";

struct Escenario {
    router: Router,
    pool: PgPool,
    organization: Uuid,
    product: Uuid,
    installation: Uuid,
    admin: String,
}

async fn seed_comercial(
    pool: &PgPool,
    plan_key: &str,
    features: Value,
    ends_at: &str,
    product_status: &str,
) -> (Uuid, Uuid, Uuid) {
    let organization = Uuid::new_v4();
    let product = Uuid::new_v4();
    let plan = Uuid::new_v4();
    test_organization(pool, organization).await;
    sqlx::query(
        "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)\n             VALUES ($1, $2, 'Producto sintético', '', 'active', 'desktop', now(), now())",
    )
    .bind(product)
    .bind(format!("pos-{}", Uuid::new_v4().simple()))
    .execute(pool)
    .await
    .unwrap();
    if product_status == "active" {
        sqlx::query(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
                 VALUES ($1, $2, 'active', now())",
        )
        .bind(organization)
        .bind(product)
        .execute(pool)
        .await
        .unwrap();
    } else {
        sqlx::query(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
                 VALUES ($1, $2, 'inactive', now())",
        )
        .bind(organization)
        .bind(product)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO plan (id, product_id, key, name, description, status, is_trial, created_at, updated_at)\n             VALUES ($1, $2, $3, 'Plan sintético', '', 'active', false, now(), now())",
    )
    .bind(plan)
    .bind(product)
    .bind(plan_key)
    .execute(pool)
    .await
    .unwrap();
    let features = features.as_object().unwrap();
    for (key, enabled) in features {
        sqlx::query("INSERT INTO plan_feature (plan_id, key, value) VALUES ($1, $2, $3)")
            .bind(plan)
            .bind(key)
            .bind(enabled.as_bool().unwrap_or(false))
            .execute(pool)
            .await
            .unwrap();
    }
    let ends_at =
        time::OffsetDateTime::parse(ends_at, &time::format_description::well_known::Rfc3339)
            .unwrap();
    // Una suscripción vencida se representa con una ventana cerrada en el
    // pasado; el CHECK subscription_dates exige ends_at > starts_at.
    let (starts_at, ends_final) = if ends_at < time::OffsetDateTime::now_utc() {
        (ends_at - time::Duration::days(31), ends_at)
    } else {
        (
            time::OffsetDateTime::now_utc() - time::Duration::days(1),
            ends_at,
        )
    };
    sqlx::query(
        "INSERT INTO subscription
             (id, organization_id, product_id, plan_id, status, is_trial, starts_at, ends_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'active', false, $5, $6, now(), now())",
    )
    .bind(Uuid::new_v4())
    .bind(organization)
    .bind(product)
    .bind(plan)
    .bind(starts_at)
    .bind(ends_final)
    .execute(pool)
    .await
    .unwrap();
    (organization, product, plan)
}

async fn escenario(
    plan_key: &str,
    features: Value,
    ends_at: &str,
    product_status: &str,
) -> Escenario {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("licensig-admin-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &email,
        password,
        platform_core::staff::StaffRole::PlatformAdmin,
    )
    .await;
    let (organization, product, _plan) =
        seed_comercial(&pool, plan_key, features, ends_at, product_status).await;

    let router = build_router(
        issuance_state(pool.clone(), true),
        false,
        test_auth_state(pool.clone()),
    );
    let admin = control_login(&router, &email, password).await;

    let installation = Uuid::new_v4();
    let registration = json!({
        "installation_id": installation,
        "organization_id": organization,
        "device_label": "fixture-emision"
    });
    let (status, _) = send(
        &router,
        Method::POST,
        "/api/v1/installations",
        Some("token-sintetico-p01"),
        Some(registration),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    Escenario {
        router,
        pool,
        organization,
        product,
        installation,
        admin,
    }
}

fn emitir_uri(installation: &Uuid) -> String {
    format!("/api/v1/control/installations/{installation}/licenses")
}

async fn emitir(scenario: &Escenario, product: Option<Uuid>) -> (StatusCode, Value) {
    let body = json!({"product_id": product.unwrap_or(scenario.product)});
    let (status, _, body) = send_method(
        &scenario.router,
        Method::POST,
        &emitir_uri(&scenario.installation),
        Some(&scenario.admin),
        ORIGIN,
        Some(body),
    )
    .await;
    (status, body)
}

async fn eventos(pool: &PgPool, accion: &str, instalacion: &Uuid) -> Vec<(String, String, Value)> {
    let rows = sqlx::query(
        "SELECT result, resource_id, metadata::text AS metadata
         FROM staff_audit_event
         WHERE action = $1 AND metadata->>'installation_id' = $2
         ORDER BY occurred_at",
    )
    .bind(accion)
    .bind(instalacion.to_string())
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|row| {
            (
                row.get::<String, _>("result"),
                row.get::<Option<Uuid>, _>("resource_id")
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                serde_json::from_str(&row.get::<String, _>("metadata")).unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn emision_elegible_firma_audita_y_deriva_todo_del_derecho() {
    let scenario = escenario(
        "pro",
        json!({"catalog": true, "sales": true, "backups.local": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let (status, body) = emitir(&scenario, None).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // El plan, las capacidades y la vigencia provienen del derecho comercial,
    // no del cuerpo de la solicitud (que sólo lleva el producto).
    let grant: Value = serde_json::from_str(body["grant_json"].as_str().unwrap()).unwrap();
    assert_eq!(grant["version"], 1);
    assert_eq!(grant["plan"], "pro");
    // El contrato v1 serializa las fechas con el formato por defecto de `time`
    // en ambas implementaciones; lo verificamos contra el parseo del mismo valor.
    let vigencia =
        time::OffsetDateTime::parse(FIN_VIGENCIA, &time::format_description::well_known::Rfc3339)
            .unwrap();
    assert_eq!(
        grant["valid_until"],
        serde_json::to_value(vigencia).unwrap()
    );
    assert_eq!(grant["installation_id"], scenario.installation.to_string());
    assert_eq!(grant["organization_id"], scenario.organization.to_string());
    let features = grant["features"].as_array().unwrap();
    assert_eq!(features.len(), 3);
    let grant_id: Uuid = serde_json::from_value(body["grant_id"].clone()).unwrap();

    // La firma verifica con la clave pública entregada en la misma respuesta.
    let verifier = ed25519_dalek::VerifyingKey::from_bytes(
        &base64::engine::general_purpose::STANDARD
            .decode(body["public_key_b64"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
    )
    .unwrap();
    let signature = ed25519_dalek::Signature::from_slice(
        &base64::engine::general_purpose::STANDARD
            .decode(body["signature_b64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    verifier
        .verify(body["grant_json"].as_str().unwrap().as_bytes(), &signature)
        .expect("el grant emitido debe verificar con su clave pública");

    // Auditoría de emisión: grant, organización, producto, plan, vigencia y actor.
    let eventos = eventos(&scenario.pool, "license.issued", &scenario.installation).await;
    assert_eq!(eventos.len(), 1);
    let (resultado, recurso, metadata) = &eventos[0];
    assert_eq!(resultado, "success");
    assert_eq!(*recurso, grant_id.to_string());
    assert_eq!(metadata["plan"], "pro");
    assert!(metadata["subscription_id"].is_string());
    assert_eq!(metadata["valid_until"], FIN_VIGENCIA);
    let actor_id: Uuid = sqlx::query_scalar(
        "SELECT staff_account_id FROM staff_audit_event WHERE action = 'license.issued' AND resource_id = $1",
    )
    .bind(grant_id)
    .fetch_one(&scenario.pool)
    .await
    .unwrap();

    // El registro de licencia enlaza la suscripción que autorizó la emisión.
    let subscription_id: Uuid =
        sqlx::query_scalar("SELECT subscription_id FROM issued_license WHERE grant_id = $1")
            .bind(grant_id)
            .fetch_one(&scenario.pool)
            .await
            .unwrap();
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM subscription WHERE id = $1",)
            .bind(subscription_id)
            .fetch_one(&scenario.pool)
            .await
            .unwrap()
            == 1
    );
    let _ = actor_id;
}

#[tokio::test]
async fn suscripcion_no_elegible_se_rechaza_y_se_audita() {
    // Vigencia vencida: el derecho comercial no es efectivo.
    let scenario = escenario(
        "local",
        json!({"catalog": true, "sales": true}),
        "2026-01-01T00:00:00Z",
        "active",
    )
    .await;
    let (status, body) = emitir(&scenario, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "not_entitled");

    let denegados = eventos(
        &scenario.pool,
        "license.issuance_denied",
        &scenario.installation,
    )
    .await;
    assert_eq!(denegados.len(), 1);
    assert_eq!(denegados[0].0, "denied");
    assert_eq!(denegados[0].2["reason"], "not_entitled");
    assert!(
        eventos(&scenario.pool, "license.issued", &scenario.installation)
            .await
            .is_empty()
    );
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM issued_license WHERE installation_id = $1")
            .bind(scenario.installation)
            .fetch_one(&scenario.pool)
            .await
            .unwrap();
    assert_eq!(total, 0);
}

#[tokio::test]
async fn producto_inhabilitado_se_rechaza() {
    let scenario = escenario(
        "local",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "inactive",
    )
    .await;
    let (status, body) = emitir(&scenario, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "not_entitled");
    let denegados = eventos(
        &scenario.pool,
        "license.issuance_denied",
        &scenario.installation,
    )
    .await;
    assert_eq!(denegados[0].2["reason"], "not_entitled");
}

#[tokio::test]
async fn permisos_insuficientes_se_rechazan_y_se_auditan() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("licensig-support-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &email,
        password,
        platform_core::staff::StaffRole::Support,
    )
    .await;
    let (organization, product, _plan) = seed_comercial(
        &pool,
        "local",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let router = build_router(
        issuance_state(pool.clone(), true),
        false,
        test_auth_state(pool.clone()),
    );
    let session = control_login(&router, &email, password).await;
    let installation = Uuid::new_v4();
    let (status, _) = send(
        &router,
        Method::POST,
        "/api/v1/installations",
        Some("token-sintetico-p01"),
        Some(json!({
            "installation_id": installation,
            "organization_id": organization,
            "device_label": "fixture-soporte"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _, body) = send_method(
        &router,
        Method::POST,
        &emitir_uri(&installation),
        Some(&session),
        ORIGIN,
        Some(json!({"product_id": product})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "forbidden");
    let denegados = eventos(&pool, "license.issuance_denied", &installation).await;
    assert_eq!(denegados.len(), 1);
    assert_eq!(denegados[0].2["reason"], "capability");
}

#[tokio::test]
async fn plan_o_capacidad_ajenos_al_contrato_v1_se_rechazan() {
    let plan_scenario = escenario(
        "pos-basico",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let (status, body) = emitir(&plan_scenario, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "unsupported_plan");

    let feature_scenario = escenario(
        "pro",
        json!({"catalog": true, "sync": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let (status, body) = emitir(&feature_scenario, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "unsupported_feature");
}

/// Reintento tras perder la respuesta HTTP (B): la segunda solicitud recupera
/// el grant original sin borrar registros ni emitir otro documento.
#[tokio::test]
async fn recuperacion_reentrega_el_mismo_grant_sin_nueva_emision() {
    let scenario = escenario(
        "local",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let (first, body) = emitir(&scenario, None).await;
    assert_eq!(first, StatusCode::CREATED, "{body}");
    assert_eq!(body["recovered"], false);
    let grant_original = body["grant_json"].clone();
    let firma_original = body["signature_b64"].clone();

    // Segunda solicitud con el mismo contexto comercial: recuperación, no
    // duplicado. Se reentrega exactamente el documento original.
    let (second, body) = emitir(&scenario, None).await;
    assert_eq!(second, StatusCode::OK, "{body}");
    assert_eq!(body["recovered"], true);
    assert_eq!(body["grant_json"], grant_original);
    assert_eq!(body["signature_b64"], firma_original);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM issued_license WHERE installation_id = $1",
        )
        .bind(scenario.installation)
        .fetch_one(&scenario.pool)
        .await
        .unwrap(),
        1,
        "sólo un grant para la suscripción"
    );

    // La recuperación se audita aparte y no se contabiliza como emisión.
    let emitidas = eventos(&scenario.pool, "license.issued", &scenario.installation).await;
    assert_eq!(emitidas.len(), 1);
    let recuperadas = eventos(&scenario.pool, "license.recovered", &scenario.installation).await;
    assert_eq!(recuperadas.len(), 1);
    assert_eq!(recuperadas[0].0, "success");
    assert_eq!(recuperadas[0].1, body["grant_id"].as_str().unwrap());
    let denegados = eventos(
        &scenario.pool,
        "license.issuance_denied",
        &scenario.installation,
    )
    .await;
    assert!(denegados.is_empty());
}

/// Solicitudes concurrentes: la unicidad (instalación, suscripción) garantiza
/// un único grant; la solicitud perdedora recupera el documento ganador.
#[tokio::test]
async fn emisiones_concurrentes_producen_un_solo_grant() {
    let scenario = escenario(
        "local",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let (a, b) = tokio::join!(emitir(&scenario, None), emitir(&scenario, None));
    assert!(a.0.is_success(), "{}", a.1);
    assert!(b.0.is_success(), "{}", b.1);
    assert_eq!(a.1["grant_id"], b.1["grant_id"]);
    assert_eq!(a.1["grant_json"], b.1["grant_json"]);
    let emitidas = eventos(&scenario.pool, "license.issued", &scenario.installation).await;
    assert_eq!(emitidas.len(), 1, "una única emisión real");
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM issued_license WHERE installation_id = $1")
            .bind(scenario.installation)
            .fetch_one(&scenario.pool)
            .await
            .unwrap();
    assert_eq!(total, 1);
}

/// La recuperación exige contexto comercial vigente: si la suscripción venció
/// después de emitir, no se reentrega el grant ni se emite otro.
#[tokio::test]
async fn recuperacion_de_grant_con_suscripcion_vencida_se_rechaza() {
    let scenario = escenario(
        "local",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let (first, body) = emitir(&scenario, None).await;
    assert_eq!(first, StatusCode::CREATED, "{body}");
    let subscription_id: Uuid =
        sqlx::query_scalar("SELECT subscription_id FROM issued_license WHERE installation_id = $1")
            .bind(scenario.installation)
            .fetch_one(&scenario.pool)
            .await
            .unwrap();
    sqlx::query("UPDATE subscription SET ends_at = now() - interval '1 day' WHERE id = $1")
        .bind(subscription_id)
        .execute(&scenario.pool)
        .await
        .unwrap();

    let (second, body) = emitir(&scenario, None).await;
    assert_eq!(second, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "not_entitled");
    assert!(
        eventos(&scenario.pool, "license.recovered", &scenario.installation)
            .await
            .is_empty()
    );
    let denegados = eventos(
        &scenario.pool,
        "license.issuance_denied",
        &scenario.installation,
    )
    .await;
    assert_eq!(denegados.len(), 1);
    assert_eq!(denegados[0].2["reason"], "not_entitled");
}

/// Suscripción suspendida: sin derecho efectivo, la emisión (y con ella la
/// recuperación) se rechaza.
#[tokio::test]
async fn suscripcion_suspendida_se_rechaza() {
    let scenario = escenario(
        "local",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    sqlx::query(
        "UPDATE subscription SET status = 'suspended', suspended_at = now() WHERE organization_id = $1",
    )
    .bind(scenario.organization)
    .execute(&scenario.pool)
    .await
    .unwrap();
    let (status, body) = emitir(&scenario, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "not_entitled");
    assert!(
        eventos(&scenario.pool, "license.issued", &scenario.installation)
            .await
            .is_empty()
    );
}

/// La renovación (C) crea una suscripción nueva: la emisión posterior es una
/// primera emisión para esa suscripción (grant nuevo), el grant anterior queda
/// en el historial y no se reutiliza ni se elimina.
#[tokio::test]
async fn renovacion_posterior_emite_grant_nuevo_para_la_nueva_suscripcion() {
    let scenario = escenario(
        "local",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let (first, body) = emitir(&scenario, None).await;
    assert_eq!(first, StatusCode::CREATED, "{body}");
    let grant_original: Uuid = serde_json::from_value(body["grant_id"].clone()).unwrap();
    let subscription_original: Uuid =
        sqlx::query_scalar("SELECT subscription_id FROM issued_license WHERE grant_id = $1")
            .bind(grant_original)
            .fetch_one(&scenario.pool)
            .await
            .unwrap();

    // Renovación por el dominio comercial: nueva suscripción con ventana nueva.
    let renewal = Request::builder()
        .method(Method::POST)
        .uri(format!(
            "/api/v1/control/subscriptions/{subscription_original}/renew"
        ))
        .header(header::COOKIE, &scenario.admin)
        .header(header::ORIGIN, "http://127.0.0.1:8080")
        .header("Idempotency-Key", format!("renew-{}", Uuid::new_v4()))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"plan_id": null, "ends_at": "2031-01-01T00:00:00Z"}).to_string(),
        ))
        .unwrap();
    let response = scenario.router.clone().oneshot(renewal).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Emisión tras renovar: el derecho vigente es la suscripción nueva, así
    // que la emisión es nueva (201) y produce un grant distinto.
    let (second, body) = emitir(&scenario, None).await;
    assert_eq!(second, StatusCode::CREATED, "{body}");
    assert_eq!(body["recovered"], false);
    let grant_nuevo: Uuid = serde_json::from_value(body["grant_id"].clone()).unwrap();
    assert_ne!(grant_nuevo, grant_original);
    let subscription_nueva: Uuid =
        sqlx::query_scalar("SELECT subscription_id FROM issued_license WHERE grant_id = $1")
            .bind(grant_nuevo)
            .fetch_one(&scenario.pool)
            .await
            .unwrap();
    assert_ne!(subscription_nueva, subscription_original);
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM issued_license WHERE installation_id = $1")
            .bind(scenario.installation)
            .fetch_one(&scenario.pool)
            .await
            .unwrap();
    assert_eq!(total, 2, "historial de grants preservado");
    // La vigencia nueva proviene de la suscripción renovada, no del cliente.
    let grant: Value = serde_json::from_str(body["grant_json"].as_str().unwrap()).unwrap();
    assert_eq!(grant["plan"], "local");
    let emitidas = eventos(&scenario.pool, "license.issued", &scenario.installation).await;
    assert_eq!(emitidas.len(), 2);
}

#[tokio::test]
async fn emision_deshabilitada_responde_503_y_no_consulta_derechos() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("licensig-admin-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &email,
        password,
        platform_core::staff::StaffRole::PlatformAdmin,
    )
    .await;
    let (organization, product, _plan) = seed_comercial(
        &pool,
        "local",
        json!({"catalog": true, "sales": true}),
        FIN_VIGENCIA,
        "active",
    )
    .await;
    let router = build_router(
        issuance_state(pool.clone(), false),
        false,
        test_auth_state(pool.clone()),
    );
    let admin = control_login(&router, &email, password).await;
    let installation = Uuid::new_v4();
    let (status, _) = send(
        &router,
        Method::POST,
        "/api/v1/installations",
        Some("token-sintetico-p01"),
        Some(json!({
            "installation_id": installation,
            "organization_id": organization,
            "device_label": "fixture-deshabilitado"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _, body) = send_method(
        &router,
        Method::POST,
        &emitir_uri(&installation),
        Some(&admin),
        ORIGIN,
        Some(json!({"product_id": product})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "licensing_disabled");
    assert!(
        eventos(&pool, "license.issuance_denied", &installation)
            .await
            .is_empty()
    );
}
