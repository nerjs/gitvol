use axum::{
    Json,
    body::Body,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use serde_json::json;
use tracing::error;

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

#[macro_export]
macro_rules! bail_cond {
    ($msg:literal $(,)?) => {
        return Err(anyhow::anyhow!($msg).into())
    };
    ($err:expr $(,)?) => {
        return Err(anyhow::anyhow!($err).into())
    };
    ($fmt:expr, $($arg:tt)*) => {
        return Err(anyhow::anyhow!($fmt, $($arg)*).into())
    };
}
#[macro_export]
macro_rules! ensure_cond {
        ($cond:expr $(,)?) => {
            if !$cond {
                // e -1
                let msg = anyhow::__private::concat!("Condition failed: `", anyhow::__private::stringify!($cond), "`");
                $crate::bail_cond!(msg)
            }
        };
        ($cond:expr, $msg:literal $(,)?) => {
            if !$cond {
                // e 1
                bail_cond!($msg)
            }
        };
        ($cond:expr, $err:expr $(,)?) => {
            if !$cond {
                // e 2
                bail_cond!($err)
            }
        };
        ($cond:expr, $fmt:expr, $($arg:tt)*) => {
            if !$cond {
                // e 3
                bail_cond!($fmt, $($arg)*)
            }
        };
    }
