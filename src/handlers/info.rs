use std::path::PathBuf;

use super::{
    result::PluginResult,
    shared_structs::{MountPoint, Named},
};
use crate::state::{GitvolState, RepoStatus, Volume};
use anyhow::{Context, Result};
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use log::{debug, kv};
use serde::Serialize;
use serde_json::json;
use tokio::join;

pub async fn capabilities_handler() -> impl IntoResponse {
    debug!("request capabilities");
    Json(json!({ "Capabilities": { "Scope": "global" } }))
}

async fn read_repo(name: &str, state: &GitvolState) -> Result<(Option<PathBuf>, RepoStatus)> {
    debug!(name; "Getting repo info.");
    let (volumes, repos) = join!(state.volumes.read(), state.repos.read());

    let Volume { path, hash, .. } = volumes.get(name).ok_or_else(|| {
        anyhow::anyhow!(
            "volume named {} has been deleted or has not yet been created",
            &name
        )
    })?;

    let repo_info = repos.get(hash);
    let status = if let Some(repo_info) = repo_info {
        let ri = repo_info.read().await;
        ri.status.clone()
    } else {
        RepoStatus::Created
    };

    let mountpoint = if status == RepoStatus::Created {
        None
    } else {
        Some(path.clone())
    };

    debug!(name, status, path = mountpoint.as_ref().map(|v| v.to_str());
        "Successfully got repo info.",
    );

    Ok((mountpoint, status))
}

pub async fn path_handler(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> PluginResult<MountPoint> {
    debug!(name; "Getting path for volume");
    let (mountpoint, _) = read_repo(&name, &state)
        .await
        .context("Getting repo info for path acquisition")?;

    debug!(name, mountpoint = format!("{:?}", mountpoint); "repo path information");
    Ok(MountPoint { mountpoint })
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

pub async fn get_handler(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> PluginResult<GetResponse> {
    debug!("Getting information for volume with the name {}", name);
    let (mountpoint, status) = read_repo(&name, &state)
        .await
        .context("Getting repo info for volume information")?;

    debug!(name, status, mountpoint = kv::Value::from_debug(&mountpoint); "repo information");

    Ok(GetResponse {
        volume: GetMountPoint {
            name,
            mountpoint,
            status,
        },
    })
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

pub async fn list_handler(State(state): State<GitvolState>) -> PluginResult<ListResponse> {
    debug!("Getting the list of volumes");
    let volumes = state.volumes.read().await;
    let volumes: Vec<ListMountPoint> = volumes
        .values()
        .into_iter()
        .map(|Volume { name, path, .. }: &Volume| ListMountPoint {
            name: name.clone(),
            mountpoint: Some(path.clone()),
        })
        .collect();

    debug!(count = volumes.len(); "volumes list");

    Ok(ListResponse { volumes })
}
