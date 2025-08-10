use anyhow::{Context, Result};
use git_url_parse::{GitUrl, Scheme};
use log::{debug, kv::Value, trace};
use std::{ffi::OsStr, path::PathBuf, process::Output};
use tokio::{fs, process::Command};

use crate::state::Repo;

pub fn parse_url(input: &str) -> Result<()> {
    let str_url = input.trim();
    if str_url.is_empty() {
        anyhow::bail!("Url can not be empty");
    }

    let GitUrl { scheme, .. } = GitUrl::parse(input).context("Failed normalize url")?;

    if vec![Scheme::Unspecified, Scheme::Ftp, Scheme::Ftps].contains(&scheme) {
        anyhow::bail!("Unsupported url scheme {:?}", scheme);
    }

    if cfg!(not(test)) {
        if scheme == Scheme::File {
            anyhow::bail!("Unsupported url scheme {:?}", scheme);
        }
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
        .with_context(|| format!("Failed to execute command '{} {:?}'", cmd, args))?;

    let stderr = String::from_utf8(stderr)
        .context("Failed to parse stderr as UTF-8")?
        .trim()
        .to_string();
    let stdout = String::from_utf8(stdout)
        .context("Failed to parse stdout as UTF-8")?
        .trim()
        .to_string();

    if !status.success() {
        if stderr.is_empty() {
            anyhow::bail!(
                "Command '{} {:?}' exited with non-zero status: {}",
                cmd,
                args,
                status
            )
        } else {
            anyhow::bail!("Command '{} {:?}' failed: {}", cmd, args, stderr)
        }
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
    let git_path = run_command(&"/".into(), "which", vec!["git"])
        .await
        .context("Failed to locate git executable")?;
    debug!(git_path;  "Located git executable.");
    let git_version = run_git_command(&"/".into(), vec!["--version"])
        .await
        .context("Failed to retrieve git version")?;
    debug!(version = format!("'{}'", git_version); "Verified git version.");

    Ok(())
}

pub async fn clone(path: &PathBuf, repo: &Repo) -> Result<()> {
    debug!(url = repo.url; "trying clonning repository.");
    anyhow::ensure!(!path.exists(), "path '{:?}' already exists", path);

    let mut raw_cmd = vec!["clone", "--depth=1"];
    if let Some(branch) = &repo.branch {
        raw_cmd.push("--branch");
        raw_cmd.push(branch);
    }
    raw_cmd.push(&repo.url);
    raw_cmd.push(path.to_str().context("Failed convert path to string")?);

    let output = run_git_command(&"/".into(), raw_cmd)
        .await
        .context("Failed clone repository")?;

    trace!("git output: {}", output);

    if !repo.refetch {
        fs::remove_dir_all(path.join(".git"))
            .await
            .context("Failed to remove .git directory. refetch is false.")?;
    }

    debug!(url = repo.url; "Succefully clonning.");

    Ok(())
}

pub async fn refetch(path: &PathBuf) -> Result<()> {
    debug!(path = Value::from_debug(path); "trying refetch repository");
    anyhow::ensure!(path.exists(), "path {:?} not exists", path);
    anyhow::ensure!(
        path.join(".git").exists(),
        "git directory {:?} not exists",
        path
    );

    _ = run_git_command(path, vec!["fetch"])
        .await
        .context("Fetching repo")?;
    _ = run_git_command(path, vec!["pull"])
        .await
        .context("Pulling repo")?;

    Ok(())
}

#[cfg(test)]
pub mod test {
    use once_cell::sync::Lazy;
    use std::str::FromStr;

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
            anyhow::ensure!(file.exists() && file.is_file(), "temp file not exists");

            let content = fs::read(file).await.context("read temp file")?;
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
            .await
            .context("Clone test.git")?;

            Ok(())
        }

        async fn credentials(path: &PathBuf) -> Result<()> {
            run_git_command(
                &path,
                vec!["config", "--local", "user.email", "test@example.com"],
            )
            .await
            .context("set git user email")?;
            run_git_command(&path, vec!["config", "--local", "user.name", "Test User"])
                .await
                .context("set git user name")?;

            Ok(())
        }

        async fn add_and_commit(path: &PathBuf, message: &str) -> Result<()> {
            run_git_command(&path, vec!["add", "."])
                .await
                .context("add changes")?;
            run_git_command(&path, vec!["commit", "-m", message])
                .await
                .context("commit changes")?;
            Ok(())
        }

        async fn setup_temp_directory(&self) -> Result<(PathBuf, TempDir)> {
            let temp = tempdir().context("create temp dir for setup temp branch")?;
            let path = temp.path().join("worker");
            self.clone_to(&path).await.context("clone to temp worker")?;
            Self::credentials(&path)
                .await
                .context("git credentials for temp")?;

            Ok((path, temp))
        }

        pub async fn setup_temp_branch(&self, branch_name: &str) -> Result<()> {
            let (path, _temp) = self.setup_temp_directory().await.context("Setup temp")?;
            run_git_command(&path, vec!["checkout", "-b", branch_name])
                .await
                .context(format!("checkout temp before setup {}", branch_name))?;

            fs::write(path.join(Self::n_file(branch_name)), "setup")
                .await
                .context("write setup info into temp file")?;
            Self::add_and_commit(&path, "temp file")
                .await
                .context("setup temp file")?;
            run_git_command(&path, vec!["push", "--set-upstream", "origin", branch_name])
                .await
                .context("push temp file")?;

            Ok(())
        }

        pub async fn change_temp_branch(&self, branch_name: &str) -> Result<()> {
            let (path, _temp) = self.setup_temp_directory().await.context("Setup temp")?;

            run_git_command(&path, vec!["fetch"])
                .await
                .context("fetch changes before change temp")?;

            run_git_command(&path, vec!["checkout", branch_name])
                .await
                .context(format!("checkout temp before change {}", branch_name))?;
            fs::write(path.join(Self::n_file(branch_name)), "changed")
                .await
                .context("change setup info into temp file")?;
            Self::add_and_commit(&path, "temp file")
                .await
                .context("change temp file")?;
            run_git_command(&path, vec!["push", "--set-upstream", "origin", branch_name])
                .await
                .context("push changed temp file")?;
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
                .context("create test.git dir")?;
            let worker = tmp.path().join("worker");

            run_git_command(
                &source.path,
                vec!["init", "--bare", "--initial-branch", &source.master],
            )
            .await
            .context("initialize test.git")?;

            source
                .clone_to(&worker)
                .await
                .context("clone to setup worker")?;

            Self::credentials(&worker)
                .await
                .context("setup git credentials")?;

            fs::write(worker.join(source.tag_file()), source.tag.clone())
                .await
                .context("create tag file")?;
            Self::add_and_commit(&worker, "creating tag file")
                .await
                .context("set tag file")?;
            run_git_command(
                &worker,
                vec!["push", "--set-upstream", "origin", &source.master],
            )
            .await
            .context("push tag file")?;
            run_git_command(&worker, vec!["tag", &source.tag])
                .await
                .context("create tag")?;
            run_git_command(&worker, vec!["push", "origin", "--tags"])
                .await
                .context("push tags")?;

            fs::remove_file(worker.join(source.tag_file()))
                .await
                .context("remove tag file")?;
            fs::write(worker.join(source.master_file()), source.master.clone())
                .await
                .context("create master file")?;
            Self::add_and_commit(&worker, "creating main file")
                .await
                .context("set main file")?;
            run_git_command(
                &worker,
                vec!["push", "--set-upstream", "origin", &source.master],
            )
            .await
            .context("push master file")?;

            run_git_command(&worker, vec!["checkout", "-b", &source.develop])
                .await
                .context("checkout develop")?;
            fs::remove_file(worker.join(source.master_file()))
                .await
                .context("remove master file")?;
            fs::write(worker.join(source.develop_file()), source.develop.clone())
                .await
                .context("create develop file")?;
            Self::add_and_commit(&worker, "creating develop file")
                .await
                .context("set develop file")?;
            run_git_command(
                &worker,
                vec!["push", "--set-upstream", "origin", &source.develop],
            )
            .await
            .context("push develop file")?;

            fs::remove_dir_all(&worker)
                .await
                .context("remove worker directory")?;

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
