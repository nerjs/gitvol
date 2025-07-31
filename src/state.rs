use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use anyhow::Result;
use log::kv::{ToValue, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock, RwLockReadGuard};

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

#[derive(Debug, Clone)]
pub struct Volume {
    pub name: String,
    pub path: Option<PathBuf>,
    pub repo: Repo,
    pub status: RepoStatus,
    pub containers: HashSet<String>,
}

impl ToValue for RepoStatus {
    fn to_value(&self) -> Value {
        Value::from_debug(self)
    }
}

pub struct VolumeReadGuard<'a> {
    volume: RwLock<Volume>,
    guard: RwLockReadGuard<'a, Volume>,
}

#[derive(Debug)]
pub struct RepoInfo {
    pub status: RepoStatus,
    pub containers: HashSet<String>,
}

type LockVolume = Arc<RwLock<Volume>>;
type LockMap = Arc<RwLock<HashMap<String, LockVolume>>>;

#[derive(Debug, Clone)]
pub struct GitvolState {
    pub path: PathBuf,
    pub volumes2: Arc<RwLock<HashMap<String, Volume2>>>,
    pub repos: Arc<RwLock<HashMap<String, Arc<RwLock<RepoInfo>>>>>,

    pub volumes: Arc<RwLock<HashMap<String, Arc<RwLock<Volume>>>>>,
}

impl GitvolState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            volumes2: Arc::new(RwLock::new(HashMap::new())),
            repos: Arc::new(RwLock::new(HashMap::new())),

            volumes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn read_map(&self) -> OwnedRwLockReadGuard<HashMap<String, Arc<RwLock<Volume>>>> {
        self.volumes.clone().read_owned().await
    }

    pub async fn write_map(&self) -> OwnedRwLockWriteGuard<HashMap<String, Arc<RwLock<Volume>>>> {
        self.volumes.clone().write_owned().await
    }

    async fn get(&self, name: &str) -> Option<Arc<RwLock<Volume>>> {
        let volumes = self.read_map().await;
        let volume = volumes.get(name);
        volume.cloned()
    }

    pub async fn create(&self, name: &str, repo: Repo) -> Result<()> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "An empty volume name is not allowed");

        let mut volumes = self.write_map().await;
        anyhow::ensure!(
            !volumes.contains_key(name),
            "Volume named '{name}' already exists"
        );

        let volume = Volume {
            name: name.to_string(),
            path: None,
            repo,
            status: RepoStatus::Created,
            containers: HashSet::new(),
        };

        let volume = Arc::new(RwLock::new(volume));
        volumes.insert(name.to_string(), volume.clone());

        Ok(())
    }

    async fn has(&self, name: &str) -> bool {
        let volumes = self.read_map().await;
        volumes.contains_key(name)
    }

    async fn get_or_create(&self, name: &str, repo: Repo) -> Arc<RwLock<Volume>> {
        let mut volumes = self.write_map().await;
        let volume = volumes.get(name);

        match volume {
            Some(volume) => volume.clone(),
            None => {
                let volume = Volume {
                    name: name.to_string(),
                    path: None,
                    repo,
                    status: RepoStatus::Created,
                    containers: HashSet::new(),
                };

                let volume = Arc::new(RwLock::new(volume));
                volumes.insert(name.to_string(), volume.clone());
                volume
            }
        }
    }

    pub async fn remove(&self, name: &str) {
        let mut volumes = self.write_map().await;
        volumes.remove(name);
    }

    pub async fn read(&self, name: &str) -> Option<OwnedRwLockReadGuard<Volume>> {
        let volume = self.get(name).await?;
        let guard = volume.read_owned().await;
        Some(guard)
    }

    pub async fn read_or_create(&self, name: &str, repo: Repo) -> OwnedRwLockReadGuard<Volume> {
        let volume = self.get_or_create(name, repo).await;
        let guard = volume.read_owned().await;
        guard
    }

    pub async fn write(&self, name: &str) -> Option<OwnedRwLockWriteGuard<Volume>> {
        let volume = self.get(name).await?;
        let guard = volume.write_owned().await;
        Some(guard)
    }

    pub async fn write_or_create(&self, name: &str, repo: Repo) -> OwnedRwLockWriteGuard<Volume> {
        let volume = self.get_or_create(name, repo).await;
        let guard = volume.write_owned().await;
        guard
    }
}

#[cfg(test)]
mod test {
    use anyhow::{Context, Result};

    use super::*;

    impl Default for Repo {
        fn default() -> Self {
            Self {
                url: Default::default(),
                branch: Default::default(),
                refetch: Default::default(),
            }
        }
    }

    #[allow(dead_code)]
    impl Repo {
        pub fn new(url: &str) -> Self {
            Self {
                url: url.to_string(),
                ..Default::default()
            }
        }

        pub fn with_url(mut self, url: &str) -> Self {
            self.url = url.to_string();
            self
        }

        pub fn with_branch(mut self, branch: &str) -> Self {
            self.branch = Some(branch.into());
            self
        }

        pub fn with_refetch(mut self, refetch: bool) -> Self {
            self.refetch = refetch;
            self
        }
    }

    impl GitvolState {
        pub async fn set_path(&self, name: &str, path: impl Into<PathBuf>) -> Result<()> {
            let mut volume = self.write(name).await.context("missing volume")?;
            volume.path = Some(path.into());
            Ok(())
        }
    }
}
