use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self},
    sync::{Mutex, RwLock},
};

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Opt {
    pub url: String,
    pub branch: Option<String>,
    pub reload: bool,
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    pub opt: Opt,
    pub used_ids: HashSet<String>,
    pub is_locked: bool,
}

#[derive(Debug)]
pub struct Volume {
    path: PathBuf,
    repo: Option<Repo>,
}

impl Volume {
    fn get_repo(&self) -> Result<&Repo> {
        let Some(repo) = &self.repo else {
            anyhow::bail!("repo is missing");
        };

        Ok(repo)
    }

    fn get_name(&self) -> Result<String> {
        Ok(self.get_repo()?.name.clone())
    }

    async fn save(&self) -> Result<()> {
        let repo = self.get_repo()?;
        let repo_path = self.path.join("repo.json");
        let content =
            serde_json::to_string(repo).context("failed to serialize repo before saving")?;
        fs::write(repo_path, content)
            .await
            .context("failed to write repo to file")?;

        Ok(())
    }

    fn get_repo_mut(&mut self) -> Result<&mut Repo> {
        let Some(repo) = &mut self.repo else {
            anyhow::bail!("repo is missing");
        };

        Ok(repo)
    }

    fn is_locked(&self) -> Result<bool> {
        Ok(self.get_repo()?.is_locked)
    }

    async fn lock_repo(&mut self, is_locked: bool) -> Result<()> {
        let repo = self
            .get_repo_mut()
            .context("failed to update repo lock state")?;
        if repo.is_locked != is_locked {
            repo.is_locked = is_locked;
            self.save()
                .await
                .context("failed to save repo after changing lock state")?;
        }

        Ok(())
    }

    async fn load_repo(&mut self) -> Result<()> {
        let path = self.path.join("repo.json");
        anyhow::ensure!(path.exists(), "repo file [{:?}] does not exists", path);
        anyhow::ensure!(
            path.is_file(),
            "repo file [{:?}] is not a regular file",
            path
        );

        let content = fs::read(path).await.context("failed to read repo.json")?;
        let content = String::from_utf8(content)
            .context("failed to convert repo.json contents to UTF-8 string")?;
        let repo: Repo =
            serde_json::from_str(&content).context("failed to deserialize repo.json")?;

        self.repo = Some(repo);
        Ok(())
    }
}

type LockedVolume = Arc<Mutex<Volume>>;
type LockedVolumes = Arc<RwLock<HashMap<String, LockedVolume>>>;
/// /// Map of (mount_name, container_id) → repo_hash
type LockedIdsRelations = Arc<RwLock<HashMap<(String, String), String>>>;

#[derive(Debug)]
pub struct Store {
    base_path: PathBuf,
    volumes: LockedVolumes,
    ids_relations: LockedIdsRelations,
}

