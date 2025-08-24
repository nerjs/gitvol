mod handlers;
mod shared;
#[cfg(test)]
mod tests;

use crate::state::GitvolState;
use axum::{
    Router,
    body::Bytes,
    extract::Request,
    http::{HeaderValue, StatusCode, header::CONTENT_TYPE},
    middleware::{self, Next},
    response::Response,
    routing::post,
};
use handlers::*;
use log::{debug, kv};

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
        .layer(middleware::from_fn(transform_headers))
        .with_state(state)
}

async fn transform_headers(mut request: Request, next: Next) -> Response {
    let mut headers = request.headers_mut();
    headers.append(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let mut response = next.run(request).await;
    let mut response_headers = response.headers_mut();
    response_headers.append(
        CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.docker.plugin.v1+json"),
    );

    response
}
