use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use log::kv::{ToValue, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub url: String,
    pub branch: Option<String>,
    pub refetch: bool,
}

impl Repo {
    pub fn hash(&self) -> String {
        let hash = Sha256::digest(format!("{:?}", self));
        format!("{:x}", hash)
    }
}

#[derive(Debug, Clone)]
pub struct Volume2 {
    pub name: String,
    pub path: PathBuf,
    pub hash: String,
    pub repo: Repo,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub enum RepoStatus {
    Created,
    Clonned,
}

impl ToValue for RepoStatus {
    fn to_value(&self) -> Value {
        Value::from_debug(self)
    }
}

#[derive(Debug)]
pub struct RepoInfo {
    pub status: RepoStatus,
    pub containers: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct GitvolState {
    pub path: PathBuf,
    pub volumes2: Arc<RwLock<HashMap<String, Volume2>>>,
    pub repos: Arc<RwLock<HashMap<String, Arc<RwLock<RepoInfo>>>>>,
}

impl GitvolState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            volumes2: Arc::new(RwLock::new(HashMap::new())),
            repos: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