impl Store {
    pub fn new(base_path: &PathBuf) -> Self {
        Self {
            base_path: base_path.clone(),
            volumes: Arc::new(RwLock::new(HashMap::new())),
            ids_relations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn exists(&self, hash: &str) -> bool {
        let volumes = self.volumes.read().await;
        volumes.contains_key(hash)
    }

    async fn get_by_hash(&self, hash: &str) -> Option<LockedVolume> {
        let volumes = self.volumes.read().await;
        volumes.get(hash).cloned()
    }

    async fn get_by_name(&self, name: &str, id: &str) -> Option<LockedVolume> {
        let ids_relations = self.ids_relations.read().await;
        let hash = ids_relations.get(&(name.to_string(), id.to_string()));
        let Some(hash) = hash else {
            return None;
        };

        self.get_by_hash(hash).await
    }

    async fn delete_volume_by_hash(&self, hash: &str) -> Result<()> {
        let volume = {
            let mut volumes = self.volumes.write().await;
            let volume = volumes.remove(hash);
            volume
        };

        if let Some(volume) = &volume {
            let (used_ids, name, path) = {
                let vol = volume.lock().await;
                let repo = vol.get_repo()?;
                (repo.used_ids.clone(), repo.name.clone(), vol.path.clone())
            };
            if path.exists() {
                fs::remove_dir_all(&path)
                    .await
                    .context("Trying remove volume directory")?;
            }

            if !used_ids.is_empty() {
                let mut ids_relations = self.ids_relations.write().await;
                for id in &used_ids {
                    ids_relations.remove(&(name.clone(), id.clone()));
                }
            }
        }

        Ok(())
    }

    pub async fn delete_volume_by_name(&self, name: &str, id: &str) -> Result<()> {
        // SAFETY: it's critical to drop the `ids_relations` guard BEFORE calling `delete_volume_by_hash`,
        // because that method acquires a write-lock on `ids_relations`.
        // Holding a read-lock here would cause a deadlock.
        // Don’t let the guard live — it bites.
        //
        // P.S. Don’t ask how much older I got before figuring this out.
        let hash = {
            let ids_relations = self.ids_relations.read().await;
            ids_relations
                .get(&(name.to_string(), id.to_string()))
                .cloned()
        };

        let Some(hash) = hash else {
            return Ok(());
        };

        self.delete_volume_by_hash(&hash).await?;

        Ok(())
    }

    async fn remember_ids(&self, name: &str, hash: &str, ids: &HashSet<String>) {
        let mut ids_relations = self.ids_relations.write().await;
        for id in ids {
            ids_relations.insert((name.to_string(), id.to_string()), hash.to_string());
        }
    }

    async fn create_volume_inner(
        &self,
        hash: &str,
        repo: Option<Repo>,
    ) -> Result<Arc<Mutex<Volume>>> {
        let path = self.base_path.join(hash);
        let volume = {
            let mut volumes = self.volumes.write().await;

            let volume = Arc::new(Mutex::new(Volume {
                path: path.clone(),
                repo,
            }));
            volumes.insert(hash.to_string(), Arc::clone(&volume));
            volume
        };

        if !path.exists() {
            fs::create_dir_all(&path)
                .await
                .context("failed to create directory for new volume")?;
        }

        Ok(volume)
    }

    pub async fn create_volume(
        &self,
        hash: &str,
        name: &str,
        repo: Repo,
    ) -> Result<Arc<Mutex<Volume>>> {
        let volume = {
            let ids = repo.used_ids.clone();
            let volume = self
                .create_volume_inner(hash, Some(repo))
                .await
                .context("failed to create volume")?;

            self.remember_ids(name, hash, &ids).await;
            volume
        };
        Ok(volume)
    }
    async fn create_draft_volume(&self, hash: &str) -> Result<Arc<Mutex<Volume>>> {
        let volume = self
            .create_volume_inner(hash, None)
            .await
            .context("failed to create draft volume")?;

        Ok(volume)
    }

    async fn load(base_path: &PathBuf) -> Result<Self> {
        let store = Self::new(base_path);

        if !base_path.exists() {
            return Ok(store);
        }
        anyhow::ensure!(
            base_path.is_dir(),
            "Pathname [{:?}] must be a directory",
            &base_path
        );

        let mut entries = fs::read_dir(&base_path).await.with_context(|| {
            format!(
                "failed to read directory [{:?}] to load volumes",
                &base_path
            )
        })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .context("failed to read entry in volume directory")?
        {
            let path = entry.path();
            let filename = entry.file_name();
            let Some(filename) = filename.to_str() else {
                continue;
            };
            let metadata = entry
                .metadata()
                .await
                .with_context(|| format!("failed to load metadata for path [{:?}]", &path))?;

            if !metadata.is_dir() {
                fs::remove_file(&path)
                    .await
                    .with_context(|| format!("removing non-directory file [{:?}]", &path))?;
                continue;
            }

            let repo_dir = path.join("repo");
            if !repo_dir.exists() {
                fs::remove_dir_all(&path).await.with_context(|| {
                    format!("removing volume [{:?}]: missing 'repo' directory", &path)
                })?;
                continue;
            }

            let volume = store
                .create_draft_volume(filename)
                .await
                .context("failed to create volume during store initialization")?;
            let mut volume = volume.lock().await;

            if let Err(_error) = volume
                .load_repo()
                .await
                .context("failed to load repo.json during initialization")
            {
                fs::remove_dir_all(&path).await.with_context(|| {
                    format!("removing volume [{:?}]: could not read repo.json", &path)
                })?;

                // FIXME: Not the best solution
                let mut volumes = store.volumes.write().await;
                volumes.remove(filename);

                continue;
            } else {
                let repo = volume.get_repo()?;
                store
                    .remember_ids(&repo.name, filename, &repo.used_ids)
                    .await;
            }
        }

        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::str::FromStr;
    use tokio::fs;

    fn create_repo(name: &str, id: &str) -> Repo {
        let mut ids = HashSet::new();
        ids.insert(id.to_string());
        Repo {
            name: name.to_string(),
            opt: Opt {
                url: "https://example.com".to_string(),
                branch: None,
                reload: false,
            },
            used_ids: ids,
            is_locked: false,
        }
    }

    fn create_volume(path: &PathBuf, repo: Option<(&str, &str)>) -> Volume {
        let repo = repo.map(|(name, id)| create_repo(name, id));
        Volume {
            path: path.clone(),
            repo,
        }
    }

    fn create_volume_by_defpath(repo: Option<(&str, &str)>) -> Volume {
        create_volume(&PathBuf::from_str("/tmp").unwrap(), repo)
    }

    fn assert_err<T: std::fmt::Debug>(result: Result<T>, contain_str: &str) {
        assert!(result.is_err());
        let error = result.unwrap_err();
        let message = format!("{:?}", error);

        assert!(
            message.contains(contain_str),
            "Unexpected error message: [{}]. should conttains: [{}]",
            message,
            contain_str
        );
    }

    mod volume {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn get_repo_returns_reference_when_present() {
            let volume = create_volume_by_defpath(Some(("volume_name", "container_id")));
            let repo = volume.get_repo().unwrap();

            assert_eq!("volume_name", repo.name);
            assert!(repo.used_ids.contains("container_id"));
        }

        #[test]
        fn get_repo_fails_when_missing() {
            let volume = create_volume_by_defpath(None);
            let result = volume.get_repo();
            assert_err(result, "is missing");
        }

        #[test]
        fn get_name_returns_repo_name_when_present() {
            let volume = create_volume_by_defpath(Some(("volume_name", "container_id")));
            let name = volume.get_name().unwrap();

            assert_eq!("volume_name", name);
        }

        #[test]
        fn get_name_fails_when_repo_missing() {
            let volume = create_volume_by_defpath(None);
            let result = volume.get_name();
            assert_err(result, "is missing");
        }

        #[tokio::test]
        async fn save_writes_repo_to_json_file_successfully() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let repo_path: PathBuf = tmp_dir.path().into();
            let repo_json = repo_path.join("repo.json");
            let repo = create_repo("volume_name", "id");
            let repo_string = serde_json::to_string(&repo).unwrap();
            let volume = Volume {
                path: repo_path,
                repo: Some(repo),
            };

            volume.save().await.unwrap();

            assert!(repo_json.exists());

            let content = fs::read(repo_json).await.unwrap();
            assert_eq!(String::from_utf8(content).unwrap(), repo_string);
        }

        #[tokio::test]
        async fn save_fails_when_repo_missing() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let repo_path: PathBuf = tmp_dir.path().into();
            let volume = create_volume(&repo_path, None);

            let result = volume.save().await;
            assert_err(result, "is missing");
        }

        #[tokio::test]
        async fn save_fails_when_write_fails() {
            let tmp_dir = tempfile::tempdir().unwrap();
            // non-existent path
            let repo_path = tmp_dir.path().join("some");
            let volume = create_volume(&repo_path, Some(("volume_name", "container_id")));

            let result = volume.save().await;
            assert_err(result, "failed to write");
        }

        #[test]
        fn get_repo_mut_returns_mutable_reference_when_present() {
            let mut volume = create_volume_by_defpath(Some(("volume_name", "container_id")));
            let repo = volume.get_repo_mut().unwrap();

            repo.name = "new name".into();
            let _ = repo;

            let repo = volume.get_repo().unwrap();
            assert_eq!(repo.name, "new name");
        }

        #[test]
        fn get_repo_mut_fails_when_missing() {
            let mut volume = create_volume_by_defpath(None);
            let result = volume.get_repo_mut();
            assert_err(result, "is missing");
        }

        #[tokio::test]
        async fn is_locked_returns_true_when_locked() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let mut volume = create_volume(
                &tmp_dir.path().into(),
                Some(("volume_name", "container_id")),
            );
            volume.lock_repo(true).await.unwrap();

            let is_locked = volume.is_locked().unwrap();
            assert!(is_locked);
        }

        #[tokio::test]
        async fn is_locked_returns_false_when_unlocked() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let mut volume = create_volume(
                &tmp_dir.path().into(),
                Some(("volume_name", "container_id")),
            );
            volume.lock_repo(false).await.unwrap();

            let is_locked = volume.is_locked().unwrap();
            assert!(!is_locked);
        }

