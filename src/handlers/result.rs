use axum::{
    Json,
    body::Body,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use log::error;
use serde_json::json;

#[derive(Debug)]
pub struct PluginError(pub anyhow::Error);

impl<E> From<E> for PluginError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl IntoResponse for PluginError {
    fn into_response(self) -> Response<Body> {
        error!("{:?}", self.0);
        (StatusCode::OK, Json(json!({"Err":format!("{}", self.0)}))).into_response()
    }
}

pub type PluginResult<T> = anyhow::Result<T, PluginError>;
