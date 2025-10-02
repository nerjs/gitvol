use std::path::{Path, PathBuf};

use tokio::fs;

use crate::domains::{
    cmd::{Cmd, Error as CmdError},
    repo::Repo,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Cmd(#[from] CmdError),

    #[error("Repository local path '{0}' already exists")]
    PathAlreadyExists(PathBuf),

    #[error("Repository local path '{0}' not exists")]
    PathNotExists(PathBuf),

    #[error("Failed to delete the .git directory. {0}")]
    RemoveGit(#[from] std::io::Error),
}

pub struct Git {
    cmd: Cmd,
}

impl Git {
    pub async fn init() -> Result<Self, Error> {
        let git_path = Cmd::new("which").command("git").exec().await?;
        println!("Located git executable - {}.", &git_path);
        let version = Cmd::new(&git_path).arg("--version").exec().await?;
        println!("Verified git version: {}", version);

        Ok(Self {
            cmd: Cmd::new(git_path),
        })
    }

    pub async fn clone(&self, path: &Path, repo: &Repo) -> Result<(), Error> {
        println!("trying clonning repository {}", repo);

        if path.exists() {
            return Err(Error::PathAlreadyExists(path.to_path_buf()));
        }

        let mut cmd = self.cmd.command("clone");

        cmd.arg("--depth=1");
        if let Some(branch) = &repo.branch {
            cmd.args(["--branch", branch]);
        }
        let output = cmd
            .args([&repo.url.to_string(), path.to_str().unwrap_or_default()])
            .exec()
            .await?;

        println!("git output: {}", output);

        if !repo.refetch {
            fs::remove_dir_all(path.join(".git")).await?;
        }

        println!("Succefully clonning repository {}", repo);

        Ok(())
    }

    pub async fn refetch(&self, path: &Path) -> Result<(), Error> {
        println!("trying refetch repository {:?}", path);

        if !path.exists() {
            return Err(Error::PathNotExists(path.to_path_buf()));
        }

        let git_path = path.join(".git");
        if !git_path.exists() {
            return Err(Error::PathNotExists(git_path.to_path_buf()));
        }

        self.cmd.command("fetch").current_dir(path).exec().await?;
        self.cmd.command("pull").current_dir(path).exec().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test_mocks {
    use std::{fs, path::Path, process::Command, str::FromStr};

    use tempfile::{TempDir, tempdir};

    use crate::domains::{repo::Repo, url::Url};

    #[derive(Debug)]
    pub struct TestRepo {
        temp: TempDir,
        default_branch: String
    }

    fn has_config_field(dir: &Path, field: &str) -> bool {
        let stdout = Command::new("git")
            .current_dir(dir)
            .args(["config", field])
            .output()
            .unwrap()
            .stdout;
        let result = String::from_utf8(stdout).unwrap();
        !result.trim().is_empty()
    }

    impl TestRepo {
        pub fn new() -> Self {
            let temp = TempDir::with_prefix("test-repository-").unwrap();
            let default_branch = "master".to_string();

            let current_dir = std::env::current_dir().unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["init", "--bare", "--initial-branch", &default_branch])
                .output()
                .unwrap();

            let test_repo = Self {
                temp,
                default_branch: default_branch.clone()
            };
            test_repo.with_branch(&default_branch)
        }

        fn check_git_config(&self, dir: &Path, name: &str, value: &str) {
            if !has_config_field(dir, name) {
                Command::new("git")
                    .current_dir(dir)
                    .args(["config", "--local", name, value])
                    .output()
                    .unwrap();
            }
        }

        fn check_credentials(&self, dir: &Path) {
            self.check_git_config(dir, "user.name", "Test User");
            self.check_git_config(dir, "user.email", "test@example.com");
        }

        fn clone_to(&self) -> TempDir {
            let temp = tempdir().unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["clone", self.path().to_str().unwrap(), "."])
                .output()
                .unwrap();
            self.check_credentials(temp.path());
            temp
        }

        pub fn with_branch(self, name: &str) -> Self {
            let temp = self.clone_to();
            Command::new("git")
                .current_dir(temp.path())
                .args(["checkout", "-b", name])
                .output()
                .unwrap();

            fs::write(temp.path().join(format!("branch-{}", name)), "").unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["add", "."])
                .output()
                .unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["commit", "-m", &format!("setup branch {}", name)])
                .output()
                .unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["push", "--set-upstream", "origin", name])
                .output()
                .unwrap();
            self
        }

        pub fn with_tag(self, name: &str) -> Self {
            let temp = self.clone_to();
            let branch_name = format!("temp-tag-{}", name);
            Command::new("git")
                .current_dir(temp.path())
                .args(["checkout", "-b", &branch_name])
                .output()
                .unwrap();

            fs::write(temp.path().join(format!("tag-{}", name)), "").unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["add", "."])
                .output()
                .unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["commit", "-m", &format!("setup branch {}", branch_name)])
                .output()
                .unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["push", "--set-upstream", "origin", &branch_name])
                .output()
                .unwrap();

            Command::new("git")
                .current_dir(temp.path())
                .args(["tag", name])
                .output()
                .unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["push", "origin", "--tags"])
                .output()
                .unwrap();

            self
        }

        pub fn change(&self, name: &str, value: &str) {
            let temp = self.clone_to();
            Command::new("git")
                .current_dir(temp.path())
                .args(["checkout", name])
                .output()
                .unwrap();
            Self::test_is_branch(temp.path(), name);
            let file_path = temp.path().join(format!("branch-{}", name));
            fs::write(file_path, value).unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["add", "."])
                .output()
                .unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .args(["commit", "-m", &format!("change branch {}", name)])
                .output()
                .unwrap();
            Command::new("git")
                .current_dir(temp.path())
                .arg("push")
                .output()
                .unwrap();
        }

        pub fn path(&self) -> &Path {
            self.temp.path()
        }

        pub fn create_repo(&self, branch: Option<String>, refetch: bool) -> Repo {
            Repo {
                url: Url::from_str(&self.path().display().to_string()).unwrap(),
                branch,
                refetch,
            }
        }

        pub fn test_is_git(path: &Path) {
            let git_path = path.join(".git");
            assert!(git_path.exists());
            assert!(git_path.is_dir());
        }
        pub fn test_is_not_git(path: &Path) {
            let git_path = path.join(".git");
            assert!(!git_path.exists());
        }

        pub fn test_is_branch(path: &Path, name: &str) {
            let file_name = format!("branch-{}", name);
            let file_path = path.join(&file_name);
            assert!(
                file_path.exists(),
                "The repository converted to {:?} shows no signs of branch {}. The file {} must be present.",
                path,
                name,
                file_name
            );
        }

        pub fn test_is_default_branch(&self, path: &Path) {
            Self::test_is_branch(path, &self.default_branch);
        }

        pub fn test_is_tag(path: &Path, name: &str) {
            let file_name = format!("tag-{}", name);
            let file_path = path.join(&file_name);
            assert!(
                file_path.exists(),
                "The repository converted to {:?} shows no signs of tag {}. The file {} must be present.",
                path,
                name,
                file_name
            );
        }

        pub fn test_is_changed(path: &Path, name: &str, value: &str) {
            Self::test_is_branch(path, name);
            let file_name = format!("branch-{}", name);
            let file_path = path.join(&file_name);

            let content = fs::read(file_path).unwrap();
            let data_str = String::from_utf8(content).unwrap();
            assert_eq!(
                data_str, value,
                "The content of the branch file does not match what was expected."
            )
        }
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use tempfile::{TempDir, tempdir};

    use crate::domains::url::Url;

    use super::test_mocks::*;
    use super::*;

    fn create_row() -> (TempDir, TestRepo, PathBuf) {
        let temp = tempdir().unwrap();
        let path = temp.path().join("w");
        (temp, TestRepo::new(), path)
    }

    #[tokio::test]
    async fn clone_with_default_branch_and_nogit() {
        let git = Git::init().await.unwrap();
        let (_guard, test_repo, path) = create_row();
        let repo = test_repo.create_repo(None, false);

        git.clone(&path, &repo).await.unwrap();

        TestRepo::test_is_not_git(&path);
        test_repo.test_is_default_branch(&path);
    }

    #[tokio::test]
    async fn clone_fails_if_target_dir_exists() {
        let git = Git::init().await.unwrap();
        let temp = tempdir().unwrap();
        let repo = Repo {
            url: Url::from_str("https://example.com/repo.git").unwrap(),
            branch: None,
            refetch: false,
        };

        let result = git.clone(temp.path(), &repo).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, Error::PathAlreadyExists(_)));
    }

    #[tokio::test]
    async fn clone_fails_if_wrong_source() {
        let git = Git::init().await.unwrap();
        let temp = tempdir().unwrap();
        let path = temp.path().join("w");
        let source = temp.path().join("source");
        let repo = Repo {
            url: Url::from_str(source.as_os_str().to_str().unwrap()).unwrap(),
            branch: None,
            refetch: false,
        };

        let result = git.clone(&path, &repo).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, Error::Cmd(_)));
    }

    #[tokio::test]
    async fn clone_with_some_branch() {
        let test_repo = TestRepo::new().with_branch("develop");
        let temp = tempdir().unwrap();
        let path = temp.path().join("w");
        let git = Git::init().await.unwrap();
        let repo = test_repo.create_repo(Some("develop".to_string()), false);

        git.clone(&path, &repo).await.unwrap();
        TestRepo::test_is_branch(&path, "develop");
    }

    #[tokio::test]
    async fn clone_with_some_tag() {
        let test_repo = TestRepo::new().with_tag("v1");
        let temp = tempdir().unwrap();
        let path = temp.path().join("w");
        let git = Git::init().await.unwrap();
        let repo = test_repo.create_repo(Some("v1".to_string()), false);

        git.clone(&path, &repo).await.unwrap();
        TestRepo::test_is_tag(&path, "v1");
    }

    #[tokio::test]
    async fn clone_with_refetch() {
        let test_repo = TestRepo::new();
        let temp = tempdir().unwrap();
        let path = temp.path().join("w");
        let git = Git::init().await.unwrap();
        let repo = test_repo.create_repo(None, true);

        git.clone(&path, &repo).await.unwrap();
        TestRepo::test_is_git(&path);
    }

    #[tokio::test]
    async fn failed_refetch_if_path_not_exists() {
        let git = Git::init().await.unwrap();
        let temp = tempdir().unwrap();
        let path = temp.path().join("inner");

        let result = git.refetch(&path).await;

        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(matches!(error, Error::PathNotExists(_)));
    }

    #[tokio::test]
    async fn failed_refetch_if_missing_git_directory() {
        let git = Git::init().await.unwrap();
        let temp = tempdir().unwrap();

        let result = git.refetch(temp.path()).await;

        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(matches!(error, Error::PathNotExists(_)));
    }

    #[tokio::test]
    async fn refetch_cloned_repository() {
        let test_repo = TestRepo::new().with_branch("some");
        let temp = tempdir().unwrap();
        let path = temp.path().join("w");
        let git = Git::init().await.unwrap();
        let repo = test_repo.create_repo(Some("some".to_string()), true);

        git.clone(&path, &repo).await.unwrap();
        test_repo.change("some", "changed value");

        git.refetch(&path).await.unwrap();
        TestRepo::test_is_changed(&path, "some", "changed value");
    }
}

/*

clone()

clone_succeeds_and_strips_git_when_refetch_false — клонирует и удаляет .git.

clone_succeeds_and_keeps_git_when_refetch_true — клонирует и оставляет .git.

clone_uses_branch_flag_when_branch_set — передаёт --branch <name> при наличии ветки.

clone_propagates_failure_from_git — пробрасывает ошибку, если git clone упал.

clone_propagates_failure_on_git_dir_remove — пробрасывает I/O ошибку при удалении .git.

refetch()

refetch_fails_if_repo_dir_missing — PathNotExists для корневой папки.

refetch_fails_if_git_dir_missing — PathNotExists для <path>/.git.

refetch_runs_fetch_then_pull_in_repo_dir — вызывает fetch и затем pull в нужном каталоге.

refetch_propagates_failure_from_fetch_or_pull — пробрасывает ошибку, если fetch/pull упал.
 */
