#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    backend::config::run().await
}
