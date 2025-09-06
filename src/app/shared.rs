use std::{path::PathBuf, str::FromStr};

use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::fs;
use tracing::{debug, error, field};

use crate::{
    domains::url::Url,
    result::{Error, ErrorIoExt},
    state::{Repo, RepoStatus},
};

// CORE

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        error!("Response error: {:?}", self);
        let error = format!("{}", self);

        (StatusCode::OK, Json(json!({"Err":error}))).into_response()
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
    pub(super) refetch: Option<String>,
}

impl TryInto<Repo> for Option<RawRepo> {
    type Error = Error;

    fn try_into(self) -> crate::result::Result<Repo> {
        let Some(RawRepo {
            url,
            branch,
            tag,
            refetch,
        }) = self
        else {
            return Err(Error::ParamsNoOptions);
        };

        let Some(url) = url else {
            return Err(Error::ParamsRequiredUrl);
        };

        if branch.is_some() && tag.is_some() {
            return Err(Error::ParamsSingleBranch);
        }

        let branch = branch.or(tag);
        let refetch = refetch.unwrap_or("false".to_string()) == "true";
        let url = Url::from_str(&url)?.to_string();

        debug!(url, branch, refetch, "Parsed repository options");

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
pub(super) struct MpStatus {
    pub(super) status: RepoStatus,
}

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct GetMp {
    pub(super) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) mountpoint: Option<PathBuf>,
    // TODO: format must be an object/ example: {"CreatedAt": "2025-08-24T19:44:31", "Size": "10GB", "Available": "5GB"}
    pub(super) status: MpStatus,
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
pub(super) struct Empty {}

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

pub(super) async fn remove_dir_if_exists(path: Option<PathBuf>) -> crate::result::Result<()> {
    if let Some(path) = path
        && path.exists()
    {
        debug!(path = field::debug(&path), "Attempting to remove directory");
        fs::remove_dir_all(&path).await.map_io_error(&path)?;
    }

    Ok(())
}
