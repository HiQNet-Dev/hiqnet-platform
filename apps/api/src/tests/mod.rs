//! Pruebas de la API organizadas por escenario.
//!
//! El preludio replica el que usaba el módulo `tests` inline: glob de la crate,
//! utilidades HTTP y aserciones. Cada escenario añade sólo sus fixtures.

use crate::*;
use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, header},
};
use base64::Engine as _;
use platform_application::enrollment::EnrollmentError;
use serde_json::{Value, json};
use std::time::Duration;
use tower::ServiceExt;

mod authentication;
mod commercial;
mod common;
mod control;
mod control_audit;
mod control_directory;
mod control_installations;
mod control_licensing;
mod control_organization_products;
mod enrollment;
mod licensing;
mod organizations;
mod routes;
mod security;
