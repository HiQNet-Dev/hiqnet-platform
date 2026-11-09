//! Casos de uso del registro de productos y proyectos.
//!
//! La autorización es deny-by-default y se evalúa aquí, antes de tocar
//! persistencia. Toda mutación se audita en la misma transacción del adaptador.

use async_trait::async_trait;
use platform_core::{
    ids::StaffAccountId,
    product::{
        LifecycleStatus, NewProduct, NewProject, Product, ProductChanges, ProductError,
        ProductResult, ProductType, Project, ProjectChanges, is_valid_key, validate_description,
        validate_name,
    },
    staff::{Capability, StaffRole},
};
use time::OffsetDateTime;

/// Actor staff ya autenticado por el extractor HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffActor {
    pub account_id: StaffAccountId,
    pub role: StaffRole,
}

impl StaffActor {
    pub fn require(&self, capability: Capability) -> ProductResult<()> {
        if self.role.has(capability) {
            Ok(())
        } else {
            Err(ProductError::Forbidden)
        }
    }
}

pub const DEFAULT_PAGE_LIMIT: u32 = 50;
pub const MAX_PAGE_LIMIT: u32 = 100;
pub const MAX_CURSOR_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[async_trait]
pub trait ProductRepository: Send + Sync {
    async fn list_products(
        &self,
        status: Option<LifecycleStatus>,
        product_type: Option<ProductType>,
        limit: u32,
        after: Option<&str>,
    ) -> ProductResult<Vec<Product>>;
    async fn get_product(&self, key: &str) -> ProductResult<Option<Product>>;
    async fn create_product(
        &self,
        actor: &StaffActor,
        product: &NewProduct,
        now: OffsetDateTime,
    ) -> ProductResult<Product>;
    async fn update_product(
        &self,
        actor: &StaffActor,
        key: &str,
        changes: &ProductChanges,
        now: OffsetDateTime,
    ) -> ProductResult<Product>;

    async fn list_projects(
        &self,
        product_key: Option<&str>,
        limit: u32,
        after: Option<&str>,
    ) -> ProductResult<Vec<Project>>;
    async fn get_project(&self, key: &str) -> ProductResult<Option<Project>>;
    async fn create_project(
        &self,
        actor: &StaffActor,
        project: &NewProject,
        now: OffsetDateTime,
    ) -> ProductResult<Project>;
    async fn update_project(
        &self,
        actor: &StaffActor,
        key: &str,
        changes: &ProjectChanges,
        now: OffsetDateTime,
    ) -> ProductResult<Project>;
}

pub struct ProductService<R: ProductRepository> {
    repository: R,
}

impl<R: ProductRepository> ProductService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn list_products(
        &self,
        actor: &StaffActor,
        status: Option<LifecycleStatus>,
        product_type: Option<ProductType>,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> ProductResult<Page<Product>> {
        actor.require(Capability::ProductsRead)?;
        let (limit, cursor) = page_params(limit, cursor)?;
        let mut items = self
            .repository
            .list_products(status, product_type, limit + 1, cursor.as_deref())
            .await?;
        let next_cursor = take_page_cursor(&mut items, limit, |product| product.key.clone());
        Ok(Page { items, next_cursor })
    }

    pub async fn get_product(&self, actor: &StaffActor, key: &str) -> ProductResult<Product> {
        actor.require(Capability::ProductsRead)?;
        self.repository
            .get_product(key)
            .await?
            .ok_or(ProductError::NotFound)
    }

    pub async fn create_product(
        &self,
        actor: &StaffActor,
        product: NewProduct,
        now: OffsetDateTime,
    ) -> ProductResult<Product> {
        actor.require(Capability::ProductsManage)?;
        validate_new_product(&product)?;
        self.repository.create_product(actor, &product, now).await
    }

    pub async fn update_product(
        &self,
        actor: &StaffActor,
        key: &str,
        changes: ProductChanges,
        now: OffsetDateTime,
    ) -> ProductResult<Product> {
        actor.require(Capability::ProductsManage)?;
        validate_product_changes(&changes)?;
        self.repository
            .update_product(actor, key, &changes, now)
            .await
    }

    pub async fn list_projects(
        &self,
        actor: &StaffActor,
        product_key: Option<String>,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> ProductResult<Page<Project>> {
        actor.require(Capability::ProjectsRead)?;
        if let Some(key) = product_key.as_deref() {
            if !is_valid_key(key) {
                return Err(ProductError::InvalidInput);
            }
        }
        let (limit, cursor) = page_params(limit, cursor)?;
        let mut items = self
            .repository
            .list_projects(product_key.as_deref(), limit + 1, cursor.as_deref())
            .await?;
        let next_cursor = take_page_cursor(&mut items, limit, |project| project.key.clone());
        Ok(Page { items, next_cursor })
    }

    pub async fn get_project(&self, actor: &StaffActor, key: &str) -> ProductResult<Project> {
        actor.require(Capability::ProjectsRead)?;
        self.repository
            .get_project(key)
            .await?
            .ok_or(ProductError::NotFound)
    }

    pub async fn create_project(
        &self,
        actor: &StaffActor,
        project: NewProject,
        now: OffsetDateTime,
    ) -> ProductResult<Project> {
        actor.require(Capability::ProjectsManage)?;
        validate_new_project(&project)?;
        self.repository.create_project(actor, &project, now).await
    }

    pub async fn update_project(
        &self,
        actor: &StaffActor,
        key: &str,
        changes: ProjectChanges,
        now: OffsetDateTime,
    ) -> ProductResult<Project> {
        actor.require(Capability::ProjectsManage)?;
        validate_project_changes(&changes)?;
        self.repository
            .update_project(actor, key, &changes, now)
            .await
    }
}

fn page_params(limit: Option<u32>, cursor: Option<String>) -> ProductResult<(u32, Option<String>)> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
    if let Some(cursor) = cursor.as_deref() {
        if cursor.is_empty() || cursor.len() > MAX_CURSOR_LEN {
            return Err(ProductError::InvalidInput);
        }
    }
    Ok((limit, cursor))
}

