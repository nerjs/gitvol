#![allow(warnings)]
mod app;
mod git;
mod macros;
mod state;

use std::{fmt::Debug, os::unix::fs::FileTypeExt, path::PathBuf};

use anyhow::{Context, Result};
use axum::serve;
use clap::Parser;
use log::{
    debug, info,
    kv::{self, ToValue},
};
use tokio::{fs, net::UnixListener};

use crate::state::GitvolState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp(None)
        .target(env_logger::Target::Stdout)
        .init();

    let settings = Settings::parse().await.context("parsing arguments")?;

    git::ensure_git_exists()
        .await
        .context("Failed check git exists")?;

    if settings.socket.exists() {
        fs::remove_file(&settings.socket)
            .await
            .context("remove old socket")?;
    }

    let state = GitvolState::new(settings.mount_path);
    let app = app::create(state);
    let listener = UnixListener::bind(settings.socket)?;
    info!("listening on {:?}", listener.local_addr().unwrap());

    serve(listener, app.into_make_service()).await?;

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

        let current_dir =
            std::env::current_dir().context("Failed getting current dir for start settings.")?;

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
            let socket_metadata = fs::metadata(socket.clone()).await?.file_type();
            if !socket_metadata.is_socket() {
                anyhow::bail!("The path to the socket {:?} is not a socket.", socket);
            }
            debug!("Socket already exists.");
        } else {
            let Some(socket_parent) = socket.parent() else {
                anyhow::bail!("Incorrect socket path {socket:?}.");
            };
            debug!(socket_parent = kv::Value::from_debug(&socket_parent); "Trying to create socket parent dir.");
            fs::create_dir_all(socket_parent)
                .await
                .context("create socket parent directory.")?;
        }

        if mount_path.exists() {
            anyhow::ensure!(mount_path.is_dir(), "Mounting path is not directory.");
        } else {
            debug!(mount_path = kv::Value::from_debug(&mount_path); "Trying to create mount dir.");
            fs::create_dir_all(&mount_path)
                .await
                .context("create mounting directory.")?;
        }

        Ok(Self { socket, mount_path })
    }
}
