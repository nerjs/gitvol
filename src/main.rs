mod handlers;
mod macros;
mod middlewares;
mod state;

use anyhow::Context;
use axum::{Router, middleware, routing::post, serve};
use log::info;
use tokio::net::TcpListener;

use crate::{
    handlers::{
        activate::activate_plugin,
        info::{capabilities_handler, get_handler, list_handler, path_handler},
        workers::{create_handler, mount_handler, remove_handler, unmount_handler},
    },
    middlewares::save_middleware,
    state::GitvolState,
};

fn create_app(state: GitvolState) -> Router {
    let router_wit_save_middleware = Router::new()
        .route("/Plugin.Activate", post(activate_plugin))
        .route("/VolumeDriver.Create", post(create_handler))
        .route("/VolumeDriver.Remove", post(remove_handler))
        .route("/VolumeDriver.Mount", post(mount_handler))
        .route("/VolumeDriver.Unmount", post(unmount_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            save_middleware,
        ));

    Router::new()
        .route("/VolumeDriver.Capabilities", post(capabilities_handler))
        .route("/VolumeDriver.Get", post(get_handler))
        .route("/VolumeDriver.List", post(list_handler))
        .route("/VolumeDriver.Path", post(path_handler))
        .merge(router_wit_save_middleware)
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
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
