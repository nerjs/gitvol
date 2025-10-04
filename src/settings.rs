use clap::Parser;
use std::{io::ErrorKind, os::unix::fs::FileTypeExt, path::PathBuf};
use tokio::fs;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed getting current directory: {0:?}")]
    CurrentDir(ErrorKind),

    #[error("Failed getting socket metadata: {0:?}")]
    SocketMetadata(ErrorKind),

    #[error("Failed to create directory for {0}: {1:?}")]
    CreateDir(String, ErrorKind),

    #[error("Path {:?} must be correct unix socket", .0)]
    NoSocket(PathBuf),

    #[error("Mounting path {:?} is not directory.", .0)]
    NoDirMountingPath(PathBuf),

    #[error("Socket {:?} do not have patent path", .0)]
    MissingSocketParent(PathBuf),
}

#[derive(Debug, clap::Parser)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    socket: Option<PathBuf>,

    #[arg(short, long)]
    mount_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Settings {
    pub socket: PathBuf,
    pub mount_path: PathBuf,
}

impl Settings {
    pub async fn parse() -> Result<Self, Error> {
        let args = Args::parse();
        println!("parsing cli args. {args:?}");

        let current_dir = std::env::current_dir().map_err(|e| Error::CurrentDir(e.kind()))?;

        let mut socket = args
            .socket
            .unwrap_or_else(|| current_dir.join("gitvol_socket/plugin.sock"));
        if !socket.is_absolute() {
            socket = current_dir.join(socket);
            println!("Relative socket path. fixed this. {socket:?}");
        }

        let mut mount_path = args
            .mount_path
            .unwrap_or_else(|| current_dir.join("gitvol_volumes"));
        if !mount_path.is_absolute() {
            mount_path = current_dir.join(mount_path);
            println!("Relative mount path. fixed this. {mount_path:?}");
        }

        if socket.exists() {
            let socket_metadata = fs::metadata(socket.clone())
                .await
                .map_err(|e| Error::SocketMetadata(e.kind()))?
                .file_type();
            if !socket_metadata.is_socket() {
                return Err(Error::NoSocket(socket.clone()));
            }
            println!("Socket already exists.");
        } else {
            let Some(socket_parent) = socket.parent() else {
                return Err(Error::MissingSocketParent(socket.clone()));
            };
            println!("Trying to create socket parent dir. {socket_parent:?}");
            fs::create_dir_all(&socket_parent)
                .await
                .map_err(|e| Error::CreateDir("socket".to_string(), e.kind()))?;
        }

        if mount_path.exists() {
            if !mount_path.is_dir() {
                return Err(Error::NoDirMountingPath(mount_path.clone()));
            }
        } else {
            println!("Trying to create mount dir {mount_path:?}");
            fs::create_dir_all(&mount_path)
                .await
                .map_err(|e| Error::CreateDir("mount".to_string(), e.kind()))?;
        }

        let settings = Self { socket, mount_path };
        println!("paths: {settings:?}");

        Ok(settings)
    }
}
