//! Emisión de licencias a partir de derechos comerciales ya resueltos.
//!
//! Esta capa **no** consulta la suscripción: exige que [`Entitlements`] venga
//! ya resuelto por `CommercialService::entitlements` (organización activa +
//! producto habilitado + suscripción vigente) y traduce ese derecho al contrato
//! `LicenseGrant` v1. El cliente HTTP nunca aporta plan, features ni fechas.
//!
//! La traducción es el único punto donde el plan comercial se convierte en el
//! documento firmado que el POS verifica. Sólo se emiten planes y capacidades
//! representables en v1 (`PlanName` y `FeatureKey` de `platform-core`), porque
//! el verificador del POS es un enum cerrado: un grant con claves ajenas sería
//! rechazado en la instalación.

use crate::commercial_domain::Entitlements;
use crate::error::{AppError, AppResult};
use platform_core::ids::OrganizationId;
use platform_core::plan::{FeatureKey, PlanName};
use time::OffsetDateTime;
use uuid::Uuid;

/// Derechos verificados que autorizan una emisión concreta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitledPlan {
    pub organization_id: OrganizationId,
    pub product_id: Uuid,
    pub subscription_id: Uuid,
    pub plan: String,
    pub features: Vec<String>,
    pub valid_until: OffsetDateTime,
    pub is_trial: bool,
}

impl EntitledPlan {
    /// Construye el derecho a emitir desde el resultado del dominio comercial.
    ///
    /// Se rechaza cualquier combinación incompleta: sin vigencia, sin plan, sin
    /// suscripción o sin capacidades efectivas no hay nada que firmar. El plan y
    /// las capacidades se traducen al catálogo v1; una clave no representable es
    /// un error de configuración comercial, no un problema de elegibilidad, así
    /// que se distingue de [`AppError::NotEntitled`].
    pub fn from_entitlements(
        organization_id: OrganizationId,
        product_id: Uuid,
        entitlements: &Entitlements,
    ) -> AppResult<Self> {
        if !entitlements.effective {
            return Err(AppError::NotEntitled);
        }
        let plan_ref = entitlements.plan.as_ref().ok_or(AppError::NotEntitled)?;
        let valid_until = entitlements.ends_at.ok_or(AppError::NotEntitled)?;
        let subscription_id = entitlements.subscription_id.ok_or(AppError::NotEntitled)?;
        let plan = PlanName::parse(&plan_ref.key).map_err(|_| AppError::UnsupportedPlan)?;
        let mut features = Vec::new();
        for (key, enabled) in &entitlements.features {
            if !*enabled {
                continue;
            }
            let feature = FeatureKey::from_key(key).ok_or(AppError::UnsupportedFeature)?;
            features.push(feature.as_str().to_owned());
        }
        if features.is_empty() {
            return Err(AppError::NotEntitled);
        }
        Ok(Self {
            organization_id,
            product_id,
            subscription_id,
            plan: plan.as_str().to_owned(),
            features,
            valid_until,
            is_trial: entitlements.is_trial,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial_domain::{PlanRef, SubscriptionStatus};
    use std::collections::BTreeMap;

    fn entitlements(effective: bool) -> Entitlements {
        let mut features = BTreeMap::new();
        features.insert("catalog".to_owned(), true);
        features.insert("sales".to_owned(), true);
        Entitlements {
            effective,
            status: Some(SubscriptionStatus::Active),
            reason: "active".into(),
            plan: Some(PlanRef {
                id: Uuid::new_v4(),
                key: "local".into(),
                name: "Local".into(),
                is_trial: false,
            }),
            subscription_id: Some(Uuid::new_v4()),
            is_trial: false,
            features,
            limits: BTreeMap::new(),
            starts_at: Some(OffsetDateTime::now_utc()),
            ends_at: Some(OffsetDateTime::now_utc() + time::Duration::days(30)),
        }
    }

    #[test]
    fn traduce_el_derecho_comercial_a_v1() {
        let entitled = EntitledPlan::from_entitlements(
            OrganizationId::new(),
            Uuid::new_v4(),
            &entitlements(true),
        )
        .unwrap();
        assert_eq!(entitled.plan, "local");
        assert!(entitled.features.contains(&"catalog".to_owned()));
        assert!(entitled.features.contains(&"sales".to_owned()));
    }

    #[test]
    fn rechaza_derecho_no_efectivo() {
        let error = EntitledPlan::from_entitlements(
            OrganizationId::new(),
            Uuid::new_v4(),
            &entitlements(false),
        )
        .unwrap_err();
        assert_eq!(error, AppError::NotEntitled);
    }

    #[test]
    fn rechaza_falta_de_plan_de_suscripcion_o_vigencia() {
        let mut incomplete = entitlements(true);
        incomplete.plan = None;
        assert_eq!(
            EntitledPlan::from_entitlements(OrganizationId::new(), Uuid::new_v4(), &incomplete)
                .unwrap_err(),
            AppError::NotEntitled
        );
        incomplete.plan = Some(PlanRef {
            id: Uuid::new_v4(),
            key: "local".into(),
            name: "Local".into(),
            is_trial: false,
        });
        incomplete.ends_at = None;
        assert_eq!(
            EntitledPlan::from_entitlements(OrganizationId::new(), Uuid::new_v4(), &incomplete)
                .unwrap_err(),
            AppError::NotEntitled
        );
        incomplete.ends_at = Some(OffsetDateTime::now_utc());
        incomplete.subscription_id = None;
        assert_eq!(
            EntitledPlan::from_entitlements(OrganizationId::new(), Uuid::new_v4(), &incomplete)
                .unwrap_err(),
            AppError::NotEntitled
        );
    }

    #[test]
    fn rechaza_plan_no_representable_en_v1() {
        let mut foreign = entitlements(true);
        foreign.plan = Some(PlanRef {
            id: Uuid::new_v4(),
            key: "pos-basico".into(),
            name: "POS Básico".into(),
            is_trial: false,
        });
        assert_eq!(
            EntitledPlan::from_entitlements(OrganizationId::new(), Uuid::new_v4(), &foreign)
                .unwrap_err(),
            AppError::UnsupportedPlan
        );
    }

    #[test]
    fn rechaza_capacidad_no_representable_en_v1() {
        let mut foreign = entitlements(true);
        foreign.features.insert("unlimited.users".to_owned(), true);
        assert_eq!(
            EntitledPlan::from_entitlements(OrganizationId::new(), Uuid::new_v4(), &foreign)
                .unwrap_err(),
            AppError::UnsupportedFeature
        );
    }

    #[test]
    fn omite_capacidades_desactivadas_y_exige_alguna_activa() {
        let mut disabled = entitlements(true);
        for enabled in disabled.features.values_mut() {
            *enabled = false;
        }
        assert_eq!(
            EntitledPlan::from_entitlements(OrganizationId::new(), Uuid::new_v4(), &disabled)
                .unwrap_err(),
            AppError::NotEntitled
        );
        let mut mixed = entitlements(true);
        mixed.features.insert("sales".to_owned(), false);
        let entitled =
            EntitledPlan::from_entitlements(OrganizationId::new(), Uuid::new_v4(), &mixed).unwrap();
        assert_eq!(entitled.features, vec!["catalog".to_owned()]);
    }
}
