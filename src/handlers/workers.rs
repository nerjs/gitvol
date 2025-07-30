use std::{collections::HashSet, path::PathBuf, sync::Arc};

use super::{
    result::PluginResult,
    shared_structs::{Empty, MountPoint, Named, NamedWID},
};
use crate::{
    bail_into, ensure_into, git,
    state::{GitvolState, Repo, RepoInfo, RepoStatus, Volume2},
};
use anyhow::{Context, Result};
use axum::{Json, extract::State};
use log::{
    debug, info,
    kv::{self, Value},
    warn,
};
use serde::Deserialize;
use tokio::{fs, sync::RwLock};

#[derive(Debug, Clone, Deserialize)]
pub struct RawRepo {
    pub url: Option<String>,
    pub branch: Option<String>,
    pub refetch: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawCreateRequest {
    pub name: String,
    pub opts: Option<RawRepo>,
}

async fn prepare_opts(opts: Option<RawRepo>) -> Result<Repo> {
    let Some(RawRepo {
        url,
        branch,
        refetch,
    }) = opts
    else {
        bail_into!("url option is required");
    };

    let Some(url) = url else {
        bail_into!("url option is required");
    };

    Ok(Repo {
        url,
        branch,
        refetch: refetch.unwrap_or(false),
    })
}

pub async fn create_handler(
    State(state): State<GitvolState>,
    Json(RawCreateRequest { name, opts }): Json<RawCreateRequest>,
) -> PluginResult<Empty> {
    debug!("attempt to create volume named {}", name);
    let mut volumes = state.volumes2.write().await;
    let repo = prepare_opts(opts).await?;
    let hash = repo.hash();

    debug!(name, hash, url = repo.url, branch = repo.branch; "create volume draft");

    match volumes.get(&name) {
        Some(volume) => {
            ensure_into!(
                volume.hash == hash,
                "The repository settings are not the same as previously set"
            );
            debug!(name; "volume was created earlier");
        }
        None => {
            let path = state.path.join(&hash);
            let volume = Volume2 {
                hash,
                name: name.clone(),
                path,
                repo,
            };
            volumes.insert(name.clone(), volume);
            debug!(name; "volume was created");
        }
    }

    Ok(Empty)
}

async fn clear_volume(name: &str, state: &GitvolState) -> Result<()> {
    debug!(name; "Deleting all data volume");
    let mut volumes = state.volumes2.write().await;

    let Some(Volume2 { path, hash, .. }) = volumes.get(name) else {
        debug!(name; "Nothing to delete. volume does not exist");
        return Ok(());
    };
    let mut repos = state.repos.write().await;
    repos.remove(hash);

    if path.exists() {
        debug!(name, path = kv::Value::from_debug(path); "Deleting the ‘{:?}’ directory for volume '{}'", path, name);
        fs::remove_dir_all(path)
            .await
            .with_context(|| format!("remove volume '{}' dir", &name))?;
    }

    volumes.remove(name);
    debug!(name; "All data for volume has been deleted");

    Ok(())
}

pub async fn remove_handler(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> PluginResult<Empty> {
    debug!(name; "attempt to remove volume");
    clear_volume(&name, &state)
        .await
        .context("clear all data")?;

    debug!(name; "volume was removed");

    Ok(Empty)
}

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
