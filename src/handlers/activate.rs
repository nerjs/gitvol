use std::sync::Arc;

use crate::state::GitvolState;

use super::result::PluginResult;
use anyhow::Context;
use axum::{Json, extract::State, response::IntoResponse};
use log::debug;
use serde_json::json;
use tokio::fs;

pub async fn activate_plugin(State(state): State<GitvolState>) -> PluginResult<impl IntoResponse> {
    debug!("Trying to activate the plugin");
    state.restore().await.context("Failed restore state")?;
    // if !state.path.exists() {
    //     fs::create_dir_all(&state.path)
    //         .await
    //         .with_context(|| format!("create volumes directory [{:?}]", &state.path))?;
    //     debug!(path = state.path.to_str(); "volumes directory was created");
    // }
    Ok(Json(json!({ "Implements": ["VolumeDriver"] })))
}
