//! Registro universal de productos y proyectos de HiQNet.
//!
//! Genérico y multiproducto: HiQNet POS es sólo la primera entrada. No contiene
//! reglas comerciales (precios, planes, entitlements) ni depende del POS.

use crate::ids::{ProductId, ProjectId};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProductError {
    #[error("producto o proyecto no encontrado")]
    NotFound,
    #[error("capability insuficiente para la operación")]
    Forbidden,
    #[error("clave o nombre inválidos")]
    InvalidInput,
    #[error("la clave ya existe")]
    Conflict,
    #[error("no fue posible completar la operación")]
    Storage,
}

pub type ProductResult<T> = Result<T, ProductError>;

pub const MAX_KEY_LEN: usize = 64;
pub const MAX_NAME_LEN: usize = 200;
pub const MAX_DESCRIPTION_LEN: usize = 2000;

/// Clave técnica estable: minúsculas ASCII, dígitos y guiones; 1–64 caracteres;
/// debe empezar y terminar en alfanumérico. Es inmutable una vez creada.
pub fn is_valid_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_KEY_LEN {
        return false;
    }
    let is_edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if !is_edge(bytes[0]) || !is_edge(bytes[bytes.len() - 1]) {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

pub fn validate_name(name: &str) -> ProductResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_LEN {
        return Err(ProductError::InvalidInput);
    }
    Ok(())
}

pub fn validate_description(description: &str) -> ProductResult<()> {
    if description.chars().count() > MAX_DESCRIPTION_LEN {
        return Err(ProductError::InvalidInput);
    }
    Ok(())
}

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ProductError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(ProductError::InvalidInput),
                }
            }
        }
    };
}

text_enum!(LifecycleStatus {
    Development => "development",
    Active => "active",
    Maintenance => "maintenance",
    Deprecated => "deprecated",
    Retired => "retired",
});

text_enum!(ProductType {
    Desktop => "desktop",
    Saas => "saas",
    Web => "web",
    Mobile => "mobile",
    Service => "service",
    Integration => "integration",
    Internal => "internal",
});

text_enum!(ProjectType {
    Service => "service",
    Application => "application",
    Integration => "integration",
    Infrastructure => "infrastructure",
    Initiative => "initiative",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub id: ProductId,
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: LifecycleStatus,
    pub product_type: ProductType,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub key: String,
    pub name: String,
    pub description: String,
    pub project_type: ProjectType,
    pub status: LifecycleStatus,
    pub product_id: Option<ProductId>,
    pub product_key: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProduct {
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: LifecycleStatus,
    pub product_type: ProductType,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductChanges {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<LifecycleStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProject {
    pub key: String,
    pub name: String,
    pub description: String,
    pub project_type: ProjectType,
    pub status: LifecycleStatus,
    pub product_key: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectChanges {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<LifecycleStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claves_estables_validas_e_invalidas() {
        assert!(is_valid_key("pos"));
        assert!(is_valid_key("example-product"));
        assert!(is_valid_key("a"));
        assert!(!is_valid_key(""));
        assert!(!is_valid_key("Pos"));
        assert!(!is_valid_key("-pos"));
        assert!(!is_valid_key("pos-"));
        assert!(!is_valid_key("pos_uno"));
        assert!(!is_valid_key("pos uno"));
        assert!(!is_valid_key(&"a".repeat(MAX_KEY_LEN + 1)));
    }

    #[test]
    fn enums_sobreviven_al_texto() {
        assert_eq!(
            LifecycleStatus::Active
                .as_str()
                .parse::<LifecycleStatus>()
                .unwrap(),
            LifecycleStatus::Active
        );
        assert_eq!(ProductType::Desktop.to_string(), "desktop");
        assert_eq!(ProjectType::Infrastructure.to_string(), "infrastructure");
        assert!("unknown".parse::<LifecycleStatus>().is_err());
    }

    #[test]
    fn validacion_de_nombre_y_descripcion() {
        assert_eq!(validate_name("  HiQNet POS "), Ok(()));
        assert_eq!(validate_name("   "), Err(ProductError::InvalidInput));
        assert_eq!(
            validate_name(&"x".repeat(MAX_NAME_LEN + 1)),
            Err(ProductError::InvalidInput)
        );
        assert_eq!(validate_description("ok"), Ok(()));
        assert_eq!(
            validate_description(&"x".repeat(MAX_DESCRIPTION_LEN + 1)),
            Err(ProductError::InvalidInput)
        );
    }
}
