use super::{
    result::PluginResult,
    shared_structs::{MountPoint, Named},
};
use crate::{
    bail_into,
    state::{GitvolState, RepoStatus},
};
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use log::{debug, kv};
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;

pub async fn capabilities_handler() -> impl IntoResponse {
    debug!("Retrieving plugin capabilities");
    Json(json!({ "Capabilities": { "Scope": "global" } }))
}

pub async fn path_handler(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> PluginResult<MountPoint> {
    debug!(name; "Retrieving path for volume.");
    let Some(volume) = state.read(&name).await else {
        log::warn!(name; "Volume not found.");
        return Ok(MountPoint { mountpoint: None });
    };

    let mountpoint = volume.path.clone();

    debug!(name, mountpoint = format!("{:?}", mountpoint); "Retrieved volume path information");
    Ok(MountPoint { mountpoint })
}

pub async fn get_handler(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> PluginResult<GetResponse> {
    debug!(name; "Retrieving information for volume");
    let Some(volume) = state.read(&name).await else {
        bail_into!("Failed to find volume '{}'", name);
    };

    let mountpoint = volume.path.clone();

    debug!(name, status = &volume.status, mountpoint = kv::Value::from_debug(&mountpoint); "Retrieved volume information");

    Ok(GetResponse {
        volume: GetMountPoint {
            name,
            mountpoint,
            status: volume.status.clone(),
        },
    })
}

pub async fn list_handler(State(state): State<GitvolState>) -> PluginResult<ListResponse> {
    debug!("Retrieving list of volumes");
    let map_volumes = state.read_map().await;

    let mut volumes: Vec<ListMountPoint> = Vec::with_capacity(map_volumes.len());

    for volume in map_volumes.clone().into_values() {
        let volume = volume.read().await;
        volumes.push(ListMountPoint {
            name: volume.name.clone(),
            mountpoint: volume.path.clone(),
        });
    }

    debug!(count = volumes.len(); "Retrieved volumes list");

    Ok(ListResponse { volumes })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMountPoint {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<PathBuf>,
    pub status: RepoStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetResponse {
    pub volume: GetMountPoint,
}

impl IntoResponse for GetResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListMountPoint {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListResponse {
    pub volumes: Vec<ListMountPoint>,
}

impl IntoResponse for ListResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
