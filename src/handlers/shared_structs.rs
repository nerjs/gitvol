use std::path::PathBuf;

use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[cfg_attr(test, derive(Debug, PartialEq))]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<PathBuf>,
}

impl IntoResponse for MountPoint {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl Named {
        pub fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }
}
