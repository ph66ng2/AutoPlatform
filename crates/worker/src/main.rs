#[path = "../../wired_modules.rs"]
mod modules;

use ap_infra::{load, serve, ServiceKind};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = load(ServiceKind::Worker)?;
    serve(config, modules::NAMES).await?;
    Ok(())
}
