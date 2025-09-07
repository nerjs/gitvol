use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::{
    domains::{repo::RawRepo, volume::Volume},
    result::{Error, Result},
};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

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

    pub async fn create(&self, name: &str, raw: Option<RawRepo>) -> Result<()> {
        let mut volumes = self.write_map().await;

        let volume = Volume::try_from((name, raw))?;

        if volumes.contains_key(&volume.name) {
            return Err(Error::VolumeAlreadyExists {
                name: name.to_string(),
            });
        }

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
    use crate::domains::volume::{Status, test::VOLUME_NAME};

    impl GitvolState {
        pub fn stub() -> Self {
            Self::new(std::env::temp_dir())
        }

        pub async fn stub_with_create() -> Self {
            let state = Self::stub();
            state
                .create(VOLUME_NAME, Some(RawRepo::stub()))
                .await
                .unwrap();

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
        let opts = RawRepo::stub();
        state.create(VOLUME_NAME, Some(opts.clone())).await.unwrap();

        let volume = state.read(VOLUME_NAME).await.unwrap();

        assert_eq!(volume.name, VOLUME_NAME);
        assert_eq!(volume.path, None);
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

        let result = state.create("", Some(RawRepo::stub())).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn create_volume_with_whitespace_name() {
        let state = GitvolState::stub();

        // trimmed volume name
        let result = state.create("   ", Some(RawRepo::stub())).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn create_duplicate_volume() {
        let state = GitvolState::stub();

        let result1 = state.create(VOLUME_NAME, Some(RawRepo::stub())).await;
        assert!(result1.is_ok());

        let result2 = state.create(VOLUME_NAME, Some(RawRepo::stub())).await;
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
        assert_eq!(volume.status, Status::Created);

        volume.status = Status::Cleared;
        drop(volume);

        let volume = state.read(VOLUME_NAME).await.unwrap();
        assert_eq!(volume.status, Status::Cleared);
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
            Arc::new(RwLock::new(Volume::stub())),
        );
        drop(volumes);

        let volumes = state.read_map().await;

        assert_eq!(volumes.len(), 1);
        assert!(volumes.contains_key(VOLUME_NAME));
    }
}
