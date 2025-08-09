use std::path::PathBuf;

use anyhow::Context;
use axum::{Json, body::Body, http::StatusCode, response::IntoResponse};
use log::{debug, kv};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::fs;

use crate::state::{Repo, RepoStatus};

// CORE

#[derive(Debug)]
pub(super) struct Error(anyhow::Error);
impl<E> From<E> for Error
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        Self(value.into())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response<Body> {
        log::error!("{:?}", self.0);
        (StatusCode::OK, Json(json!({"Err":format!("{}", self.0)}))).into_response()
    }
}

pub(super) type Result<T = Json<serde_json::Value>> = std::result::Result<T, Error>;

// INPUT

#[cfg_attr(test, derive(Clone))]
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Named {
    pub(super) name: String,
}

#[cfg_attr(test, derive(Default, Clone))]
#[derive(Deserialize)]
pub(super) struct RawRepo {
    pub(super) url: Option<String>,
    pub(super) branch: Option<String>,
    pub(super) tag: Option<String>,
    pub(super) refetch: Option<bool>,
}

impl TryInto<Repo> for Option<RawRepo> {
    type Error = anyhow::Error;

    fn try_into(self) -> anyhow::Result<Repo> {
        let Some(RawRepo {
            url,
            branch,
            tag,
            refetch,
        }) = self
        else {
            anyhow::bail!(
                "Invalid repository configuration: no options provided, git URL is required"
            );
        };

        let Some(url) = url else {
            anyhow::bail!("Invalid repository configuration: git URL is missing");
        };

        if branch.is_some() && tag.is_some() {
            anyhow::bail!("Only one of branch, tag, or ref parameters is allowed");
        }

        let branch = branch.or(tag);
        let refetch = refetch.unwrap_or(false);

        debug!(url, branch, refetch; "Parsed repository options");

        Ok(Repo {
            url,
            branch,
            refetch,
        })
    }
}

#[cfg_attr(test, derive(Default, Clone))]
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct RawCreateRequest {
    pub(super) name: String,
    pub(super) opts: Option<RawRepo>,
}

#[cfg_attr(test, derive(Clone, Debug))]
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct NamedWID {
    pub(super) name: String,
    #[serde(rename = "ID")]
    pub(super) id: String,
}

// OUTPUT

#[cfg_attr(test, derive(Debug))]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Mp {
    pub(super) mountpoint: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct OptionalMp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) mountpoint: Option<PathBuf>,
}

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct GetMp {
    pub(super) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) mountpoint: Option<PathBuf>,
    pub(super) status: RepoStatus,
}

#[cfg_attr(test, derive(Debug))]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct GetResponse {
    pub(super) volume: GetMp,
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct ListMp {
    pub(super) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) mountpoint: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct ListResponse {
    pub(super) volumes: Vec<ListMp>,
}

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize)]
pub(super) struct Empty;

macro_rules! into_response {
    ($($name:ident),*) => {
        $(

            impl axum::response::IntoResponse for $name {
                fn into_response(self) -> axum::response::Response {
                    Json(self).into_response()
                }
            }
        )*
    };
}

into_response!(Mp, OptionalMp, GetResponse, ListResponse, Empty);

// HELPERS

pub(super) async fn remove_dir_if_exists(path: Option<PathBuf>) -> anyhow::Result<()> {
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
