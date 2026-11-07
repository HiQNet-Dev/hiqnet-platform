//! Dominio puro de HiQNet Platform.
//!
//! Este crate no conoce Axum, SQLx ni PostgreSQL. Modela el catálogo de planes,
//! el registro de instalaciones y el contrato de autorización comercial que el
//! POS verifica localmente.

pub mod auth;
pub mod commercial;
pub mod error;
pub mod ids;
pub mod installation;
pub mod license;
pub mod plan;

pub use error::{DomainError, DomainResult};
