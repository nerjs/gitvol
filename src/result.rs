use std::{io::ErrorKind, path::PathBuf, string::FromUtf8Error};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Path {:?} must be correct unix socket", .0)]
    NoSocket(PathBuf),

    #[error("Mounting path {:?} is not directory.", .0)]
    NoDirMountingPath(PathBuf),

    #[error("Socket {:?} do not have patent path", .0)]
    MissingSocketParent(PathBuf),

    #[error("Volume name can not be empty")]
    EmptyVolumeName,

    #[error("Volume named '{name}' already exists")]
    VolumeAlreadyExists { name: String },

    #[error("Non existen volume named {:?}", .0)]
    VolumeNonExists(String),

    #[error("repository URL  parsing: {}", .0.to_string())]
    Url(#[from] crate::domains::url::Error),

    #[error(transparent)]
    Repo(#[from] crate::domains::repo::Error),

    #[error("Failed to execute command '{cmd} {args:?}' with reason: {reason:?}")]
    Cmd {
        reason: String,
        cmd: String,
        args: Vec<String>,
    },

    #[error("Error converting to utf-8:  {:?}", .0)]
    FromUtf8(#[from] FromUtf8Error),

    #[error("Repository local path '{path}' already exists")]
    PathAlreadyExists { path: PathBuf },
    #[error("Repository local path '{path}' not exists")]
    PathNotExists { path: PathBuf },

    #[error("Failed to remove {reason}. Error kind: {kind:?}")]
    RemoveDirectory { kind: ErrorKind, reason: String },

    #[error("IO error [{reason:?}] : {kind:?}")]
    Io { kind: ErrorKind, reason: String },

    #[cfg(test)]
    #[error("Temporary test file {file:?} not found or not a file")]
    TestTmpNotExists { file: PathBuf },
}

pub trait ErrorIoExt<T> {
    fn map_io_error<S: std::fmt::Debug + ?Sized>(self, reason: &S) -> Result<T>;
}

impl<T> ErrorIoExt<T> for std::io::Result<T> {
    fn map_io_error<S: std::fmt::Debug + ?Sized>(self, reason: &S) -> Result<T> {
        let reason = format!("{reason:?}");
        self.map_err(|e| Error::Io {
            kind: e.kind(),
            reason,
        })
    }
}

pub type Result<T> = std::result::Result<T, Error>;
