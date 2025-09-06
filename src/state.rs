use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use crate::{
    domains::repo::Repo,
    result::{Error, Result},
};
use serde::Serialize;
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

#[derive(Debug, Serialize, Clone, PartialEq)]
pub enum RepoStatus {
    Created,
    Clonned,
    Cleared,
}

#[cfg_attr(test, derive(Clone))]
pub struct Volume {
    pub name: String,
    pub path: Option<PathBuf>,
    pub repo: Repo,
    pub status: RepoStatus,
    pub containers: HashSet<String>,
}

impl Volume {
    pub fn build_path(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.repo.hash(&mut hasher);
        format!("{}_{}", self.name, hasher.finish())
    }
}

#[derive(Clone)]
pub struct GitvolState {
    pub path: PathBuf,
    pub volumes: Arc<RwLock<HashMap<String, Arc<RwLock<Volume>>>>>,
}

impl GitvolState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,

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

        if name.is_empty() {
            return Err(Error::EmptyVolumeName);
        }

        let mut volumes = self.write_map().await;

        if volumes.contains_key(name) {
            return Err(Error::VolumeAlreadyExists {
                name: name.to_string(),
            });
        }

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

    pub async fn read(&self, name: &str) -> Option<OwnedRwLockReadGuard<Volume>> {
        let volume = self.get(name).await?;
        let guard = volume.read_owned().await;
        Some(guard)
    }

    pub async fn write(&self, name: &str) -> Option<OwnedRwLockWriteGuard<Volume>> {
        let volume = self.get(name).await?;
        let guard = volume.write_owned().await;
        Some(guard)
    }

    pub async fn try_read(&self, name: &str) -> Result<OwnedRwLockReadGuard<Volume>> {
        let volume = self.read(name).await;
        volume.ok_or_else(|| Error::VolumeNonExists(name.to_string()))
    }

    pub async fn try_write(&self, name: &str) -> Result<OwnedRwLockWriteGuard<Volume>> {
        let volume = self.write(name).await;
        volume.ok_or_else(|| Error::VolumeNonExists(name.to_string()))
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    pub const VOLUME_NAME: &str = "test_volume";
    pub const REPO_URL: &str = "https://example.com/repo.git";

    impl Volume {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                containers: HashSet::new(),
                path: None,
                repo: Repo::stub(),
                status: RepoStatus::Cleared,
            }
        }
    }

    impl GitvolState {
        pub fn stub() -> Self {
            Self::new(std::env::temp_dir())
        }

        pub async fn stub_with_create() -> Self {
            let state = Self::stub();
            state.create(VOLUME_NAME, Repo::stub()).await.unwrap();

            state
        }
    }

    #[tokio::test]
    async fn create_new_state() {
        let path = std::env::current_dir().unwrap().join("test");
        let state = GitvolState::new(path.clone());

        assert_eq!(path, state.path);

        let volumes = state.read_map().await;
        assert_eq!(volumes.len(), 0);
    }

    #[tokio::test]
    async fn create_and_read_new_volume() {
        let state = GitvolState::stub();
        let repo = Repo::stub();
        state.create(VOLUME_NAME, repo.clone()).await.unwrap();

        let volume = state.read(VOLUME_NAME).await.unwrap();

        assert_eq!(volume.name, VOLUME_NAME);
        assert_eq!(volume.path, None);
        assert_eq!(volume.repo, repo);
    }

    #[tokio::test]
    async fn read_nonexistent_volume() {
        let state = GitvolState::stub();

        let volume = state.read(VOLUME_NAME).await;
        assert!(volume.is_none());
    }

    #[tokio::test]
    async fn create_volume_with_empty_name() {
        let state = GitvolState::stub();

        let result = state.create("", Repo::stub()).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn create_volume_with_whitespace_name() {
        let state = GitvolState::stub();

        // trimmed volume name
        let result = state.create("   ", Repo::stub()).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn create_duplicate_volume() {
        let state = GitvolState::stub();

        let result1 = state.create(VOLUME_NAME, Repo::stub()).await;
        assert!(result1.is_ok());

        let result2 = state.create(VOLUME_NAME, Repo::stub()).await;
        assert!(result2.is_err());
        let error = result2.unwrap_err();
        assert!(error.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn write_nonexistent_volume() {
        let state = GitvolState::stub();

        let volume = state.write(VOLUME_NAME).await;
        assert!(volume.is_none());
    }

    #[tokio::test]
    async fn write_existing_volume() {
        let state = GitvolState::stub_with_create().await;

        let mut volume = state.write(VOLUME_NAME).await.unwrap();
        assert_eq!(volume.status, RepoStatus::Created);

        volume.status = RepoStatus::Cleared;
        drop(volume);

        let volume = state.read(VOLUME_NAME).await.unwrap();
        assert_eq!(volume.status, RepoStatus::Cleared);
    }

    #[tokio::test]
    async fn read_map_empty() {
        let state = GitvolState::stub();

        let volumes = state.read_map().await;

        assert_eq!(volumes.len(), 0);
    }

    #[tokio::test]
    async fn read_map_with_volumes() {
        let state = GitvolState::stub_with_create().await;
        let volumes = state.read_map().await;

        assert_eq!(volumes.len(), 1);
        assert!(volumes.contains_key(VOLUME_NAME));
    }

    #[tokio::test]
    async fn write_map_add_volume() {
        let state = GitvolState::stub();

        let mut volumes = state.write_map().await;
        volumes.insert(
            VOLUME_NAME.to_string(),
            Arc::new(RwLock::new(Volume::new(VOLUME_NAME))),
        );
        drop(volumes);

        let volumes = state.read_map().await;

        assert_eq!(volumes.len(), 1);
        assert!(volumes.contains_key(VOLUME_NAME));
    }
}
