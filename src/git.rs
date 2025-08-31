use crate::result::{Error, Result};
use git_url_parse::{GitUrl, Scheme};
use log::{debug, kv::Value, trace};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Output,
};
use tokio::{fs, process::Command};

use crate::state::Repo;

impl Error {
    pub fn cmd<S, R>(command: &str, args: &[S], reason: R) -> Error
    where
        S: std::fmt::Debug + Clone,
        R: std::fmt::Debug,
    {
        Error::Cmd {
            reason: format!("{reason:?}"),
            cmd: command.to_string(),
            args: args.iter().map(|s| format!("{s:?}")).collect(),
        }
    }
}

pub fn parse_url(input: &str) -> Result<()> {
    let str_url = input.trim();
    if str_url.is_empty() {
        return Err(Error::EmptyUrl);
    }

    let GitUrl { scheme, .. } = GitUrl::parse(input)?;

    if [Scheme::Unspecified, Scheme::Ftp, Scheme::Ftps].contains(&scheme) {
        return Err(Error::UnsupportedUrlScheme {
            scheme,
            url: str_url.into(),
        });
    }

    if cfg!(not(test)) && scheme == Scheme::File {
        return Err(Error::UnsupportedUrlScheme {
            scheme,
            url: str_url.into(),
        });
    }

    Ok(())
}

async fn run_command<S>(current_dir: &PathBuf, cmd: &str, args: Vec<S>) -> Result<String>
where
    S: AsRef<OsStr> + std::fmt::Debug + Clone,
{
    let Output {
        status,
        stderr,
        stdout,
    } = Command::new(cmd)
        .args(args.clone())
        .current_dir(current_dir)
        .output()
        .await
        .map_err(|e| Error::cmd(cmd, &args, e.kind()))?;

    let stderr = String::from_utf8(stderr)?;
    let stdout = String::from_utf8(stdout)?.trim().to_string();

    if !status.success() {
        let error = if stderr.is_empty() {
            Error::cmd(cmd, &args, format!("exited with non-zero status: {status}"))
        } else {
            Error::cmd(cmd, &args, stderr)
        };
        return Err(error);
    }

    Ok(stdout)
}
async fn run_git_command<S>(current_dir: &PathBuf, args: Vec<S>) -> Result<String>
where
    S: AsRef<OsStr> + std::fmt::Debug + Clone,
{
    run_command(current_dir, "git", args).await
}

pub async fn ensure_git_exists() -> Result<()> {
    let git_path = run_command(&"/".into(), "which", vec!["git"]).await?;
    debug!(git_path;  "Located git executable.");
    let git_version = run_git_command(&"/".into(), vec!["--version"]).await?;
    debug!(version = format!("'{}'", git_version); "Verified git version.");

    Ok(())
}

pub async fn clone(path: &Path, repo: &Repo) -> Result<()> {
    debug!(url = repo.url; "trying clonning repository.");

    if path.exists() {
        return Err(Error::PathAlreadyExists {
            path: path.to_path_buf(),
        });
    }

    let mut raw_cmd = vec!["clone", "--depth=1"];
    if let Some(branch) = &repo.branch {
        raw_cmd.push("--branch");
        raw_cmd.push(branch);
    }
    raw_cmd.push(&repo.url);
    raw_cmd.push(path.to_str().unwrap_or_default());

    let output = run_git_command(&"/".into(), raw_cmd).await?;

    trace!("git output: {}", output);

    if !repo.refetch {
        fs::remove_dir_all(path.join(".git"))
            .await
            .map_err(|e| Error::RemoveDirectory {
                kind: e.kind(),
                reason: ".git directory".into(),
            })?;
    }

    debug!(url = repo.url; "Succefully clonning.");

    Ok(())
}

pub async fn refetch(path: &PathBuf) -> Result<()> {
    debug!(path = Value::from_debug(path); "trying refetch repository");

    if !path.exists() {
        return Err(Error::PathNotExists { path: path.clone() });
    }

    let git_path = path.join(".git");
    if !git_path.exists() {
        return Err(Error::PathNotExists {
            path: git_path.clone(),
        });
    }

    _ = run_git_command(path, vec!["fetch"]).await?;
    _ = run_git_command(path, vec!["pull"]).await?;

    Ok(())
}

