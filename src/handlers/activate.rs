use crate::state::GitvolState;

use super::result::PluginResult;
use anyhow::Context;
use axum::{Json, extract::State, response::IntoResponse};
use serde_json::json;
use tokio::fs;
use tracing::debug;

pub async fn activate_plugin(State(state): State<GitvolState>) -> PluginResult<impl IntoResponse> {
    debug!("Trying to activate the plugin");
    if !state.path.exists() {
        fs::create_dir_all(&state.path)
            .await
            .with_context(|| format!("create volumes directory [{:?}]", &state.path))?;
        debug!("volumes directory was created");
    }
    Ok(Json(json!({ "Implements": ["VolumeDriver"] })))
}
