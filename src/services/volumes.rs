use crate::domains::{repo::RawRepo, volume::Volume};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Volume named {0} already exists")]
    AlreadyExists(String),

    #[error("Non existen volume named {0}")]
    NonExists(String),

    #[error(transparent)]
    Volume(#[from] crate::domains::volume::Error),
}

type Vol = Arc<RwLock<Volume>>;
type VolMap = HashMap<String, Vol>;

#[derive(Clone)]
pub struct Volumes {
    inner: Arc<RwLock<VolMap>>,
}

impl Volumes {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn read_map(&self) -> OwnedRwLockReadGuard<VolMap> {
        self.inner.clone().read_owned().await
    }

    async fn write_map(&self) -> OwnedRwLockWriteGuard<VolMap> {
        self.inner.clone().write_owned().await
    }

    async fn get(&self, name: &str) -> Option<Vol> {
        let volumes = self.read_map().await;
        let volume = volumes.get(name);
        volume.cloned()
    }

    pub async fn create(
        &self,
        name: &str,
        raw: Option<RawRepo>,
    ) -> Result<OwnedRwLockWriteGuard<Volume>, Error> {
        let mut volumes = self.write_map().await;

        let volume = Volume::try_from((name, raw))?;

        if volumes.contains_key(&volume.name) {
            return Err(Error::AlreadyExists(name.to_string()));
        }

        let volume = Arc::new(RwLock::new(volume));
        volumes.insert(name.to_string(), volume.clone());

        Ok(volume.write_owned().await)
    }

    pub async fn remove(&self, name: &str) -> Option<Volume> {
        let mut list = self.write_map().await;

        let locked_volume = list.get(name)?;
        let volume_guard = locked_volume.read().await;

        let cloned_volume = volume_guard.clone();
        drop(volume_guard);
        list.remove(name);

        Some(cloned_volume)
    }

    pub async fn read(&self, name: &str) -> Option<OwnedRwLockReadGuard<Volume>> {
        let volume = self.get(name).await?;
        let guard = volume.read_owned().await;
        Some(guard)
    }

    pub async fn read_all(&self) -> Vec<Volume> {
        let map = self.read_map().await;
        let mut list: Vec<Volume> = Vec::with_capacity(map.len());

        for volume in map.values() {
            let volume = volume.read().await;
            list.push(volume.clone());
        }

        list
    }

    pub async fn write(&self, name: &str) -> Option<OwnedRwLockWriteGuard<Volume>> {
        let volume = self.get(name).await?;
        let guard = volume.write_owned().await;
        Some(guard)
    }

    pub async fn try_read(&self, name: &str) -> Result<OwnedRwLockReadGuard<Volume>, Error> {
        let volume = self.read(name).await;
        volume.ok_or_else(|| Error::NonExists(name.to_string()))
    }

