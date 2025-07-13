use std::path::PathBuf;

use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone)]
pub struct Empty;

impl IntoResponse for Empty {
    fn into_response(self) -> Response {
        Json(json!({})).into_response()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Named {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NamedWID {
    pub name: String,
    #[serde(rename = "ID")]
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MountPoint {
    pub mountpoint: PathBuf,
}

impl IntoResponse for MountPoint {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

impl From<PathBuf> for MountPoint {
    fn from(value: PathBuf) -> Self {
        Self { mountpoint: value }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NamedMountPoint {
    pub name: String,
    pub mountpoint: Option<String>,
}
