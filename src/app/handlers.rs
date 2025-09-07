use axum::{Json, extract::State};
use serde_json::json;
use tokio::fs;
use tracing::{debug, field, warn};

use crate::{domains::volume::Status, git, result::ErrorIoExt, state::GitvolState};

use super::shared::*;

pub(super) async fn activate_plugin() -> Result {
    debug!("Initiating plugin activation.");
    Ok(Json(json!({ "Implements": ["VolumeDriver"] })))
}

pub(super) async fn capabilities_handler() -> Result {
    debug!("Retrieving plugin capabilities.");
    Ok(Json(json!({ "Capabilities": { "Scope": "local" } })))
}

pub(super) async fn get_volume_path(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> Result<OptionalMp> {
    debug!(name, "Retrieving path for volume.");
    let Some(volume) = state.read(&name).await else {
        warn!(name, "Volume not found.");
        return Ok(OptionalMp { mountpoint: None });
    };

    let mountpoint = volume.path.clone();

    debug!(
        name,
        mountpoint = field::debug(&mountpoint),
        "Retrieved volume path information"
    );
    Ok(OptionalMp { mountpoint })
}

pub(super) async fn get_volume(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> Result<GetResponse> {
    debug!(name, "Retrieving information for volume.");
    let volume = state.try_read(&name).await?;

    let mountpoint = volume.path.clone();
    debug!(
        name,
        status = field::debug(&volume.status),
        mountpoint = field::debug(&mountpoint),
        "Retrieved volume information"
    );

    Ok(GetResponse {
        volume: GetMp {
            name,
            mountpoint,
            status: MpStatus {
                status: volume.status.clone(),
            },
        },
    })
}

pub(super) async fn list_volumes(State(state): State<GitvolState>) -> Result<ListResponse> {
    debug!("Retrieving list of volumes.");
    let map_volumes = state.read_map().await;

    let mut volumes: Vec<ListMp> = Vec::with_capacity(map_volumes.len());

    for volume in map_volumes.clone().into_values() {
        let volume = volume.read().await;
        volumes.push(ListMp {
            name: volume.name.clone(),
            mountpoint: volume.path.clone(),
        });
    }

    debug!(count = volumes.len(), "Retrieved volumes list.");

    Ok(ListResponse { volumes })
}

pub(super) async fn create_volume(
    State(state): State<GitvolState>,
    Json(RawCreateRequest { name, opts }): Json<RawCreateRequest>,
) -> Result<Empty> {
    state.create(&name, opts).await?;
    debug!(name, "Volume created successfully.");
    Ok(Empty {})
}

pub(super) async fn remove_volume(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> Result<Empty> {
    debug!(name, "Attempting to remove volume.");
    let mut volumes = state.write_map().await;

    let Some(volume) = volumes.get(&name) else {
        warn!(name, "Volume not found.");
        return Ok(Empty {});
    };

    let volume = volume.read().await;
    remove_dir_if_exists(volume.path.clone()).await?;
    drop(volume);
    volumes.remove(&name);

    debug!(name, "Volume removed successfully.");
    Ok(Empty {})
}

pub(super) async fn mount_volume_to_container(
    State(state): State<GitvolState>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> Result<Mp> {
    debug!(name, id, "Attempting to mount volume.");
    let mut volume = state.try_write(&name).await?;

    if let Some(path) = volume.path.clone() {
        debug!(name, id, "Repository already cloned.");
        if volume.repo.refetch {
            debug!(name, id, "Attempting to refetch repository.");
            git::refetch(&path).await?;
        }
        volume.containers.insert(id.clone());
        return Ok(Mp {
            mountpoint: path.clone(),
        });
    }

    let path = volume.create_path_from(&state.path);
    if path.exists() {
        debug!(name, id, "Repository directory already exists. Remooving");
        fs::remove_dir_all(&path).await.map_io_error(&path)?;
    }
    git::clone(&path, &volume.repo).await?;

    volume.containers.insert(id.clone());
    volume.status = Status::Clonned;

    debug!(name, id, "Volume mounted successfully.");
    Ok(Mp { mountpoint: path })
}

pub(super) async fn unmount_volume_by_container(
    State(state): State<GitvolState>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> Result<Empty> {
    debug!(name, id, "Attempting to unmount volume.");
    let Some(mut volume) = state.write(&name).await else {
        warn!(name, id, "Volume not found.");
        return Ok(Empty {});
    };

    volume.containers.remove(&id);

    if !volume.containers.is_empty() {
        debug!(
            name,
            container_count = volume.containers.len(),
            "Volume still in use by containers."
        );
        return Ok(Empty {});
    }

    volume.status = Status::Cleared;
    remove_dir_if_exists(volume.path.clone()).await?;
    volume.path = None;

    debug!(name, "Volume unmounted successfully.");
    Ok(Empty {})
}
