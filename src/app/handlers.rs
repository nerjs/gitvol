use axum::{Json, extract::State};
use serde_json::json;
use tracing::debug;

use crate::{driver::Driver, plugin::Plugin, result::Error};

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
    State(plugin): State<Plugin>,
    Json(Named { name }): Json<Named>,
) -> Result<OptionalMp> {
    let mountpoint = plugin
        .path(&name)
        .await
        .map_err(|e| Error::String(e.to_string()))?;
    Ok(OptionalMp { mountpoint })
}

pub(super) async fn get_volume(
    State(plugin): State<Plugin>,
    Json(Named { name }): Json<Named>,
) -> Result<GetResponse> {
    let info = plugin
        .get(&name)
        .await
        .map_err(|e| Error::String(e.to_string()))?;
    Ok(GetResponse {
        volume: GetMp {
            name,
            mountpoint: info.mountpoint,
            status: info.status,
        },
    })
}

pub(super) async fn list_volumes(State(plugin): State<Plugin>) -> Result<ListResponse> {
    let list = plugin
        .list()
        .await
        .map_err(|e| Error::String(e.to_string()))?;

    Ok(ListResponse {
        volumes: list
            .into_iter()
            .map(|item| ListMp {
                name: item.name,
                mountpoint: item.mountpoint,
            })
            .collect(),
    })
}

pub(super) async fn create_volume(
    State(plugin): State<Plugin>,
    Json(RawCreateRequest { name, opts }): Json<RawCreateRequest>,
) -> Result<Empty> {
    plugin
        .create(&name, opts)
        .await
        .map_err(|e| Error::String(e.to_string()))?;
    Ok(Empty {})
}

pub(super) async fn remove_volume(
    State(plugin): State<Plugin>,
    Json(Named { name }): Json<Named>,
) -> Result<Empty> {
    plugin
        .remove(&name)
        .await
        .map_err(|e| Error::String(e.to_string()))?;
    Ok(Empty {})
}

pub(super) async fn mount_volume_to_container(
    State(plugin): State<Plugin>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> Result<Mp> {
    let mountpoint = plugin
        .mount(&name, &id)
        .await
        .map_err(|e| Error::String(e.to_string()))?;
    Ok(Mp { mountpoint })
}

pub(super) async fn unmount_volume_by_container(
    State(plugin): State<Plugin>,
    Json(NamedWID { name, id }): Json<NamedWID>,
) -> Result<Empty> {
    plugin
        .unmount(&name, &id)
        .await
        .map_err(|e| Error::String(e.to_string()))?;
    Ok(Empty {})
}
