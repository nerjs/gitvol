use std::{path::PathBuf, str::FromStr};

use super::{
    result::PluginResult,
    shared_structs::{Empty, MountPoint, Named, NamedWID},
};
use crate::state::{GitvolState, Repo};
use anyhow::Result;
use axum::{Json, extract::State};
use serde::Deserialize;
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

pub struct CreateRequest {
    pub name: String,
    pub opts: Repo,
}

async fn prepare_opts(RawCreateRequest { name, opts }: RawCreateRequest) -> Result<CreateRequest> {
    let Some(RawRepo {
        url,
        branch,
        updatable,
    }) = opts
    else {
        anyhow::bail!("url option is required");
    };
    let Some(url) = url else {
        anyhow::bail!("url option is required");
    };

    let updatable = updatable.unwrap_or(false);

    Ok(CreateRequest {
        name,
        opts: Repo {
            url,
            branch,
            updatable,
        },
    })
}

pub async fn create_handler(
    State(_state): State<GitvolState>,
    Json(req): Json<RawCreateRequest>,
) -> PluginResult<Empty> {
    let CreateRequest { name, opts } = prepare_opts(req).await?;

    debug!("create volume: {name:?} -> {opts:?}");
    Ok(Empty)
}

pub async fn remove_handler(
    State(_state): State<GitvolState>,
    Json(req): Json<Named>,
) -> PluginResult<Empty> {
    debug!("remove volume: {req:?}");
    Ok(Empty)
}

pub async fn mount_handler(
    State(_state): State<GitvolState>,
    Json(req): Json<NamedWID>,
) -> PluginResult<MountPoint> {
    debug!("mount volume: {req:?}");
    Ok(PathBuf::from_str("/oo").unwrap().into())
}

pub async fn unmount_handler(
    State(_state): State<GitvolState>,
    Json(req): Json<NamedWID>,
) -> PluginResult<Empty> {
    debug!("unmount volume: {req:?}");
    Ok(Empty)
}
