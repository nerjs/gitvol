use std::path::PathBuf;

use crate::{
    domains::{repo::RawRepo, volume::Volume},
    result::Result,
    services::volumes::Volumes,
};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

#[derive(Clone)]
pub struct GitvolState {
    pub path: PathBuf,
    pub volumes: Volumes,
}

impl GitvolState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,

            volumes: Volumes::new(),
        }
    }

    pub async fn create(&self, name: &str, raw: Option<RawRepo>) -> Result<()> {
        _ = self.volumes.create(name, raw).await?;
        Ok(())
    }

    pub async fn remove(&self, name: &str) -> Option<Volume> {
        self.volumes.remove(name).await
    }

    pub async fn read(&self, name: &str) -> Option<OwnedRwLockReadGuard<Volume>> {
        self.volumes.read(name).await
    }

    pub async fn write(&self, name: &str) -> Option<OwnedRwLockWriteGuard<Volume>> {
        self.volumes.write(name).await
    }

    pub async fn try_read(&self, name: &str) -> Result<OwnedRwLockReadGuard<Volume>> {
        Ok(self.volumes.try_read(name).await?)
    }

    pub async fn try_write(&self, name: &str) -> Result<OwnedRwLockWriteGuard<Volume>> {
        Ok(self.volumes.try_write(name).await?)
    }

    pub async fn read_all(&self) -> Vec<Volume> {
        self.volumes.read_all().await
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
}
