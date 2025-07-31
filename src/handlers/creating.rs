use std::path::PathBuf;

use anyhow::{Context, Result};
use axum::{Json, extract::State};
use log::{debug, kv, warn};
use serde::Deserialize;
use tokio::fs;

use super::{
    result::PluginResult,
    shared_structs::{Empty, Named},
};
use crate::state::{GitvolState, Repo};

pub async fn create_handler(
    State(state): State<GitvolState>,
    Json(RawCreateRequest { name, opts }): Json<RawCreateRequest>,
) -> PluginResult<Empty> {
    debug!(name; "Attempting to create volume");
    let repo = create_repo_from_raw(opts).context("Failed to parse repository options")?;
    state
        .create(&name, repo)
        .await
        .context("Failed to create volume")?;
    debug!(name; "Volume created successfully");
    Ok(Empty)
}

pub async fn remove_handler(
    State(state): State<GitvolState>,
    Json(Named { name }): Json<Named>,
) -> PluginResult<Empty> {
    debug!(name; "Attempting to remove volume");
    let mut volumes = state.write_map().await;

    let Some(volume) = volumes.get(&name) else {
        warn!(name; "Volume not found");
        return Ok(Empty);
    };

    let volume = volume.read().await;
    remove_dir_if_exists(volume.path.clone())
        .await
        .with_context(|| format!("Failed to remove directory for volume '{}'", name))?;
    drop(volume);
    volumes.remove(&name);

    debug!(name; "Volume removed successfully");
    Ok(Empty)
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawRepo {
    pub url: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub refetch: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawCreateRequest {
    pub name: String,
    pub opts: Option<RawRepo>,
}

fn create_repo_from_raw(raw: Option<RawRepo>) -> Result<Repo> {
    let Some(RawRepo {
        url,
        branch,
        tag,
        reference,
        refetch,
    }) = raw
    else {
        anyhow::bail!("Invalid repository configuration: no options provided, git URL is required");
    };

    let Some(url) = url else {
        anyhow::bail!("Invalid repository configuration: git URL is missing");
    };

    let ref_count = [branch.is_some(), reference.is_some(), tag.is_some()]
        .iter()
        .filter(|x| **x)
        .count();
    if ref_count > 1 {
        anyhow::bail!("Only one of branch, tag, or ref parameters is allowed");
    }

    let branch = branch.or(tag).or(reference);
    let refetch = refetch.unwrap_or(false);

    debug!(url, branch, refetch; "Parsed repository options");

    Ok(Repo {
        url,
        branch,
        refetch,
    })
}

async fn remove_dir_if_exists(path: Option<PathBuf>) -> Result<()> {
    if let Some(path) = path {
        if path.exists() {
            debug!(path = kv::Value::from_debug(&path); "Attempting to remove directory");
            fs::remove_dir_all(&path)
                .await
                .with_context(|| format!("Failed to remove directory '{:?}'", path))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::tempdir;

    impl Default for RawRepo {
        fn default() -> Self {
            Self {
                url: None,
                branch: None,
                tag: None,
                reference: None,
                refetch: None,
            }
        }
    }

    #[allow(dead_code)]
    impl RawRepo {
        fn with_url(mut self, url: &str) -> Self {
            self.url = Some(url.into());
            self
        }

        fn with_branch(mut self, branch: &str) -> Self {
            self.branch = Some(branch.into());
            self
        }

        fn with_reference(mut self, reference: &str) -> Self {
            self.reference = Some(reference.into());
            self
        }

        fn with_tag(mut self, tag: &str) -> Self {
            self.tag = Some(tag.into());
            self
        }

        fn with_refetch(mut self, refetch: bool) -> Self {
            self.refetch = Some(refetch);
            self
        }
    }

    impl Default for RawCreateRequest {
        fn default() -> Self {
            Self {
                name: Default::default(),
                opts: None,
            }
        }
    }

    #[allow(dead_code)]
    impl RawCreateRequest {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                ..Default::default()
            }
        }

        fn with_opts(mut self, opts: RawRepo) -> Self {
            self.opts = Some(opts);
            self
        }

        fn with_url(self, url: &str) -> Self {
            let opts = self.opts.clone().unwrap_or_default().with_url(url);
            self.with_opts(opts)
        }

        fn with_branch(self, branch: &str) -> Self {
            let opts = self.opts.clone().unwrap_or_default().with_branch(branch);
            self.with_opts(opts)
        }

        fn with_reference(self, reference: &str) -> Self {
            let opts = self
                .opts
                .clone()
                .unwrap_or_default()
                .with_reference(reference);
            self.with_opts(opts)
        }

        fn with_tag(self, tag: &str) -> Self {
            let opts = self.opts.clone().unwrap_or_default().with_tag(tag);
            self.with_opts(opts)
        }

        fn with_refetch(self, refetch: bool) -> Self {
            let opts = self.opts.clone().unwrap_or_default().with_refetch(refetch);
            self.with_opts(opts)
        }
    }

    const VOLUME_NAME: &str = "volume_name";
    const REPO_URL: &str = "https://example.com/repo.git";

    #[tokio::test]
    async fn create_handler_success() {
        let state = GitvolState::new("/tmp".into());
        let request = RawCreateRequest::new(VOLUME_NAME).with_url(REPO_URL);

        let result = create_handler(State(state.clone()), Json(request)).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Empty);

        let volume = state.read(VOLUME_NAME).await.unwrap();

        assert_eq!(volume.name, VOLUME_NAME);
        assert_eq!(volume.repo.url, REPO_URL);
        assert_eq!(volume.repo.branch, None);
        assert_eq!(volume.repo.refetch, false);
    }

    #[tokio::test]
    async fn create_handler_missing_raw() {
        let state = GitvolState::new("/tmp".into());
        let request = RawCreateRequest::new(VOLUME_NAME);

        let result = create_handler(State(state.clone()), Json(request)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_handler_missing_url() {
        let state = GitvolState::new("/tmp".into());
        let request = RawCreateRequest::new(VOLUME_NAME).with_opts(RawRepo::default());

        let result = create_handler(State(state.clone()), Json(request)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_handler_multiple_refs() {
        let state = GitvolState::new("/tmp".into());
        let request = RawCreateRequest::new(VOLUME_NAME)
            .with_url(REPO_URL)
            .with_branch("branch")
            .with_tag("tag");

        let result = create_handler(State(state.clone()), Json(request)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_handler_state_incorrect_name_error() {
        let state = GitvolState::new("/tmp".into());
        // empty trimmed name
        let request = RawCreateRequest::new("  ").with_url(REPO_URL);

        let result = create_handler(State(state.clone()), Json(request)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_handler_state_duplicate_error() {
        let state = GitvolState::new("/tmp".into());
        state
            .create(VOLUME_NAME, Repo::new(REPO_URL))
            .await
            .unwrap();
        let request = RawCreateRequest::new(VOLUME_NAME).with_url(REPO_URL);

        let result = create_handler(State(state.clone()), Json(request)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remove_handler_success() {
        let temp = tempdir().unwrap();
        let path = temp.path().to_path_buf();
        let state = GitvolState::new("/tmp".into());
        state
            .create(VOLUME_NAME, Repo::new(REPO_URL))
            .await
            .unwrap();

        fs::create_dir_all(&path).await.unwrap();
        fs::write(path.join("some.file"), "contents").await.unwrap();
        state.set_path(VOLUME_NAME, &path).await.unwrap();

        let request = Named::new(VOLUME_NAME);

        assert!(path.exists());
        let result = remove_handler(State(state.clone()), Json(request)).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Empty);

        let volume = state.read(VOLUME_NAME).await;
        assert!(volume.is_none());
        assert!(!path.exists());
    }
}
