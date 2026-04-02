use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    hyprbridge_daemon::serve_default().await
}
