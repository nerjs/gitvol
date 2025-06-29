use std::{path::PathBuf, str::FromStr};

use anyhow::Result;
use sha2::{Digest, Sha256};
use url::Url;

use crate::global_config::GlobalConfig;

#[derive(Debug)]
pub struct MountConfig {
    pub id: String,
    pub repo_base_dir: PathBuf,
    pub repo_dir: PathBuf,
    pub repo: String,
    pub branch: Option<String>,
    pub updatable: bool,
}

#[derive(Debug)]
pub struct MountConfigRequest {
    pub id: String,
    pub repo: String,
    pub branch: Option<String>,
    pub updatable: Option<bool>,
}

impl MountConfig {
    pub fn new(global_config: &GlobalConfig, mount_request: MountConfigRequest) -> Result<Self> {
        let MountConfigRequest {
            id,
            repo,
            branch,
            updatable,
        } = mount_request;
        let mut repo_base_dir = global_config.base_dir.clone();

        let (repo, url_branch) = match Url::from_str(&repo) {
            Ok(mut url) => {
                let fragment = url.fragment().map(|s| s.to_string());
                url.set_fragment(None);

                (url.to_string(), fragment)
            }
            Err(_) => (repo, None),
        };

        anyhow::ensure!(
            branch.clone().and(url_branch.clone()).is_none(),
            "ref (branch or tag) can be specified either in url or as a separate parameter."
        );

        let branch = branch.or(url_branch).and_then(|s| {
            let res = s.trim();
            if res.is_empty() {
                None
            } else {
                Some(res.to_string())
            }
        });

        let updatable = updatable.unwrap_or(false);
        let updatable_prefix = if updatable {
            "updatable"
        } else {
            "nonupdatable"
        };

        repo_base_dir.push(updatable_prefix);

        let repo_hash = Sha256::digest(&repo);
        repo_base_dir.push(format!("{:x}", repo_hash));
        let mut repo_dir = repo_base_dir.clone();
        repo_dir.push(branch.clone().unwrap_or("_".to_string()));

        Ok(Self {
            id,
            repo_base_dir,
            repo_dir,
            repo,
            updatable,
            branch,
        })
    }

    pub fn set_branch(&mut self, branch: &str) {
        self.branch = Some(branch.to_string());
        self.repo_dir.pop();
        self.repo_dir.push(branch);
        dbg!(branch, self.repo_dir.clone());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn mount_config_basic() {
        let temp = tempdir().unwrap();
        let global_config = GlobalConfig::new(temp.path().to_str().unwrap(), "gitvol")
            .await
            .unwrap();
        let req = MountConfigRequest {
            id: "test1".into(),
            repo: "https://github.com/user/repo.git".into(),
            branch: None,
            updatable: None,
        };

        let config = MountConfig::new(&global_config, req).unwrap();

        assert_eq!(config.id, "test1");
        assert_eq!(config.branch, None);
        assert_eq!(config.updatable, false);
        assert!(config.repo_base_dir.starts_with(global_config.base_dir));
        assert!(
            config
                .repo_base_dir
                .to_str()
                .unwrap()
                .contains("nonupdatable")
        );
        assert!(config.repo_dir.starts_with(config.repo_base_dir));
        assert!(config.repo_dir.ends_with("_"));
    }

    #[tokio::test]
    async fn mount_config_with_branch_param() {
        let temp = tempdir().unwrap();
        let global_config = GlobalConfig::new(temp.path().to_str().unwrap(), "gitvol")
            .await
            .unwrap();

        let req = MountConfigRequest {
            id: "test2".into(),
            repo: "https://github.com/user/repo.git".into(),
            branch: Some("main".into()),
            updatable: None,
        };

        let config = MountConfig::new(&global_config, req).unwrap();
        assert_eq!(config.branch.as_deref(), Some("main"));
        assert!(config.repo_dir.ends_with("main"));
    }

    #[tokio::test]
    async fn mount_config_with_fragment_branch() {
        let temp = tempdir().unwrap();
        let global_config = GlobalConfig::new(temp.path().to_str().unwrap(), "gitvol")
            .await
            .unwrap();

        let req = MountConfigRequest {
            id: "test3".into(),
            repo: "https://github.com/user/repo.git#dev".into(),
            branch: None,
            updatable: None,
        };

        let config = MountConfig::new(&global_config, req).unwrap();
        assert_eq!(config.branch.as_deref(), Some("dev"));
        assert!(config.repo_dir.ends_with("dev"));
    }

    #[tokio::test]
    async fn mount_config_conflicting_branches() {
        let temp = tempdir().unwrap();
        let global_config = GlobalConfig::new(temp.path().to_str().unwrap(), "gitvol")
            .await
            .unwrap();

        let req = MountConfigRequest {
            id: "test4".into(),
            repo: "https://github.com/user/repo.git#dev".into(),
            branch: Some("main".into()),
            updatable: None,
        };

        let result = MountConfig::new(&global_config, req);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("can be specified either"));
    }

    #[tokio::test]
    async fn mount_config_updatable_true() {
        let temp = tempdir().unwrap();
        let global_config = GlobalConfig::new(temp.path().to_str().unwrap(), "gitvol")
            .await
            .unwrap();

        let req = MountConfigRequest {
            id: "test5".into(),
            repo: "https://github.com/user/repo.git".into(),
            branch: None,
            updatable: Some(true),
        };

        let config = MountConfig::new(&global_config, req).unwrap();
        assert_eq!(config.updatable, true);
        assert!(config.repo_base_dir.to_str().unwrap().contains("updatable"));
    }

    #[tokio::test]
    async fn set_branch_updates_path() {
        let temp = tempdir().unwrap();
        let global_config = GlobalConfig::new(temp.path().to_str().unwrap(), "gitvol")
            .await
            .unwrap();

        let req = MountConfigRequest {
            id: "test6".into(),
            repo: "https://github.com/user/repo.git".into(),
            branch: None,
            updatable: None,
        };

        let mut config = MountConfig::new(&global_config, req).unwrap();
        config.set_branch("feature-x");

        assert_eq!(config.branch.as_deref(), Some("feature-x"));
        assert!(config.repo_dir.ends_with("feature-x"));
    }

    #[tokio::test]
    async fn nonstandard_repo_path() {
        let temp = tempdir().unwrap();
        let global_config = GlobalConfig::new(temp.path().to_str().unwrap(), "gitvol")
            .await
            .unwrap();

        let req = MountConfigRequest {
            id: "test7".into(),
            repo: "git@github.com:user/repo.git".into(),
            branch: None,
            updatable: None,
        };

        let config = MountConfig::new(&global_config, req).unwrap();
        assert_eq!(config.repo, "git@github.com:user/repo.git".to_string());
    }
}
