mod handlers;
mod shared;
#[cfg(test)]
mod tests;

use crate::state::GitvolState;
use axum::{Router, routing::post};
use handlers::*;

pub fn create(state: GitvolState) -> Router {
    Router::new()
        .route("/Plugin.Activate", post(activate_plugin))
        .route("/VolumeDriver.Capabilities", post(capabilities_handler))
        .route("/VolumeDriver.Path", post(get_volume_path))
        .route("/VolumeDriver.Get", post(get_volume))
        .route("/VolumeDriver.List", post(list_volumes))
        .route("/VolumeDriver.Create", post(create_volume))
        .route("/VolumeDriver.Remove", post(remove_volume))
        .route("/VolumeDriver.Mount", post(mount_volume_to_container))
        .route("/VolumeDriver.Unmount", post(unmount_volume_by_container))
        .with_state(state)
}
