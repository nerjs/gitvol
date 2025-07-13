use super::result::PluginResult;
use axum::{Json, response::IntoResponse};
use serde_json::json;
use tracing::debug;

pub async fn activate_plugin() -> PluginResult<impl IntoResponse> {
    debug!("activate plugin");
    Ok(Json(json!({ "Implements": ["VolumeDriver"] })))
}
