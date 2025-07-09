use anyhow::Result;
mod fs_manager;
mod global_config;
mod mount_config;
mod store;

#[tokio::main]
async fn main() -> Result<()> {
    Ok(())
}
