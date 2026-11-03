use crate::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Planes comerciales aprobados. Trial dura 30 días y habilita las capacidades
/// de Local. Los precios siguen pendientes (D-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanName {
    Trial,
    Local,
    Pro,
    Enterprise,
}

impl PlanName {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanName::Trial => "trial",
            PlanName::Local => "local",
            PlanName::Pro => "pro",
            PlanName::Enterprise => "enterprise",
        }
    }

    pub fn parse(value: &str) -> DomainResult<Self> {
        match value {
            "trial" => Ok(Self::Trial),
            "local" => Ok(Self::Local),
            "pro" => Ok(Self::Pro),
            "enterprise" => Ok(Self::Enterprise),
            _ => Err(DomainError::InvalidPlan),
        }
    }
}

/// Capacidades comerciales con claves estables. Deben coincidir con el catálogo
/// que el POS verifica localmente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKey {
    Catalog,
    Sales,
    Inventory,
    Purchasing,
    Expenses,
    Reports,
    LocalBackups,
    CloudSync,
    RemoteAdmin,
    EnterpriseIntegrations,
}

impl FeatureKey {
    pub fn as_str(self) -> &'static str {
        match self {
            FeatureKey::Catalog => "catalog",
            FeatureKey::Sales => "sales",
            FeatureKey::Inventory => "inventory",
            FeatureKey::Purchasing => "purchasing",
            FeatureKey::Expenses => "expenses",
            FeatureKey::Reports => "reports",
            FeatureKey::LocalBackups => "backups.local",
            FeatureKey::CloudSync => "sync.cloud",
            FeatureKey::RemoteAdmin => "admin.remote",
            FeatureKey::EnterpriseIntegrations => "enterprise.integrations",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value {
            "catalog" => Some(Self::Catalog),
            "sales" => Some(Self::Sales),
            "inventory" => Some(Self::Inventory),
            "purchasing" => Some(Self::Purchasing),
            "expenses" => Some(Self::Expenses),
            "reports" => Some(Self::Reports),
            "backups.local" => Some(Self::LocalBackups),
            "sync.cloud" => Some(Self::CloudSync),
            "admin.remote" => Some(Self::RemoteAdmin),
            "enterprise.integrations" => Some(Self::EnterpriseIntegrations),
            _ => None,
        }
    }
}

/// Plan con sus capacidades. Los límites numéricos siguen pendientes (D-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDefinition {
    pub plan: PlanName,
    pub display_name: &'static str,
    pub features: BTreeSet<FeatureKey>,
}

impl PlanDefinition {
    pub fn catalog() -> Vec<Self> {
        vec![
            Self {
                plan: PlanName::Trial,
                display_name: "Trial (30 días)",
                features: local_features(),
            },
            Self {
                plan: PlanName::Local,
                display_name: "Local",
                features: local_features(),
            },
            Self {
                plan: PlanName::Pro,
                display_name: "Pro",
                features: pro_features(),
            },
            Self {
                plan: PlanName::Enterprise,
                display_name: "Enterprise",
                features: enterprise_features(),
            },
        ]
    }
}

fn local_features() -> BTreeSet<FeatureKey> {
    [
        FeatureKey::Catalog,
        FeatureKey::Sales,
        FeatureKey::Inventory,
        FeatureKey::Purchasing,
        FeatureKey::Expenses,
        FeatureKey::Reports,
        FeatureKey::LocalBackups,
    ]
    .into_iter()
    .collect()
}

fn pro_features() -> BTreeSet<FeatureKey> {
    let mut features = local_features();
    features.insert(FeatureKey::CloudSync);
    features.insert(FeatureKey::RemoteAdmin);
    features
}

fn enterprise_features() -> BTreeSet<FeatureKey> {
    let mut features = pro_features();
    features.insert(FeatureKey::EnterpriseIntegrations);
    features
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogo_tiene_cuatro_planes_y_pro_incluye_sync() {
        let catalog = PlanDefinition::catalog();
        assert_eq!(catalog.len(), 4);
        let pro = catalog
            .iter()
            .find(|plan| plan.plan == PlanName::Pro)
            .unwrap();
        assert!(pro.features.contains(&FeatureKey::CloudSync));
        assert!(!catalog[1].features.contains(&FeatureKey::CloudSync));
    }

    #[test]
    fn parsea_planes_validos() {
        assert_eq!(PlanName::parse("trial").unwrap(), PlanName::Trial);
        assert!(PlanName::parse("free").is_err());
    }
}
