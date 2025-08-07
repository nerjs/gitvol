mod app;
mod git;
mod macros;
mod state;

use anyhow::Context;
use axum::serve;
use log::{debug, info};
use tokio::{fs, net::TcpListener};

use crate::state::GitvolState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let volumes_dir = std::env::current_dir()
        .context("getting the current directory to create a volumes directory")?
        .join("gitvol_volumes");

    if !volumes_dir.exists() {
        fs::create_dir_all(&volumes_dir).await.with_context(|| {
            format!("Failed to create volumes directory at '{:?}'", &volumes_dir)
        })?;
        debug!(path = volumes_dir.to_str(); "Created volumes directory");
    }

    git::ensure_git_exists()
        .await
        .context("Failed check git exists")?;

    let state = GitvolState::new(volumes_dir);
    let app = app::create(state);
    let listener = TcpListener::bind("127.0.0.1:5432").await?;
    info!("listening on {}", listener.local_addr().unwrap());

    serve(listener, app).await?;

    Ok(())
}
