use std::path::PathBuf;

use anyhow::{Context, Result};
use log::{debug, kv::Value};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::state::Repo;

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