#[cfg(test)]
pub mod test {
    use once_cell::sync::Lazy;
    use std::str::FromStr;

    use crate::result::ErrorIoExt;

    use super::*;
    use tempfile::{Builder, TempDir, tempdir};
    use tokio::sync::Mutex;
    use uuid::Uuid;

    #[derive(Debug, Clone)]
    pub struct TestRepo {
        pub path: PathBuf,
        pub file: String,
        pub master: String,
        pub develop: String,
        pub tag: String,
    }

    static TEST_REPOSITORY: Lazy<Mutex<Option<TestRepo>>> = Lazy::new(|| Mutex::new(None));

    impl TestRepo {
        fn new(path: PathBuf) -> Self {
            let file = format!("file:///{}", path.to_string_lossy().trim_start_matches("/"));
            Self {
                path,
                file,
                master: "master".into(),
                develop: "develop".into(),
                tag: "v1".into(),
            }
        }

        fn n_file(name: &str) -> String {
            format!("{}_file", name)
        }

        fn master_file(&self) -> String {
            Self::n_file(&self.master)
        }
        fn develop_file(&self) -> String {
            Self::n_file(&self.develop)
        }
        fn tag_file(&self) -> String {
            Self::n_file(&self.tag)
        }

        pub fn is_n(path: &PathBuf, name: &str) -> bool {
            let file = path.join(Self::n_file(name));
            file.exists() && file.is_file()
        }

        pub async fn is_temp_changed(path: &PathBuf, branch_name: &str) -> Result<bool> {
            let file = path.join(Self::n_file(branch_name));

            if !file.exists() || !file.is_file() {
                return Err(Error::TestTmpNotExists { file });
            }

            let content = fs::read(&file).await.map_io_error(&file)?;
            let str_content = String::from_utf8(content)?;
            Ok(str_content == "changed")
        }

        pub fn is_master(&self, path: &PathBuf) -> bool {
            Self::is_n(path, &self.master)
        }
        pub fn is_develop(&self, path: &PathBuf) -> bool {
            Self::is_n(path, &self.develop)
        }
        pub fn is_tag(&self, path: &PathBuf) -> bool {
            Self::is_n(path, &self.tag)
        }

        async fn clone_to(&self, path: &PathBuf) -> Result<()> {
            run_git_command(
                &PathBuf::from_str("/").unwrap(),
                vec!["clone", &self.file, path.to_str().unwrap()],
            )
            .await?;

            Ok(())
        }

        async fn credentials(path: &PathBuf) -> Result<()> {
            run_git_command(
                &path,
                vec!["config", "--local", "user.email", "test@example.com"],
            )
            .await?;
            run_git_command(&path, vec!["config", "--local", "user.name", "Test User"]).await?;

            Ok(())
        }

        async fn add_and_commit(path: &PathBuf, message: &str) -> Result<()> {
            run_git_command(&path, vec!["add", "."]).await?;
            run_git_command(&path, vec!["commit", "-m", message]).await?;
            Ok(())
        }

        async fn setup_temp_directory(&self) -> Result<(PathBuf, TempDir)> {
            let temp = tempdir().unwrap();
            let path = temp.path().join("worker");
            self.clone_to(&path).await?;
            Self::credentials(&path).await?;

            Ok((path, temp))
        }

        pub async fn setup_temp_branch(&self, branch_name: &str) -> Result<()> {
            let (path, _temp) = self.setup_temp_directory().await?;
            run_git_command(&path, vec!["checkout", "-b", branch_name]).await?;

            let branch_path = path.join(Self::n_file(branch_name));
            fs::write(&branch_path, "setup")
                .await
                .map_io_error(&branch_path)?;
            Self::add_and_commit(&path, "temp file").await?;
            run_git_command(&path, vec!["push", "--set-upstream", "origin", branch_name]).await?;

            Ok(())
        }

