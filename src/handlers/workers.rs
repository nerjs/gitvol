use std::{collections::HashSet, path::PathBuf, sync::Arc};

use super::{
    result::PluginResult,
    shared_structs::{Empty, MountPoint, NamedWID},
};
use crate::{
    bail_into, git,
    state::{GitvolState, RepoInfo, RepoStatus, Volume2},
};
use anyhow::{Context, Result};
use axum::{Json, extract::State};
use log::{debug, info, kv::Value, warn};
use tokio::{fs, sync::RwLock};

pub async fn mount_handler(
    State(state): State<GitvolState>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> PluginResult<MountPoint> {
    debug!(name, id; "trying to mount the volume");
    let volumes = state.volumes2.read().await;

    let Some(Volume2 {
        path, hash, repo, ..
    }) = volumes.get(&name)
    else {
        bail_into!("volume named '{}' not found", name);
    };

    let repo_info = {
        let mut repos = state.repos.write().await;
        match repos.get(hash) {
            Some(repo_info) => repo_info.clone(),
            None => {
                debug!(name, id, hash; "missing repo info creating");
                let repo_info = Arc::new(RwLock::new(RepoInfo {
                    status: RepoStatus::Created,
                    containers: HashSet::new(),
                }));
                repos.insert(hash.to_string(), repo_info.clone());
                repo_info
            }
        }
    };

    let mut repo_info = repo_info.write().await;

    if repo_info.status == RepoStatus::Clonned {
        if repo.refetch {
            git::refetch(path)
                .await
                .with_context(|| format!("refetch {}", &repo.url))?;
            info!(name, url = &repo.url; "repository was updated (refetch)");
        } else {
            debug!(name, id, url = &repo.url; "The repository is already cloned and does not need to be updated");
        }
    } else {
        if path.exists() {
            warn!(name, path = Value::from_debug(path); "repository path already exists. delete");
            fs::remove_dir_all(path)
                .await
                .context("removing repository directory")?;
        }

        git::clone(path, &repo)
            .await
            .with_context(|| format!("clone {}", &repo.url))?;
        info!(name, url = &repo.url; "repository was cloned");
        repo_info.status = RepoStatus::Clonned;
    }

    if !repo_info.containers.contains(&id) {
        repo_info.containers.insert(id);
        debug!(name, containers_count = repo_info.containers.len(); "container added to dependent")
    }

    Ok(MountPoint {
        mountpoint: Some(path.clone()),
    })
}

async fn trying_remove_path(name: &str, path: &PathBuf) -> Result<()> {
    if path.exists() {
        debug!(name, path = Value::from_debug(path); "remove repo path");
        fs::remove_dir_all(path)
            .await
            .context("failed remove repo path")?;
    }
    Ok(())
}

pub async fn unmount_handler(
    State(state): State<GitvolState>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> PluginResult<Empty> {
    debug!(name, id; "trying to unmount the volume");
    let volumes = state.volumes2.read().await;

    let Some(Volume2 { path, hash, .. }) = volumes.get(&name) else {
        bail_into!("volume named '{}' not found", name);
    };

    debug!(id, name; "clear container info");

    let count = {
        let repos = state.repos.read().await;
        let Some(repo_info) = repos.get(hash) else {
            warn!(name; "repo info not found");
            trying_remove_path(&name, &path).await?;
            return Ok(Empty);
        };

        let mut repo_info = repo_info.write().await;
        repo_info.containers.remove(&id);
        repo_info.containers.len()
    };

    if count == 0 {
        debug!(name; "no containers");
        let mut repos = state.repos.write().await;
        repos.remove(hash);
        trying_remove_path(&name, &path).await?;
    }

    Ok(Empty)
}