fn take_page_cursor<T>(
    items: &mut Vec<T>,
    limit: u32,
    key: impl Fn(&T) -> String,
) -> Option<String> {
    if items.len() as u32 > limit {
        items.pop();
        items.last().map(key)
    } else {
        None
    }
}

fn validate_new_product(product: &NewProduct) -> ProductResult<()> {
    if !is_valid_key(&product.key) {
        return Err(ProductError::InvalidInput);
    }
    validate_name(&product.name)?;
    validate_description(&product.description)
}

fn validate_product_changes(changes: &ProductChanges) -> ProductResult<()> {
    if let Some(name) = changes.name.as_deref() {
        validate_name(name)?;
    }
    if let Some(description) = changes.description.as_deref() {
        validate_description(description)?;
    }
    Ok(())
}

fn validate_new_project(project: &NewProject) -> ProductResult<()> {
    if !is_valid_key(&project.key) {
        return Err(ProductError::InvalidInput);
    }
    validate_name(&project.name)?;
    validate_description(&project.description)
}

fn validate_project_changes(changes: &ProjectChanges) -> ProductResult<()> {
    if let Some(name) = changes.name.as_deref() {
        validate_name(name)?;
    }
    if let Some(description) = changes.description.as_deref() {
        validate_description(description)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::product::{MAX_DESCRIPTION_LEN, NewProduct};

    fn actor(role: StaffRole) -> StaffActor {
        StaffActor {
            account_id: StaffAccountId::new(),
            role,
        }
    }

    #[test]
    fn autorizacion_deny_by_default_en_el_servicio() {
        assert!(
            actor(StaffRole::PlatformAdmin)
                .require(Capability::ProductsManage)
                .is_ok()
        );
        assert_eq!(
            actor(StaffRole::ReadOnly).require(Capability::ProductsManage),
            Err(ProductError::Forbidden)
        );
        assert_eq!(
            actor(StaffRole::Operator).require(Capability::ProjectsManage),
            Err(ProductError::Forbidden)
        );
        assert!(
            actor(StaffRole::Operator)
                .require(Capability::ProjectsRead)
                .is_ok()
        );
    }

    #[test]
    fn pagina_toma_solo_el_limite_y_devuelve_cursor() {
        let mut items = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let cursor = take_page_cursor(&mut items, 2, |value| value.clone());
        assert_eq!(items, vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(cursor.as_deref(), Some("b"));

        let mut exact = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(take_page_cursor(&mut exact, 2, |value| value.clone()), None);
    }

    #[test]
    fn parametros_de_pagina_se_acotan_y_validan() {
        assert_eq!(page_params(None, None).unwrap().0, DEFAULT_PAGE_LIMIT);
        assert_eq!(page_params(Some(10_000), None).unwrap().0, MAX_PAGE_LIMIT);
        assert_eq!(page_params(Some(0), None).unwrap().0, 1);
        assert!(page_params(Some(10), Some(String::new())).is_err());
        assert!(page_params(Some(10), Some("x".repeat(MAX_CURSOR_LEN + 1))).is_err());
    }

    #[test]
    fn validacion_de_producto_nuevo() {
        let valid = NewProduct {
            key: "pos".into(),
            name: "HiQNet POS".into(),
            description: String::new(),
            status: LifecycleStatus::Active,
            product_type: ProductType::Desktop,
        };
        assert_eq!(validate_new_product(&valid), Ok(()));

        let bad_key = NewProduct {
            key: "POS".into(),
            ..valid.clone()
        };
        assert_eq!(
            validate_new_product(&bad_key),
            Err(ProductError::InvalidInput)
        );

        let bad_description = NewProduct {
            description: "x".repeat(MAX_DESCRIPTION_LEN + 1),
            ..valid
        };
        assert_eq!(
            validate_new_product(&bad_description),
            Err(ProductError::InvalidInput)
        );
    }
}