        #[tokio::test]
        async fn is_locked_fails_when_repo_missing() {
            let mut volume = create_volume_by_defpath(None);

            let lock_result = volume.lock_repo(true).await;
            assert_err(lock_result, "failed to update");

            let is_locked_result = volume.is_locked();
            assert_err(is_locked_result, "is missing");
        }

        #[tokio::test]
        async fn load_repo_reads_json_and_parses_successfully() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let path: PathBuf = tmp_dir.path().into();
            let repo = create_repo("volume_repo", "container_id");
            fs::write(
                path.join("repo.json"),
                serde_json::to_string(&repo).unwrap(),
            )
            .await
            .unwrap();
            let mut volume = create_volume(&path, None);
            volume.load_repo().await.unwrap();

            let loaded_repo = volume.get_repo().unwrap();

            assert_eq!(&repo, loaded_repo);
        }

        #[tokio::test]
        async fn load_repo_fails_when_file_missing() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let path: PathBuf = tmp_dir.path().into();
            let mut volume = create_volume(&path, None);

            let result = volume.load_repo().await;
            assert_err(result, "not exists");
        }

        #[tokio::test]
        async fn load_repo_fails_when_not_a_file() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let path: PathBuf = tmp_dir.path().into();
            let mut volume = create_volume(&path, None);
            fs::create_dir_all(path.join("repo.json")).await.unwrap();

            let result = volume.load_repo().await;
            assert_err(result, "is not a regular file");
        }

        #[tokio::test]
        async fn load_repo_fails_when_read_fails() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().to_path_buf();
            let bad_file = path.join("repo.json");

            fs::write(&bad_file, "{}").await.unwrap();
            #[cfg(unix)]
            std::fs::set_permissions(&bad_file, std::fs::Permissions::from_mode(0o000)).unwrap();

            let mut volume = create_volume(&path, None);

            let result = volume.load_repo().await;
            assert_err(result, "Permission denied");

            #[cfg(unix)]
            std::fs::set_permissions(&bad_file, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        #[tokio::test]
        async fn load_repo_fails_when_deserialization_fails() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().to_path_buf();
            let bad_file = path.join("repo.json");

            let invalid_json = r#"{
                "name": "test",
                "used_ids": [],
                "is_locked": false,
                "opt": 
            }"#;

            fs::write(&bad_file, invalid_json).await.unwrap();

            let mut volume = create_volume(&path, None);

            let result = volume.load_repo().await;
            assert_err(result, "failed to deserialize");
        }
    }

    mod store {
        use super::*;

        mod create {
            use super::*;

            #[test]
            fn creates_store_with_empty_maps() {
                let store = Store::new(&PathBuf::from_str("/tmp").unwrap());
                assert_eq!(store.base_path, PathBuf::from_str("/tmp").unwrap());
            }

            #[tokio::test]
            async fn create_volume_inner_creates_directory_and_registers_volume() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let store = Store::new(&path.to_path_buf());
                let name = "volume_name";
                let id = "container_id";
                let hash = "some_hash_string";
                let repo = create_repo(name, id);

                let volume = store.create_volume_inner(hash, Some(repo)).await.unwrap();

                assert!(path.join(hash).exists());

                let volume = volume.lock().await;
                assert_eq!(name, volume.get_name().unwrap());

                let volumes = store.volumes.read().await;
                assert!(volumes.contains_key(hash));
            }

            // FIXME: how tested it?
            #[tokio::test]
            async fn create_volume_inner_fails_when_dir_creation_fails() {}

            #[tokio::test]
            async fn create_volume_stores_volume_and_remembers_ids() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let store = Store::new(&path.to_path_buf());
                let name = "volume_name";
                let id = "container_id";
                let hash = "some_hash_string";
                let repo = create_repo(name, id);

                let volume = store.create_volume(hash, name, repo).await.unwrap();

                assert!(path.join(hash).exists());

                let volume = volume.lock().await;
                assert_eq!(name, volume.get_name().unwrap());

                let volumes = store.volumes.read().await;
                assert!(volumes.contains_key(hash));

                let ids_relations = store.ids_relations.read().await;

                let relations_hash = ids_relations.get(&(name.to_string(), id.to_string()));
                assert!(relations_hash.is_some());
                let relations_hash = relations_hash.unwrap();
                assert_eq!(hash, relations_hash);
            }

            // FIXME: how tested it?
            #[tokio::test]
            async fn create_volume_fails_when_inner_fails() {}

            #[tokio::test]
            async fn create_draft_volume_creates_empty_volume() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let store = Store::new(&path.to_path_buf());
                let hash = "some_hash_string";

                let volume = store.create_draft_volume(hash).await.unwrap();
                let volume = volume.lock().await;

                assert!(volume.repo.is_none());
                assert_eq!(path.join(hash), volume.path);
            }
        }

        mod get_and_exists {
            use super::*;

            #[tokio::test]
            async fn exists_returns_false_for_missing_volume() {
                let temp = tempfile::tempdir().unwrap();
                let store = Store::new(&temp.path().to_path_buf());
                let hash = "some hash string";

                let exists = store.exists(hash).await;

                assert!(!exists, "non-existent value with hash [{}] found", hash);
            }

            #[tokio::test]
            async fn exists_returns_true_for_existing_volume() {
                let temp = tempfile::tempdir().unwrap();
                let store = Store::new(&temp.path().to_path_buf());
                let hash = "some hash string";

                let _ = store.create_draft_volume(hash).await.unwrap();
                let exists = store.exists(hash).await;

                assert!(exists, "volume with hash [{}] not found", hash);
            }

            #[tokio::test]
            async fn get_by_hash_returns_none_when_missing() {
                let temp = tempfile::tempdir().unwrap();
                let store = Store::new(&temp.path().to_path_buf());
                let hash = "some hash string";

                let result = store.get_by_hash(hash).await;

                assert!(
                    result.is_none(),
                    "non-existent volume with hash [{}] found",
                    hash
                );
            }

            #[tokio::test]
            async fn get_by_hash_returns_some_when_exists() {
                let temp = tempfile::tempdir().unwrap();
                let store = Store::new(&temp.path().to_path_buf());
                let hash = "some hash string";

                let _ = store.create_draft_volume(hash).await.unwrap();
                let result = store.get_by_hash(hash).await;

                assert!(result.is_some(), "volume with hash [{}] not found", hash);
            }

            #[tokio::test]
            async fn get_by_name_returns_none_when_unmapped() {
                let temp = tempfile::tempdir().unwrap();
                let store = Store::new(&temp.path().to_path_buf());

                let result = store.get_by_name("name", "id").await;

                assert!(result.is_none(), "non-existent volume found");
            }

            #[tokio::test]
            async fn get_by_name_returns_volume_when_mapped() {
                let temp = tempfile::tempdir().unwrap();
                let store = Store::new(&temp.path().to_path_buf());
                let name = "volume_name";
                let id = "container_id";
                let hash = "some hash string";
                let repo = create_repo(name, id);

                let _ = store.create_volume(hash, name, repo).await.unwrap();
                let result = store.get_by_name(name, id).await;
                assert!(result.is_some(), "volume not found");
            }
        }

        #[tokio::test]
        async fn remember_ids_adds_all_ids_for_volume() {
            let temp = tempfile::tempdir().unwrap();
            let store = Store::new(&temp.path().to_path_buf());
            let name = "volume_name";
            let id = "container_id";
            let hash = "some_hash_string";

            store
                .remember_ids(name, hash, &HashSet::from_iter(vec![id.to_string()]))
                .await;

            let ids_relations = store.ids_relations.read().await;

            let relations_hash = ids_relations.get(&(name.to_string(), id.to_string()));
            assert!(relations_hash.is_some());
            let relations_hash = relations_hash.unwrap();
            assert_eq!(hash, relations_hash);
        }

        mod delete {
            use super::*;

            #[tokio::test]
            async fn delete_volume_by_hash_removes_volume_and_related_ids() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let store = Store::new(&path.to_path_buf());
                let name = "volume_name";
                let id = "container_id";
                let hash = "some_hash_string";
                let repo = create_repo(name, id);

                let _ = store.create_volume(hash, name, repo).await.unwrap();
                store.delete_volume_by_hash(hash).await.unwrap();

                let maybe_vol = store.get_by_hash(hash).await;
                assert!(maybe_vol.is_none());

                let ids_relations = store.ids_relations.read().await;
                assert_eq!(0, ids_relations.len())
            }

            #[tokio::test]
            async fn delete_volume_by_hash_skips_when_not_found() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let store = Store::new(&path.to_path_buf());

                store
                    .delete_volume_by_hash("some non-existing hash")
                    .await
                    .unwrap();
            }

            #[tokio::test]
            async fn delete_volume_by_hash_handles_missing_repo() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let store = Store::new(&path.to_path_buf());
                let hash = "some_hash_string";

                let _ = store.create_volume_inner(hash, None).await.unwrap();

                let result = store.delete_volume_by_hash(hash).await;

                assert_err(result, "repo is missing");
            }

            #[tokio::test]
            async fn delete_volume_by_name_removes_volume_if_mapped() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let store = Store::new(&path.to_path_buf());
                let name = "mapped";
                let id = "container_id";
                let hash = "mapped_hash";
                let repo = create_repo(name, id);

                let _ = store.create_volume(hash, name, repo).await.unwrap();
                store.delete_volume_by_name(name, id).await.unwrap();

                let maybe_vol = store.get_by_hash(hash).await;
                assert!(maybe_vol.is_none());

                let ids_relations = store.ids_relations.read().await;
                assert_eq!(0, ids_relations.len())
            }

            #[tokio::test]
            async fn delete_volume_by_name_does_nothing_if_not_mapped() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let store = Store::new(&path.to_path_buf());

                let _ = store.create_draft_volume("some_hash").await.unwrap();

                let result = store.delete_volume_by_name("name", "id").await;
                assert!(result.is_ok());
            }
        }

        mod load {
            use super::*;

            #[tokio::test]
            async fn returns_empty_store_if_dir_missing() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().join("non_existent_dir");

                let store = Store::load(&path).await.unwrap();

                let volumes = store.volumes.read().await;
                assert_eq!(0, volumes.len());
            }

            #[tokio::test]
            async fn removes_non_directory_entries_in_base_path() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let trash_file_path = path.join("some.txt");
                fs::write(&trash_file_path, "contents").await.unwrap();

                let store = Store::load(&path).await.unwrap();

                let volumes = store.volumes.read().await;
                assert_eq!(0, volumes.len());
                assert!(!trash_file_path.exists());
            }

            #[tokio::test]
            async fn removes_directories_missing_repo_subdir() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let repo_path = path.join("some-hash");
                fs::create_dir_all(&repo_path).await.unwrap();

                let json = r#"{
                    "name": "name",
                    "opt": {
                        "url": "http://some.git",
                        "reload": true
                    },
                    "used_ids": [],
                    "is_locked": false
                }"#;

                fs::write(repo_path.join("repo.json"), json).await.unwrap();

                let store = Store::load(&path).await.unwrap();

                let volumes = store.volumes.read().await;
                assert_eq!(0, volumes.len());
                assert!(!repo_path.exists());
            }

            #[tokio::test]
            async fn removes_volumes_with_invalid_or_unreadable_repo_json() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let repo_path = path.join("some-hash");
                fs::create_dir_all(&repo_path.join("repo")).await.unwrap();

                // incorrect json
                let json = r#"{
                    "name": "name,
                    "opt": {
                        "url": "http://some.git",
                        "reload": true
                    },
                    "used_ids": [],
                    "is_locked": false
                }"#;

                fs::write(repo_path.join("repo.json"), json).await.unwrap();

                let store = Store::load(&path).await.unwrap();
                let volumes = store.volumes.read().await;
                assert_eq!(0, volumes.len());
                assert!(!repo_path.exists());
            }

            #[tokio::test]
            async fn restores_valid_volumes_and_rebuilds_ids_relations() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().to_path_buf();
                let repo_1_path = path.join("some-hash-1");
                let repo_2_path = path.join("some-hash-2");

                fs::create_dir_all(repo_1_path.join("repo")).await.unwrap();
                fs::create_dir_all(repo_2_path.join("repo")).await.unwrap();

                let json1 = r#"{
                    "name": "name 1",
                    "opt": {
                        "url": "http://some.git",
                        "reload": true
                    },
                    "used_ids": [],
                    "is_locked": false
                }"#;

                fs::write(repo_1_path.join("repo.json"), json1)
                    .await
                    .unwrap();

                let json2 = r#"{
                    "name": "name 2",
                    "opt": {
                        "url": "http://some.git",
                        "reload": true
                    },
                    "used_ids": [],
                    "is_locked": false
                }"#;

                fs::write(repo_2_path.join("repo.json"), json2)
                    .await
                    .unwrap();

                let store = Store::load(&path).await.unwrap();

                let volumes = store.volumes.read().await;

                let volume1 = store.get_by_hash("some-hash-1").await.unwrap();
                let volume1 = volume1.lock().await;
                let name1 = volume1.get_name().unwrap();
                let volume2 = store.get_by_hash("some-hash-2").await.unwrap();
                let volume2 = volume2.lock().await;
                let name2 = volume2.get_name().unwrap();

                assert_eq!(2, volumes.len());
                assert_eq!("name 1", name1);
                assert_eq!("name 2", name2);
            }
        }
    }
}
