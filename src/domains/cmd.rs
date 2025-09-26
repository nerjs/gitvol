use std::{
    ffi::OsStr,
    path::Path,
    process::{ExitStatus, Output},
    string::FromUtf8Error,
};

use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum KindError {
    #[error(transparent)]
    Io(std::io::Error),

    #[error("Error converting to utf-8:  {0}")]
    FromUtf8(FromUtf8Error),

    #[error("exited with status {status}: {stderr}")]
    NonZero { status: ExitStatus, stderr: String },
}

fn join_cmd(command: &str, subcommand: &Option<String>) -> String {
    let sub_str = subcommand
        .clone()
        .map_or(String::new(), |s| format!(" {s}"));
    format!("{command}{sub_str}")
}

#[derive(Debug, thiserror::Error)]
#[error("CMD: {} : {}", join_cmd(command, subcommand), .kind.to_string())]
pub struct Error {
    command: String,
    subcommand: Option<String>,
    kind: KindError,
}

pub struct Cmd(String);

impl Cmd {
    pub fn new<T: Into<String>>(command: T) -> Self {
        Self(command.into())
    }

    pub fn arg<S: AsRef<OsStr>>(&self, arg: S) -> CmdRunner {
        let mut runner = Command::new(self.0.clone());
        runner.arg(arg);

        CmdRunner {
            runner,
            command: self.0.clone(),
            subcommand: None,
        }
    }

    pub fn command<T: Into<String>>(&self, subcommand: T) -> CmdRunner {
        let subcommand: String = subcommand.into();
        let runner = self.arg(subcommand.clone());
        CmdRunner {
            subcommand: Some(subcommand),
            ..runner
        }
    }
}

pub struct CmdRunner {
    runner: Command,
    command: String,
    subcommand: Option<String>,
}

impl CmdRunner {
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.runner.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.runner.args(args);
        self
    }

    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.runner.current_dir(dir);
        self
    }

    fn error(&self, kind: KindError) -> Error {
        Error {
            command: self.command.clone(),
            subcommand: self.subcommand.clone(),
            kind,
        }
    }

    pub async fn exec(&mut self) -> Result<String, Error> {
        let Output {
            status,
            stderr,
            stdout,
        } = self
            .runner
            .output()
            .await
            .map_err(|e| self.error(KindError::Io(e)))?;

        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).into_owned();
            return Err(self.error(KindError::NonZero { status, stderr }));
        }

        let stdout = String::from_utf8(stdout)
            .map_err(|e| self.error(KindError::FromUtf8(e)))?
            .trim()
            .to_string();
        Ok(stdout)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn exec_returns_stdout() {
        let result = Cmd::new("echo").arg("qwerty").exec().await.unwrap();
        assert_eq!(result, "qwerty");
    }

    #[tokio::test]
    async fn exec_subcommand() {
        let result = Cmd::new("cargo")
            .command("help")
            .args(["run"])
            .exec()
            .await
            .unwrap();
        assert!(result.contains("cargo run"));
    }

    #[tokio::test]
    async fn current_dir() {
        let result = Cmd::new("pwd")
            .arg("-P")
            .current_dir(std::env::current_dir().unwrap())
            .exec()
            .await
            .unwrap();
        assert_eq!(result, std::env::current_dir().unwrap().to_string_lossy());
    }

    #[tokio::test]
    async fn trimmed_output() {
        let result = Cmd::new("echo").arg("  qwerty  ").exec().await.unwrap();
        assert_eq!(result, "qwerty");
    }

    #[tokio::test]
    async fn failed_exec_io() {
        let result = Cmd::new("non_existing_command_123")
            .command("help-subcommand")
            .arg("-v")
            .exec()
            .await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error.kind, KindError::Io(_)));
        assert!(
            error
                .to_string()
                .contains("CMD: non_existing_command_123 help-subcommand")
        )
    }

    #[tokio::test]
    async fn failed_exec_non_zero() {
        let result = Cmd::new("ls").arg("some-non-existent-file").exec().await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(&error.kind, KindError::NonZero { .. }));
    }
}