        pub async fn change_temp_branch(&self, branch_name: &str) -> Result<()> {
            let (path, _temp) = self.setup_temp_directory().await?;

            run_git_command(&path, vec!["fetch"]).await?;

            run_git_command(&path, vec!["checkout", branch_name]).await?;

            let branch_path = path.join(Self::n_file(branch_name));
            fs::write(&branch_path, "changed")
                .await
                .map_io_error(&branch_path)?;
            Self::add_and_commit(&path, "temp file").await?;
            run_git_command(&path, vec!["push", "--set-upstream", "origin", branch_name]).await?;
            Ok(())
        }

        pub async fn get_or_create() -> Result<Self> {
            let mut tmp_repo = TEST_REPOSITORY.lock().await;
            if let Some(repository) = &mut *tmp_repo {
                return Ok(repository.clone());
            }
            let tmp = Builder::new()
                .prefix("gitvol-test-repository-")
                .disable_cleanup(true)
                .tempdir()
                .unwrap();
            let source = TestRepo::new(tmp.path().join("test.git"));
            *tmp_repo = Some(source.clone());

            fs::create_dir(&source.path)
                .await
                .map_io_error(&source.path)?;
            let worker = tmp.path().join("worker");

            run_git_command(
                &source.path,
                vec!["init", "--bare", "--initial-branch", &source.master],
            )
            .await?;

            source.clone_to(&worker).await?;

            Self::credentials(&worker).await?;

            let worker_tag = worker.join(source.tag_file());
            let worker_master = worker.join(source.master_file());
            let worker_develop = worker.join(source.develop_file());

            fs::write(&worker_tag, source.tag.clone())
                .await
                .map_io_error(&worker_tag)?;
            Self::add_and_commit(&worker, "creating tag file").await?;
            run_git_command(
                &worker,
                vec!["push", "--set-upstream", "origin", &source.master],
            )
            .await?;
            run_git_command(&worker, vec!["tag", &source.tag]).await?;
            run_git_command(&worker, vec!["push", "origin", "--tags"]).await?;

            fs::remove_file(&worker_tag)
                .await
                .map_io_error(&worker_tag)?;
            fs::write(&worker_master, source.master.clone())
                .await
                .map_io_error(&worker_master)?;
            Self::add_and_commit(&worker, "creating main file").await?;
            run_git_command(
                &worker,
                vec!["push", "--set-upstream", "origin", &source.master],
            )
            .await?;

            run_git_command(&worker, vec!["checkout", "-b", &source.develop]).await?;
            fs::remove_file(&worker_master)
                .await
                .map_io_error(&worker_master)?;
            fs::write(&worker_develop, source.develop.clone())
                .await
                .map_io_error(&worker_develop)?;
            Self::add_and_commit(&worker, "creating develop file").await?;
            run_git_command(
                &worker,
                vec!["push", "--set-upstream", "origin", &source.develop],
            )
            .await?;

            fs::remove_dir_all(&worker).await.map_io_error(&worker)?;

            Ok(source)
        }
    }

    pub fn is_git_dir(path: &PathBuf) -> bool {
        let git_dir = path.join(".git");
        git_dir.exists() && git_dir.is_dir()
    }

    #[test]
    fn check_valid_urls() {
        let valid_urls = vec![
            "ssh://host/path-to-git-repo",
            "ssh://user@host/path-to-git-repo",
            "ssh://host:123/path-to-git-repo",
            "ssh://user@host:123/path-to-git-repo",
            "git://host/path-to-git-repo",
            "git://host:123/path-to-git-repo",
            "http://host/path-to-git-repo",
            "https://host/path-to-git-repo",
            "http://host:123/path-to-git-repo",
            "https://host:123/path-to-git-repo",
            "ssh://host/~user/path-to-git-repo",
            "ssh://user@host/~user/path-to-git-repo",
            "ssh://host:123/~user/path-to-git-repo",
            "ssh://user@host:123/~user/path-to-git-repo",
            "git://host/~user/path-to-git-repo",
            "git://host:123/~user/path-to-git-repo",
            "host:~user/path-to-git-repo",
            "user@host:~user/path-to-git-repo",
        ];

        let not_valid_urls = vec![
            "ftp://host/path-to-git-repo",
            "ftps://host/path-to-git-repo",
            "ftp://host:123/path-to-git-repo",
            "ftps://host:123/path-to-git-repo",
            "",
        ];

        for url in valid_urls {
            assert!(parse_url(url).is_ok(), "Failed parsing url '{}'", url);
        }
        for url in not_valid_urls {
            assert!(parse_url(url).is_err(), "Failed parsing url '{}'", url);
        }
    }

