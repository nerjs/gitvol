use std::{path::PathBuf, str::FromStr};

use super::{
    result::PluginResult,
    shared_structs::{MountPoint, Named, NamedMountPoint},
};
use crate::state::GitvolState;
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::json;
use tracing::debug;

pub async fn capabilities_handler() -> impl IntoResponse {
    Json(json!({ "Capabilities": { "Scope": "global" } }))
}

#[tracing::instrument(skip(_state))]
pub async fn path_handler(
    State(_state): State<GitvolState>,
    Json(_req): Json<Named>,
) -> PluginResult<MountPoint> {
    debug!("path handler");

    Ok(PathBuf::from_str("/fff").unwrap().into())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetResponse {
    pub volume: NamedMountPoint,
}

impl IntoResponse for GetResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

pub async fn get_handler(
    State(_state): State<GitvolState>,
    Json(_req): Json<Named>,
) -> PluginResult<GetResponse> {
    Ok(GetResponse {
        volume: NamedMountPoint {
            name: "()".into(),
            mountpoint: None,
        },
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListResponse {
    pub volumes: Vec<NamedMountPoint>,
}

impl IntoResponse for ListResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

pub async fn list_handler(State(_state): State<GitvolState>) -> PluginResult<ListResponse> {
    Ok(ListResponse { volumes: vec![] })
}
