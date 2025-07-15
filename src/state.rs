use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

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

#[derive(Debug, Clone)]
pub struct Volume {
    pub name: String,
    pub path: PathBuf,
    pub hash: String,
    pub repo: Repo,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub enum RepoStatus {
    Created,
    Clonning,
    Ready,
}

#[derive(Debug)]
pub struct RepoInfo {
    pub status: RepoStatus,
    pub containers: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct GitvolState {
    pub path: PathBuf,
    pub volumes: Arc<RwLock<HashMap<String, Volume>>>,
    pub repos: Arc<RwLock<HashMap<String, Arc<RwLock<RepoInfo>>>>>,
}

impl GitvolState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            volumes: Arc::new(RwLock::new(HashMap::new())),
            repos: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
