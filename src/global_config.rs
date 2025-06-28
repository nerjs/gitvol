use anyhow::{Context, Result};
use std::{path::PathBuf, str::FromStr};
use tokio::fs::{create_dir, remove_dir_all};

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub root_dir: String,
    pub plugin_name: String,
    pub base_dir: PathBuf,
}

impl GlobalConfig {
    pub async fn new(root_dir: &str, plugin_name: &str) -> Result<Self> {
        let mut base_dir = PathBuf::from_str(root_dir)
            .with_context(|| format!("create pathbuf from root {root_dir}"))?;
        anyhow::ensure!(base_dir.exists(), "root dir {root_dir} not exists");
        let plugin_name = plugin_name.trim();
        anyhow::ensure!(plugin_name.len() > 1, "plugin name cannot be empty");

        base_dir.push(plugin_name);

        if !base_dir.exists() {
            create_dir(&base_dir)
                .await
                .with_context(|| format!("Filed creating directory {:?}", base_dir.to_str()))?;
        }

        Ok(Self {
            root_dir: root_dir.to_string(),
            plugin_name: plugin_name.to_string(),
            base_dir,
        })
    }

    pub async fn clear(&self) -> Result<()> {
        if self.base_dir.exists() && self.base_dir.is_dir() {
            remove_dir_all(&self.base_dir)
                .await
                .context("remove base dir")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn creates_base_dir_successfully() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_str().unwrap();

        let plugin = GlobalConfig::new(root, "test_plugin").await.unwrap();

        assert_eq!(plugin.plugin_name, "test_plugin");
        assert!(plugin.base_dir.exists());
        assert!(plugin.base_dir.is_dir());
        assert!(plugin.base_dir.ends_with("test_plugin"));
    }

    #[tokio::test]
    async fn clears_base_dir_successfully() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_str().unwrap();

        let plugin = GlobalConfig::new(root, "plugin").await.unwrap();
        let mut base_dir = plugin.base_dir.clone();
        assert!(base_dir.exists());

        plugin.clear().await.unwrap();
        assert!(!base_dir.exists()); 


        base_dir.pop();
        assert!(base_dir.exists());
    }

    #[tokio::test]
    async fn clear_is_idempotent() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_str().unwrap();

        let plugin = GlobalConfig::new(root, "plugin").await.unwrap();

        plugin.clear().await.unwrap();
        // Second call - the directory has already been deleted
        plugin.clear().await.unwrap(); //  Shouldn't panic
    }

    #[tokio::test]
    async fn fails_on_nonexistent_root_dir() {
        let bad_root = "/definitely/does/not/exist";

        let result = GlobalConfig::new(bad_root, "plugin").await;

        assert!(result.is_err());
        let err_string = format!("{:?}", result.err().unwrap());
        assert!(err_string.contains("not exists"));
    }

    #[tokio::test]
    async fn fails_on_empty_plugin_name() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_str().unwrap();

        let result = GlobalConfig::new(root, "  ").await;
        assert!(result.is_err());

        let err_string = format!("{:?}", result.err().unwrap());
        assert!(err_string.contains("plugin name cannot be empty"));
    }
}
