#[tokio::main]
async fn main() {
    if let Err(error) = hiqnet_platform_api::run().await {
        eprintln!("hiqnet-platform-api failed: {error}");
        std::process::exit(1);
    }
}
