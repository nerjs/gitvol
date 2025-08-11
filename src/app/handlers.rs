use anyhow::Context;
use axum::{Json, extract::State};
use log::{debug, kv, warn};
use serde_json::json;
use tokio::fs;

use crate::{
    git,
    state::{GitvolState, Repo, RepoStatus},
};

use super::shared::*;

pub(super) async fn activate_plugin() -> Result {
    debug!("Initiating plugin activation.");
    Ok(Json(json!({ "Implements": ["VolumeDriver"] })))
}

pub(super) async fn capabilities_handler() -> Result {
    debug!("Retrieving plugin capabilities.");
    Ok(Json(json!({ "Capabilities": { "Scope": "global" } })))
}

pub(super) async fn get_volume_path(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> Result<OptionalMp> {
    debug!(name; "Retrieving path for volume.");
    let Some(volume) = state.read(&name).await else {
        log::warn!(name; "Volume not found.");
        return Ok(OptionalMp { mountpoint: None });
    };

    let mountpoint = volume.path.clone();

    debug!(name, mountpoint = format!("{:?}", mountpoint); "Retrieved volume path information");
    Ok(OptionalMp { mountpoint })
}

pub(super) async fn get_volume(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> Result<GetResponse> {
    debug!(name; "Retrieving information for volume.");
    let volume = state
        .read(&name)
        .await
        .with_context(|| format!("Failed to read volume '{}'. Volume not exists", name))?;

    let mountpoint = volume.path.clone();
    debug!(name, status = &volume.status, mountpoint = kv::Value::from_debug(&mountpoint); "Retrieved volume information");

    Ok(GetResponse {
        volume: GetMp {
            name,
            mountpoint,
            status: volume.status.clone(),
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

    debug!(count = volumes.len(); "Retrieved volumes list.");

    Ok(ListResponse { volumes })
}

pub(super) async fn create_volume(
    State(state): State<GitvolState>,
    Json(RawCreateRequest { name, opts }): Json<RawCreateRequest>,
) -> Result<Empty> {
    debug!(name; "Attempting to create volume.");
    let repo: Repo = opts
        .try_into()
        .context("Failed to parse repository options")?;
    git::parse_url(&repo.url).context("Wrong or unsupported url format")?;

    state
        .create(&name, repo)
        .await
        .context("Failed to create volume")?;
    debug!(name; "Volume created successfully.");
    Ok(Empty {})
}

pub(super) async fn remove_volume(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> Result<Empty> {
    debug!(name; "Attempting to remove volume.");
    let mut volumes = state.write_map().await;

    let Some(volume) = volumes.get(&name) else {
        warn!(name; "Volume not found.");
        return Ok(Empty {});
    };

    let volume = volume.read().await;
    remove_dir_if_exists(volume.path.clone())
        .await
        .with_context(|| format!("Failed to remove directory for volume '{}'", name))?;
    drop(volume);
    volumes.remove(&name);

    debug!(name; "Volume removed successfully.");
    Ok(Empty {})
}

pub(super) async fn mount_volume_to_container(
    State(state): State<GitvolState>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> Result<Mp> {
    debug!(name, id; "Attempting to mount volume.");
    let mut volume = state
        .write(&name)
        .await
        .with_context(|| format!("Failed to mount volume '{}': not found", name))?;

    if let Some(path) = volume.path.clone() {
        debug!(name, id; "Repository already cloned.");
        if volume.repo.refetch {
            debug!(name, id; "Attempting to refetch repository.");
            git::refetch(&path)
                .await
                .with_context(|| format!("Failed to refetch repository '{}'", volume.repo.url))?;
        }
        volume.containers.insert(id.clone());
        return Ok(Mp {
            mountpoint: path.clone(),
        });
    }

    let path = state.path.join(volume.repo.hash());
    if path.exists() {
        debug!(name, id; "Repository directory already exists. Remooving");
        fs::remove_dir_all(&path)
            .await
            .context("Removing trash repositiry dir")?;
    }
    git::clone(&path, &volume.repo)
        .await
        .with_context(|| format!("Failed to clone repository '{}'", volume.repo.url))?;

    volume.containers.insert(id.clone());
    volume.path = Some(path.clone());
    volume.status = RepoStatus::Clonned;

    debug!(name, id; "Volume mounted successfully.");
    Ok(Mp { mountpoint: path })
}

pub(super) async fn unmount_volume_by_container(
    State(state): State<GitvolState>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> Result<Empty> {
    debug!(name, id; "Attempting to unmount volume.");
    let Some(mut volume) = state.write(&name).await else {
        warn!(name, id; "Volume not found.");
        return Ok(Empty {});
    };

    volume.containers.remove(&id);

    if !volume.containers.is_empty() {
        debug!(name, id, container_count = volume.containers.len(); "Volume still in use by containers.");
        return Ok(Empty {});
    }

    volume.status = RepoStatus::Cleared;
    remove_dir_if_exists(volume.path.clone())
        .await
        .with_context(|| {
            format!(
                "Failed to remove repository directory for volume '{}'",
                name
            )
        })?;
    volume.path = None;

    debug!(name, id; "Volume unmounted successfully.");
    Ok(Empty {})
}
