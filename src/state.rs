use anyhow::Result;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use log::{
    kv::{ToValue, Value},
    trace,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{join, sync::RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub url: String,
    pub branch: Option<String>,
    pub updatable: bool,
}

impl Repo {
    pub fn hash(&self) -> String {
        let hash = Sha256::digest(format!("{:?}", self));
        format!("{:x}", hash)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub name: String,
    pub path: PathBuf,
    pub hash: String,
    pub repo: Repo,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum RepoStatus {
    Created,
    Clonning,
    Ready,
}

impl ToValue for RepoStatus {
    fn to_value(&self) -> Value {
        Value::from_debug(self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RepoInfo {
    pub status: RepoStatus,
    pub containers: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct GitvolState {
    pub base_path: Arc<RwLock<PathBuf>>,
    pub volumes: Arc<RwLock<HashMap<String, Volume>>>,
    pub repos: Arc<RwLock<HashMap<String, Arc<RwLock<RepoInfo>>>>>,
}

#[derive(Serialize, Deserialize)]
struct FileState {
    volumes: HashMap<String, Volume>,
    repos: HashMap<String, RepoInfo>
}


impl GitvolState {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path: Arc::new(RwLock::new(base_path)),
            volumes: Arc::new(RwLock::new(HashMap::new())),
            repos: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn restore(&self) -> Result<()> {
        trace!("Trying to restore stgate");
        let (base_path, volumes, repos) = join!(self.base_path.read(), self.volumes.write(), self.repos.write());

        if !base_path.exists() {
            return Ok(());
        }

        let state_json_path = base_path.join("state.json");
        if !state_json_path.exists() {
            return Ok(())
        }

        Ok(())
    }

    pub async fn save(&self) -> Result<()> {
        let (base_path, volumes, repos) = join!(self.base_path.read(), self.volumes.read(), self.repos.read());
        trace!("Trying to save stgate");
        Ok(())
    }
}
