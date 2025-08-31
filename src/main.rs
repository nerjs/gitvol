mod app;
mod git;
mod macros;
mod result;
mod state;

use std::{fmt::Debug, os::unix::fs::FileTypeExt, path::PathBuf};

use axum::serve;
use clap::Parser;
use log::{debug, info, kv};
use tokio::{fs, net::UnixListener};

use crate::{
    result::{Error, ErrorIoExt, Result},
    state::GitvolState,
};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp(None)
        .target(env_logger::Target::Stdout)
        .init();

    let settings = Settings::parse().await?;

    git::ensure_git_exists().await?;

    if settings.socket.exists() {
        fs::remove_file(&settings.socket)
            .await
            .map_io_error(&settings.socket)?;
    }

    let state = GitvolState::new(settings.mount_path);
    let app = app::create(state);
    let listener = UnixListener::bind(&settings.socket).map_io_error(&settings.socket)?;
    info!("listening on {:?}", listener.local_addr().unwrap());

    serve(listener, app.into_make_service())
        .await
        .map_io_error(&format!("serve: {:?}", settings.socket))?;

    Ok(())
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
struct Settings {
    socket: PathBuf,
    mount_path: PathBuf,
}

impl Settings {
    async fn parse() -> Result<Self> {
        let args = Args::parse();
        debug!(args = kv::Value::from_debug(&args); "parsing cli args.");

        let current_dir = std::env::current_dir().map_io_error("current dir")?;

        let mut socket = args
            .socket
            .unwrap_or_else(|| current_dir.join("gitvol_socket/plugin.sock"));
        if !socket.is_absolute() {
            socket = current_dir.join(socket);
            debug!(socket = kv::Value::from_debug(&socket); "Relative socket path. fixed this.");
        }

        let mut mount_path = args
            .mount_path
            .unwrap_or_else(|| current_dir.join("gitvol_volumes"));
        if !mount_path.is_absolute() {
            mount_path = current_dir.join(mount_path);
            debug!(mount_path = kv::Value::from_debug(&mount_path); "Relative mount path. fixed this.");
        }

        if socket.exists() {
            let socket_metadata = fs::metadata(socket.clone())
                .await
                .map_io_error(&socket)?
                .file_type();
            if !socket_metadata.is_socket() {
                return Err(Error::NoSocket(socket.clone()));
            }
            debug!("Socket already exists.");
        } else {
            let Some(socket_parent) = socket.parent() else {
                return Err(Error::MissingSocketParent(socket.clone()));
            };
            debug!(socket_parent = kv::Value::from_debug(&socket_parent); "Trying to create socket parent dir.");
            fs::create_dir_all(&socket_parent)
                .await
                .map_io_error(&socket_parent)?;
        }

        if mount_path.exists() {
            return Err(Error::NoDirMountingPath(mount_path.clone()));
        } else {
            debug!(mount_path = kv::Value::from_debug(&mount_path); "Trying to create mount dir.");
            fs::create_dir_all(&mount_path)
                .await
                .map_io_error(&mount_path)?;
        }

        Ok(Self { socket, mount_path })
    }
}
