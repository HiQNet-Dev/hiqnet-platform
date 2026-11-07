fn main() {
    println!(
        "{}",
        hiqnet_platform_api::openapi_document()
            .to_pretty_json()
            .expect("el contrato OpenAPI debe serializarse")
    );
}
