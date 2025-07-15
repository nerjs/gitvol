mod handlers;
mod state;

use anyhow::{Context, Result};
use axum::{Router, routing::post, serve};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    handlers::{
        activate::activate_plugin,
        info::{capabilities_handler, get_handler, list_handler, path_handler},
        workers::{create_handler, mount_handler, remove_handler, unmount_handler},
    },
    state::GitvolState,
};

fn create_app(state: GitvolState) -> Router {
    Router::new()
        .route("/Plugin.Activate", post(activate_plugin))
        .route("/VolumeDriver.Capabilities", post(capabilities_handler))
        .route("/VolumeDriver.Get", post(get_handler))
        .route("/VolumeDriver.List", post(list_handler))
        .route("/VolumeDriver.Path", post(path_handler))
        .route("/VolumeDriver.Create", post(create_handler))
        .route("/VolumeDriver.Remove", post(remove_handler))
        .route("/VolumeDriver.Mount", post(mount_handler))
        .route("/VolumeDriver.Unmount", post(unmount_handler))
        .with_state(state)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let volumes_dir = std::env::current_dir()
        .context("getting the current directory to create a volumes directory")?
        .join("gitvol_volumes");

    let state = GitvolState::new(volumes_dir);
    let app = create_app(state);
    let listener = TcpListener::bind("127.0.0.1:5432").await?;
    info!("listening on {}", listener.local_addr().unwrap());

    serve(listener, app).await?;

    Ok(())
}
