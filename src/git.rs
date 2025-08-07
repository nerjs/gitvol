use std::{path::PathBuf, process::Output};

use anyhow::{Context, Result};
use log::{debug, kv::Value};
use serde::{Deserialize, Serialize};
use tokio::{fs, process::Command};

use crate::state::Repo;

pub async fn ensure_git_exists() -> Result<()> {
    let git_path = run_command("which", "git")
        .await
        .context("Failed to locate git executable")?;
    debug!(git_path;  "Located git executable");
    let git_version = run_command("git", "--version")
        .await
        .context("Failed to retrieve git version")?;
    debug!(version = format!("'{}'", git_version); "Verified git version");

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Info {
    repo: Repo,
    count: u32,
}

pub async fn clone(path: &PathBuf, repo: &Repo) -> Result<()> {
    debug!(url = repo.url; "trying clonning repository");
    anyhow::ensure!(!path.exists(), "path '{:?}' already exists", path);

    fs::create_dir_all(path)
        .await
        .context("failed creating repo directory")?;
    let info_path = path.join("info.json");
    let json = serde_json::to_string(&Info {
        repo: repo.clone(),
        count: 0,
    })
    .context("serialize info")?;
    fs::write(info_path, json).await.context("save info")?;

    Ok(())
}

pub async fn refetch(path: &PathBuf) -> Result<()> {
    debug!(path = Value::from_debug(path); "trying refetch repository");
    anyhow::ensure!(path.exists(), "path {:?} not exists", path);
    let info_path = path.join("info.json");
    anyhow::ensure!(info_path.exists(), "path {:?} not exists", path);

    let content = fs::read(&info_path).await.context("read info")?;
    let mut info: Info =
        serde_json::from_str(&String::from_utf8(content).context("normalize info string")?)
            .context("deserialize info")?;
    info.count = info.count + 1;
    let json = serde_json::to_string(&info).context("serialize info")?;
    fs::write(info_path, json).await.context("update info")?;

    Ok(())
}

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
