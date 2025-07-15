use super::{
    result::PluginResult,
    shared_structs::{Empty, MountPoint, Named, NamedWID},
};
use crate::{
    bail_cond, ensure_cond,
    state::{GitvolState, Repo, Volume},
};
use anyhow::{Context, Result};
use axum::{Json, extract::State};
use serde::Deserialize;
use tokio::fs;
use tracing::debug;

#[derive(Debug, Clone, Deserialize)]
pub struct RawRepo {
    pub url: Option<String>,
    pub branch: Option<String>,
    pub updatable: Option<bool>,
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
        updatable,
    }) = opts
    else {
        bail_cond!("url option is required");
    };

    let Some(url) = url else {
        bail_cond!("url option is required");
    };

    let updatable = updatable.unwrap_or(false);

    Ok(Repo {
        url,
        branch,
        updatable,
    })
}

pub async fn create_handler(
    State(state): State<GitvolState>,
    Json(RawCreateRequest { name, opts }): Json<RawCreateRequest>,
) -> PluginResult<Empty> {
    debug!("attempt to create volume named {}", name);
    let mut volumes = state.volumes.write().await;
    let repo = prepare_opts(opts).await?;
    let hash = repo.hash();

    match volumes.get(&name) {
        Some(volume) => {
            ensure_cond!(
                volume.hash == hash,
                "The repository settings are not the same as previously set"
            );
            debug!("volume {} was created earlier", name);
        }
        None => {
            let path = state.path.join(&hash);
            let volume = Volume {
                hash,
                name: name.clone(),
                path,
                repo,
            };
            volumes.insert(name.clone(), volume);
            debug!("volume '{}' was created", name);
        }
    }

    Ok(Empty)
}

async fn clear_volume(name: &str, state: &GitvolState) -> Result<()> {
    debug!("Deleting all data volume '{}'", name);
    let mut volumes = state.volumes.write().await;

    let Some(Volume { path, hash, .. }) = volumes.get(name) else {
        debug!("Nothing to delete. volume '{}' does not exist", name);
        return Ok(());
    };
    let mut repos = state.repos.write().await;
    repos.remove(hash);

    if path.exists() {
        debug!("Deleting the ‘{:?}’ directory for volume '{}'", path, name);
        fs::remove_dir_all(path)
            .await
            .with_context(|| format!("remove volume '{}' dir", &name))?;
    }

    volumes.remove(name);
    debug!("All data for volume ‘{}’ has been deleted", name);

    Ok(())
}

pub async fn remove_handler(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> PluginResult<Empty> {
    debug!("attempt to remove volume named {}", name);
    clear_volume(&name, &state)
        .await
        .context("clear all data")?;

    Ok(Empty)
}

pub async fn mount_handler(
    State(_state): State<GitvolState>,
    Json(req): Json<NamedWID>,
) -> PluginResult<MountPoint> {
    debug!("mount volume: {req:?}");

    if true {
        bail_cond!("sss");
    }

    ensure_cond!(1 == 1 && 2 == 2 || 0 != 2, "req {}", 1);

    Ok(MountPoint {
        mountpoint: Some("/ll".into()),
    })
}

pub async fn unmount_handler(
    State(_state): State<GitvolState>,
    Json(req): Json<NamedWID>,
) -> PluginResult<Empty> {
    debug!("unmount volume: {req:?}");
    Ok(Empty)
}
