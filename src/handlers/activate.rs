use super::result::PluginResult;
use crate::state::GitvolState;
use anyhow::{Context, Result};
use axum::{Json, extract::State, response::IntoResponse};
use log::debug;
use serde_json::json;
use std::process::Output;
use tokio::{fs, process::Command};

async fn run_command(cmd: &str, arg: &str) -> Result<String> {
    let Output {
        status,
        stderr,
        stdout,
    } = Command::new(cmd)
        .arg(arg)
        .output()
        .await
        .with_context(|| format!("Failed to execute command '{} {}'", cmd, arg))?;

    let stderr = String::from_utf8(stderr)
        .context("Failed to parse stderr as UTF-8")?
        .trim()
        .to_string();
    let stdout = String::from_utf8(stdout)
        .context("Failed to parse stdout as UTF-8")?
        .trim()
        .to_string();

    if !status.success() {
        if stderr.is_empty() {
            anyhow::bail!(
                "Command '{} {}' exited with non-zero status: {}",
                cmd,
                arg,
                status
            )
        } else {
            anyhow::bail!("Command '{} {}' failed: {}", cmd, arg, stderr)
        }
    }

    Ok(stdout)
}

pub async fn activate_plugin(State(state): State<GitvolState>) -> PluginResult<impl IntoResponse> {
    debug!("Initiating plugin activation");

    let git_path = run_command("which", "git")
        .await
        .context("Failed to locate git executable")?;
    debug!(git_path;  "Located git executable");
    let git_version = run_command("git", "--version")
        .await
        .context("Failed to retrieve git version")?;
    debug!(version = format!("'{}'", git_version); "Verified git version");

    if !state.path.exists() {
        fs::create_dir_all(&state.path)
            .await
            .with_context(|| format!("Failed to create volumes directory at '{:?}'", state.path))?;
        debug!(path = state.path.to_str(); "Created volumes directory");
    }
    Ok(Json(json!({ "Implements": ["VolumeDriver"] })))
}
