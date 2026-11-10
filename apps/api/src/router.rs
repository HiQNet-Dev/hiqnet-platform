//! Composition root de rutas: rutas HTTP, capas y documento OpenAPI.

use crate::*;

pub(crate) fn api_routes() -> (Router<AppState>, OpenApiDocument) {
    let (router, mut document) = OpenApiRouter::<AppState>::new()
        .routes(routes!(crate::health::health))
        .routes(routes!(crate::health::ready))
        .routes(routes!(crate::licensing::list_plans))
        .routes(routes!(crate::licensing::register_installation))
        .routes(routes!(crate::licensing::get_installation))
        .routes(routes!(crate::licensing::start_trial))
        .routes(routes!(
            crate::licensing::issue_license,
            crate::licensing::list_licenses
        ))
        .routes(routes!(crate::commercial_auth::login))
        .routes(routes!(crate::commercial_auth::logout))
        .routes(routes!(crate::commercial_auth::me))
        .routes(routes!(crate::commercial_auth::change_password))
        .routes(routes!(crate::commercial_auth::revoke_sessions))
        .routes(routes!(crate::organizations::list_organizations))
        .routes(routes!(crate::organizations::create_organization))
        .routes(routes!(crate::organizations::get_organization))
        .routes(routes!(
            crate::organizations::list_members,
            crate::organizations::add_member
        ))
        .routes(routes!(
            crate::organizations::change_member_role,
            crate::organizations::revoke_member
        ))
        .routes(routes!(enrollment::begin_enrollment))
        .routes(routes!(enrollment::approve_enrollment))
        .routes(routes!(enrollment::prove_enrollment))
        .routes(routes!(enrollment::list_installations))
        .routes(routes!(staff::staff_login))
        .routes(routes!(staff::staff_logout))
        .routes(routes!(staff::staff_me))
        .routes(routes!(
            control::list_control_products,
            control::create_control_product
        ))
        .routes(routes!(
            control::get_control_product,
            control::update_control_product
        ))
        .routes(routes!(
            control::list_control_projects,
            control::create_control_project
        ))
        .routes(routes!(
            control::get_control_project,
            control::update_control_project
        ))
        .routes(routes!(control_directory::list_control_organizations))
        .routes(routes!(control_directory::get_control_organization))
        .routes(routes!(control_directory::list_control_accounts))
        .routes(routes!(control_directory::get_control_account))
        .routes(routes!(control_installations::list_control_installations))
        .routes(routes!(control_installations::get_control_installation))
        .routes(routes!(control_installations::suspend_control_installation))
        .routes(routes!(
            control_installations::reactivate_control_installation
        ))
        .routes(routes!(control_installations::rehost_control_installation))
        .routes(routes!(control_audit::list_control_audit))
        .routes(routes!(control_audit::get_control_audit))
        .routes(routes!(control_audit::list_control_security_events))
        .routes(routes!(control_audit::get_control_security_event))
        .routes(routes!(control_audit::list_control_staff_auth_events))
        .routes(routes!(control_audit::get_control_staff_auth_event))
        .routes(routes!(
            control_organization_products::list_control_organization_products
        ))
        .routes(routes!(
            control_organization_products::activate_control_organization_product
        ))
        .routes(routes!(
            control_organization_products::update_control_organization_product
        ))
        .routes(routes!(control_commercial::list_control_plans))
        .routes(routes!(control_commercial::get_control_plan))
        .routes(routes!(control_commercial::create_control_plan))
        .routes(routes!(control_commercial::update_control_plan))
        .routes(routes!(control_commercial::list_control_subscriptions))
        .routes(routes!(control_commercial::get_control_subscription))
        .routes(routes!(control_commercial::create_control_subscription))
        .routes(routes!(control_commercial::suspend_control_subscription))
        .routes(routes!(control_commercial::reactivate_control_subscription))
        .routes(routes!(control_commercial::cancel_control_subscription))
        .routes(routes!(control_commercial::renew_control_subscription))
        .routes(routes!(control_commercial::get_control_entitlements))
        .routes(routes!(portal_commercial::list_organization_products))
        .routes(routes!(portal_commercial::get_organization_product))
        .split_for_parts();
    document.info = Info::new("HiQNet Platform API", env!("CARGO_PKG_VERSION"));
    document.info.description = Some("Plano de control técnico con identidad comercial y sesiones; sin facturación, suscripciones ni sincronización productivas. El token administrativo es temporal y exclusivo de desarrollo.".into());
    document
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
        "devAdminToken",
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .description(Some(
                    "Token administrativo temporal para pruebas DEV; no es una sesión de usuario.",
                ))
                .build(),
        ),
    );
    document
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
            "commercialSession",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                "hiqnet_session",
                "Cookie de sesión comercial HttpOnly emitida por /api/v1/auth/login.",
            ))),
        );
    document
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
            "staffSession",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                "hiqnet_staff_session",
                "Cookie de sesión de personal interno emitida por /api/v1/control/auth/login.",
            ))),
        );
    (router, document)
}

pub fn openapi_document() -> OpenApiDocument {
    api_routes().1
}

pub fn build_router(state: AppState, docs_enabled: bool, auth_state: AuthState) -> Router {
    let (router, document) = api_routes();
    let router = router
        .with_state(state)
        .layer(axum::Extension(auth_state.clone()));
    if docs_enabled {
        router.merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", document))
    } else {
        router
    }
}
