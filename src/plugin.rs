use crate::{git, result::ErrorIoExt};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::{
    domains::{repo::RawRepo, volume::Status as VolumeStatus},
    driver::{Driver, ItemVolume, Scope, VolumeInfo},
    services::volumes::Volumes,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Volumes(#[from] crate::services::volumes::Error),

    #[error(transparent)]
    App(#[from] crate::result::Error),
}

#[cfg_attr(test, derive(Debug, PartialEq, Clone))]
#[derive(Serialize)]
pub struct Status {
    pub status: crate::domains::volume::Status,
}

#[derive(Clone)]
pub struct Plugin {
    base_path: PathBuf,
    volumes: Volumes,
}

impl Plugin {
    pub fn new(base_path: &Path) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
            volumes: Volumes::new(),
        }
    }
}

#[async_trait::async_trait]
impl Driver for Plugin {
    type Error = Error;
    type Status = Status;
    type Opts = RawRepo;

    async fn capabilities(&self) -> Result<Scope, Self::Error> {
        Ok(Scope::Global)
    }

    async fn path(&self, name: &str) -> Result<Option<PathBuf>, Self::Error> {
        let Some(volume) = self.volumes.read(name).await else {
            eprintln!("WARN: Volume named {} not found", name);
            return Ok(None);
        };

        Ok(volume.path.clone())
    }

    async fn get(&self, name: &str) -> Result<VolumeInfo<Self::Status>, Self::Error> {
        let volume = self.volumes.try_read(name).await?;
        Ok(VolumeInfo {
            mountpoint: volume.path.clone(),
            status: Status {
                status: volume.status.clone(),
            },
        })
    }

    async fn list(&self) -> Result<Vec<ItemVolume>, Self::Error> {
        let list = self.volumes.read_all().await;
        Ok(list
            .iter()
            .map(|v| ItemVolume {
                name: v.name.clone(),
                mountpoint: v.path.clone(),
            })
            .collect())
    }

    async fn create(&self, name: &str, opts: Option<Self::Opts>) -> Result<(), Self::Error> {
        self.volumes.create(name, opts).await?;
        Ok(())
    }

    async fn remove(&self, name: &str) -> Result<(), Self::Error> {
        let Some(volume) = self.volumes.remove(name).await else {
            eprintln!("WARN: Volume named {} not found", name);
            return Ok(());
        };

        remove_dir_if_exists(volume.path.clone()).await?;

        Ok(())
    }
    async fn mount(&self, name: &str, id: &str) -> Result<PathBuf, Self::Error> {
        let mut volume = self.volumes.try_write(name).await?;

        if let Some(path) = volume.path.clone() {
            println!("Repository {} already cloned.", name);
            if volume.repo.refetch {
                println!("Attempting to refetch repository {} for id {}.", name, id);
                git::refetch(&path).await?;
            }
            volume.containers.insert(id.to_string());
            return Ok(path);
        }

        let path = volume.create_path_from(&self.base_path);
        if path.exists() {
            println!("Repository directory {:?} already exists. Remooving", &path);
            fs::remove_dir_all(&path).await.map_io_error(&path)?;
        }
        git::clone(&path, &volume.repo).await?;

        volume.containers.insert(id.to_string());
        volume.status = VolumeStatus::Clonned;

        println!("Volume {} mounted successfully.", name);
        Ok(path)
    }

    async fn unmount(&self, name: &str, id: &str) -> Result<(), Self::Error> {
        let Some(mut volume) = self.volumes.write(name).await else {
            eprintln!("WARN: Volume named {} not found", name);
            return Ok(());
        };

        volume.containers.remove(id);

        if !volume.containers.is_empty() {
            println!(
                "Volume {} still in use by containers. container_count={}",
                name,
                volume.containers.len(),
            );
            return Ok(());
        }

        volume.status = VolumeStatus::Cleared;
        remove_dir_if_exists(volume.path.clone()).await?;
        volume.path = None;

        println!("Volume {} unmounted successfully.", name);
        Ok(())
    }
}

async fn remove_dir_if_exists(path: Option<PathBuf>) -> crate::result::Result<()> {
    if let Some(path) = path
        && path.exists()
    {
        println!("Attempting to remove directory {:?}", &path);
        fs::remove_dir_all(&path).await.map_io_error(&path)?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use axum::extract::State;
    use tempfile::{Builder as TempBuilder, TempDir};
    use tokio::sync::OwnedRwLockWriteGuard;

    use crate::domains::volume::{Volume, test::VOLUME_NAME};

    use super::*;

    impl Plugin {
        pub fn stub() -> Self {
            Self::new(&std::env::temp_dir())
        }

        pub fn temp() -> (Self, TempDir) {
            let temp = TempBuilder::new().prefix("temp-gitvol-").tempdir().unwrap();
            let plugin = Self::new(&temp.path());
            (plugin, temp)
        }

        pub async fn create_volume(&self, name: &str) -> Result<(), Error> {
            self.create(name, Some(RawRepo::stub())).await?;
            Ok(())
        }

        pub async fn set_path(&self, name: &str, path: &Path) {
            let mut volume = self.volumes.try_write(name).await.unwrap();
            volume.path = Some(path.to_path_buf());
        }

        pub async fn stub_with_volume() -> Self {
            let plugin = Self::stub();
            plugin
                .create(VOLUME_NAME, Some(RawRepo::stub()))
                .await
                .unwrap();
            plugin
        }

        pub async fn temp_with_volume() -> (Self, TempDir) {
            let (plugin, temp) = Self::temp();
            plugin
                .create(VOLUME_NAME, Some(RawRepo::stub()))
                .await
                .unwrap();
            (plugin, temp)
        }

        pub async fn stub_with_path(path: &Path) -> Self {
            let plugin = Self::stub_with_volume().await;
            plugin.set_path(VOLUME_NAME, path).await;
            plugin
        }

        pub async fn read(&self, name: &str) -> Option<Volume> {
            let vol = self.volumes.read(name).await;
            vol.map(|v| v.clone())
        }

        pub async fn try_write(&self, name: &str) -> OwnedRwLockWriteGuard<Volume> {
            self.volumes.try_write(name).await.unwrap()
        }

        pub fn req(&self) -> State<Self> {
            State(self.clone())
        }
    }
}