    #[tokio::test]
    async fn check_run_commands() {
        let tmp = tempdir().unwrap();
        let output = run_command::<String>(&tmp.path().to_path_buf(), "pwd", vec![])
            .await
            .unwrap();

        assert_eq!(tmp.path().to_path_buf(), PathBuf::from(output));

        let _ = run_command(&tmp.path().to_path_buf(), "mkdir", vec!["some-path"])
            .await
            .unwrap();
        assert!(tmp.path().join("some-path").exists());
    }

    #[tokio::test]
    async fn failed_run_commands() {
        let output = run_command(
            &PathBuf::from_str("/").unwrap(),
            "which",
            vec!["some-app_123"],
        )
        .await;
        assert!(output.is_err());
    }

    #[tokio::test]
    async fn check_git_commans() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("repository");

        run_git_command(
            &test_repo.path,
            vec!["clone", &test_repo.file, path.to_str().unwrap()],
        )
        .await
        .unwrap();
        assert!(is_git_dir(&path));
        assert!(test_repo.is_master(&path));

        run_git_command(&path, vec!["checkout", &test_repo.develop])
            .await
            .unwrap();

        assert!(test_repo.is_develop(&path));
    }

    #[tokio::test]
    async fn clone_with_default_branch_and_nogit() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("repository");

        let repo = Repo {
            url: test_repo.file.clone(),
            branch: None,
            refetch: false,
        };

        clone(&path, &repo).await.unwrap();

        assert!(path.exists());
        assert!(!is_git_dir(&path));
        assert!(test_repo.is_master(&path))
    }

    #[tokio::test]
    async fn clone_with_develop_branch_and_nogit() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("repository");

        let repo = Repo {
            url: test_repo.file.clone(),
            branch: Some(test_repo.develop.clone()),
            refetch: false,
        };

        clone(&path, &repo).await.unwrap();

        assert!(path.exists());
        assert!(!is_git_dir(&path));
        assert!(test_repo.is_develop(&path))
    }

    #[tokio::test]
    async fn clone_with_tag_and_nogit() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("repository");

        let repo = Repo {
            url: test_repo.file.clone(),
            branch: Some(test_repo.tag.clone()),
            refetch: false,
        };

        clone(&path, &repo).await.unwrap();

        assert!(path.exists());
        assert!(!is_git_dir(&path));
        assert!(test_repo.is_tag(&path))
    }

    #[tokio::test]
    async fn clone_with_refetch() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("repository");
        let branch_name = format!("branch_{}", Uuid::new_v4().to_string());

        test_repo.setup_temp_branch(&branch_name).await.unwrap();

        let repo = Repo {
            url: test_repo.file.clone(),
            branch: Some(branch_name.clone()),
            refetch: true,
        };

        clone(&path, &repo).await.unwrap();

        assert!(path.exists());
        assert!(is_git_dir(&path));
        assert!(TestRepo::is_n(&path, &branch_name));
        assert!(
            !TestRepo::is_temp_changed(&path, &branch_name)
                .await
                .unwrap()
        );

        test_repo.change_temp_branch(&branch_name).await.unwrap();
        refetch(&path).await.unwrap();

        assert!(
            TestRepo::is_temp_changed(&path, &branch_name)
                .await
                .unwrap()
        );
    }
}