    pub async fn try_write(&self, name: &str) -> Result<OwnedRwLockWriteGuard<Volume>, Error> {
        let volume = self.write(name).await;
        volume.ok_or_else(|| Error::NonExists(name.to_string()))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::domains::{
        repo::test::REPO_URL,
        volume::{Status, test::VOLUME_NAME},
    };
    use rstest::rstest;

    #[tokio::test]
    async fn create_with_empty_list() {
        let volumes = Volumes::new();

        let list = volumes.read_all().await;
        let map = volumes.read_map().await;

        assert_eq!(list.len(), 0);
        assert_eq!(map.len(), 0);
    }

    #[rstest]
    #[case(" ", Some(RawRepo::stub()))]
    #[case(VOLUME_NAME, None)]
    #[case(VOLUME_NAME, Some(RawRepo::from_url(" ")))]
    #[case(
        VOLUME_NAME,
        Some(RawRepo::from_url("ssh://host:123/~user/path-to-git-repo"))
    )]
    #[case(
        VOLUME_NAME,
        Some(RawRepo::from_url("ssh://user@host:123/~user/path-to-git-repo"))
    )]
    #[case(VOLUME_NAME, Some(RawRepo::from_url("ftp://host/path-to-git-repo")))]
    #[case(VOLUME_NAME, Some(RawRepo::from_url("ftps://host/path-to-git-repo")))]
    #[case(VOLUME_NAME, Some(RawRepo { branch: Some("branch".into()),  tag: Some("tag".into()), ..RawRepo::stub() }))]
    #[tokio::test]
    async fn failed_creating_params(#[case] volume_name: &str, #[case] raw_repo: Option<RawRepo>) {
        let volumes = Volumes::new();

        let result = volumes.create(volume_name, raw_repo.clone()).await;
        assert!(
            result.is_err(),
            "volume_name={}; raw_repo={:?}",
            volume_name,
            raw_repo
        );

        let error = result.unwrap_err();
        assert!(
            matches!(error, Error::Volume(_)),
            "volume_name={}; raw_repo={:?}",
            volume_name,
            raw_repo
        )
    }

    #[tokio::test]
    async fn create_first_volume() {
        let volumes = Volumes::new();
        let volume = volumes
            .create(VOLUME_NAME, Some(RawRepo::stub()))
            .await
            .unwrap();

        assert_eq!(volume.name, VOLUME_NAME);
        assert_eq!(volume.repo.url.to_string(), REPO_URL);
        assert_eq!(volume.path, None);
    }

    #[tokio::test]
    async fn create_and_read_volume() {
        let volumes = Volumes::new();

        _ = volumes
            .create(VOLUME_NAME, Some(RawRepo::stub()))
            .await
            .unwrap();

        let volume = volumes.try_read(VOLUME_NAME).await.unwrap();

        assert_eq!(volume.name, VOLUME_NAME);
        assert_eq!(volume.repo.url.to_string(), REPO_URL);
        assert_eq!(volume.path, None);
    }

    #[tokio::test]
    async fn list_volumes() {
        let volumes = Volumes::new();
        let created_volume = volumes
            .create(VOLUME_NAME, Some(RawRepo::stub()))
            .await
            .unwrap();
        let first = created_volume.clone();
        drop(created_volume);

        let list = volumes.read_all().await;
        assert_eq!(list.len(), 1);
        assert!(list.contains(&first));

        let second_volume = volumes
            .create("second_name", Some(RawRepo::stub()))
            .await
            .unwrap();
        let second = second_volume.clone();
        drop(second_volume);

        let list = volumes.read_all().await;
        assert_eq!(list.len(), 2);
        assert!(list.contains(&first));
        assert!(list.contains(&second));
    }

    #[tokio::test]
    async fn remove_missing_volume() {
        let volumes = Volumes::new();
        let volume = volumes.remove(VOLUME_NAME).await;

        assert_eq!(volume, None)
    }

    #[tokio::test]
    async fn remove_volume() {
        let volumes = Volumes::new();
        _ = volumes
            .create(VOLUME_NAME, Some(RawRepo::stub()))
            .await
            .unwrap();

        let list = volumes.read_all().await;
        assert_eq!(list.len(), 1);

        let removed = volumes.remove(VOLUME_NAME).await;
        assert!(removed.is_some());
        let removed = removed.unwrap();
        assert_eq!(removed.name, VOLUME_NAME);

        let list = volumes.read_all().await;
        assert_eq!(list.len(), 0);
    }

    #[tokio::test]
    async fn read_nonexistent_volume() {
        let volumes = Volumes::new();

        let volume = volumes.read(VOLUME_NAME).await;
        assert!(volume.is_none());
    }

    #[tokio::test]
    async fn create_duplicate_volume() {
        let volumes = Volumes::new();

        let result1 = volumes.create(VOLUME_NAME, Some(RawRepo::stub())).await;
        assert!(result1.is_ok());

        let result2 = volumes.create(VOLUME_NAME, Some(RawRepo::stub())).await;
        assert!(result2.is_err());
        let error = result2.unwrap_err();
        assert!(error.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn try_read_nonexistent_volume() {
        let state = Volumes::new();

        let error = state.try_read(VOLUME_NAME).await.unwrap_err();
        assert!(matches!(error, Error::NonExists(_)));
    }

    #[tokio::test]
    async fn try_write_nonexistent_volume() {
        let state = Volumes::new();

        let error = state.try_write(VOLUME_NAME).await.unwrap_err();
        assert!(matches!(error, Error::NonExists(_)));
    }

    #[tokio::test]
    async fn write_existing_volume() {
        let volumes = Volumes::new();

        _ = volumes
            .create(VOLUME_NAME, Some(RawRepo::stub()))
            .await
            .unwrap();

        let mut volume = volumes.try_write(VOLUME_NAME).await.unwrap();
        assert_eq!(volume.status, Status::Created);

        volume.status = Status::Cleared;
        drop(volume);

        let volume = volumes.read(VOLUME_NAME).await.unwrap();
        assert_eq!(volume.status, Status::Cleared);
    }
}
