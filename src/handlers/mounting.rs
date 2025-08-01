use std::path::PathBuf;

use anyhow::{Context, Result};
use axum::{Json, extract::State};
use log::{debug, kv, warn};
use tokio::fs;

use super::{
    result::PluginResult,
    shared_structs::{Empty, MountPoint, NamedWID},
};
use crate::{
    git,
    state::{GitvolState, RepoStatus},
};

pub async fn mount_handler(
    State(state): State<GitvolState>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> PluginResult<MountPoint> {
    debug!(name, id; "Attempting to mount volume");
    let mut volume = state
        .write(&name)
        .await
        .with_context(|| format!("Failed to mount volume '{}': not found", name))?;

    if let Some(path) = volume.path.clone() {
        debug!(name, id; "Repository already cloned");
        if volume.repo.refetch {
            debug!(name, id; "Attempting to refetch repository");
            git::refetch(&path)
                .await
                .with_context(|| format!("Failed to refetch repository '{}'", volume.repo.url))?;
        }
        volume.containers.insert(id.clone());
        return Ok(MountPoint {
            mountpoint: Some(path.clone()),
        });
    }

    let path = state.path.join(volume.repo.hash());
    git::clone(&path, &volume.repo)
        .await
        .with_context(|| format!("Failed to clone repository '{}'", volume.repo.url))?;

    volume.containers.insert(id.clone());
    volume.path = Some(path.clone());
    volume.status = RepoStatus::Clonned;

    debug!(name, id; "Volume mounted successfully");
    Ok(MountPoint {
        mountpoint: Some(path),
    })
}

pub async fn unmount_handler(
    State(state): State<GitvolState>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> PluginResult<Empty> {
    debug!(name, id; "Attempting to unmount volume");
    let mut volumes = state.write_map().await;
    let Some(mut volume) = state.write(&name).await else {
        warn!(name, id; "Volume not found");
        return Ok(Empty);
    };

    volume.containers.remove(&id);
    if volume.containers.len() > 0 {
        debug!(name, id, container_count = volume.containers.len(); "Volume still in use by containers");
        return Ok(Empty);
    }
    remove_dir_if_exists(volume.path.clone())
        .await
        .with_context(|| {
            format!(
                "Failed to remove repository directory for volume '{}'",
                name
            )
        })?;
    volumes.remove(&name);

    debug!(name, id; "Volume unmounted successfully");
    Ok(Empty)
}

async fn remove_dir_if_exists(path: Option<PathBuf>) -> Result<()> {
    if let Some(path) = path {
        if path.exists() {
            debug!(path = kv::Value::from_debug(&path); "Attempting to remove directory");
            fs::remove_dir_all(&path)
                .await
                .with_context(|| format!("Failed to remove directory '{:?}'", path))?;
        }
    }

    Ok(())
}
