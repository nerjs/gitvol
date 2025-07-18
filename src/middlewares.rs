use anyhow::Context;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use log::error;

use crate::state::GitvolState;

pub async fn save_middleware(
    State(state): State<GitvolState>,
    request: Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;

    if let Err(error) = state.save().await.context("Failed save state") {
        error!("{:?}", error);
    }

    response
}
