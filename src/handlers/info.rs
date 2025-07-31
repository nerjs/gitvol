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
use serde_json::{Value, json};
use std::path::PathBuf;

pub async fn capabilities_handler() -> Json<Value> {
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

#[cfg_attr(test, derive(PartialEq))]
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

#[cfg_attr(test, derive(PartialEq))]
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

#[cfg(test)]
mod tests {
    use crate::state::Repo;

    use super::*;

    const VOLUME_NAME: &str = "volume_name";
    const REPO_URL: &str = "https://example.com/repo.git";

    #[tokio::test]
    async fn capabilities_handler_success() {
        let Json(value) = capabilities_handler().await;
        assert_eq!(value, json!({ "Capabilities": { "Scope": "global" } }));
    }

    #[tokio::test]
    async fn path_handler_success() {
        let state = GitvolState::new("/tmp".into());
        _ = state
            .create(VOLUME_NAME, Repo::new(REPO_URL))
            .await
            .unwrap();
        state
            .set_path(VOLUME_NAME, "/tmp/test_volume")
            .await
            .unwrap();

        let request = Named::new(VOLUME_NAME);
        let result = path_handler(State(state), Json(request)).await;

        assert!(result.is_ok());
        let mount_point = result.unwrap();
        assert_eq!(
            mount_point.mountpoint,
            Some(PathBuf::from("/tmp/test_volume"))
        );
    }

    #[tokio::test]
    async fn path_handler_non_existent_volume() {
        let state = GitvolState::new("/tmp".into());

        let request = Named::new("non_existent_volume");

        let result = path_handler(State(state), Json(request)).await;

        assert!(result.is_ok());
        let mount_point = result.unwrap();
        assert_eq!(mount_point.mountpoint, None);
    }

    #[tokio::test]
    async fn get_handler_success() {
        let state = GitvolState::new("/tmp".into());
        _ = state
            .create(VOLUME_NAME, Repo::new(REPO_URL))
            .await
            .unwrap();
        state
            .set_path(VOLUME_NAME, "/tmp/test_volume")
            .await
            .unwrap();

        let request = Named::new(VOLUME_NAME);

        let result = get_handler(State(state), Json(request)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(
            response.volume,
            GetMountPoint {
                name: VOLUME_NAME.to_string(),
                mountpoint: Some(PathBuf::from("/tmp/test_volume")),
                status: RepoStatus::Created,
            }
        );
    }

    #[tokio::test]
    async fn get_handler_non_existent_volume() {
        let state = GitvolState::new("/tmp".into());

        let request = Named::new(VOLUME_NAME);

        let result = get_handler(State(state), Json(request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_handler_with_volumes() {
        let second_volume_name = format!("{}_2", VOLUME_NAME);
        let state = GitvolState::new("/tmp".into());
        _ = state
            .create(VOLUME_NAME, Repo::new(REPO_URL))
            .await
            .unwrap();
        _ = state
            .create(&second_volume_name, Repo::new(REPO_URL))
            .await
            .unwrap();
        state
            .set_path(VOLUME_NAME, "/tmp/test_volume")
            .await
            .unwrap();

        let result = list_handler(State(state)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.volumes.len(), 2);

        assert!(response.volumes.contains(&ListMountPoint {
            name: VOLUME_NAME.to_string(),
            mountpoint: Some(PathBuf::from("/tmp/test_volume")),
        }));
        assert!(response.volumes.contains(&ListMountPoint {
            name: second_volume_name,
            mountpoint: None,
        }));
    }

    #[tokio::test]
    async fn list_handler_empty() {
        let state = GitvolState::new("/tmp".into());
        let result = list_handler(State(state)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.volumes.is_empty());
    }
}
