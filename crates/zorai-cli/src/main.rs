use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    zorai_cli::run().await
}
