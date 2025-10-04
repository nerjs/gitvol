mod domains;
mod driver;
mod macros;
mod plugin;
mod services;
mod settings;
mod split_tracing;

use axum::serve;
use tokio::{fs, net::UnixListener};

use crate::{driver::Driver, plugin::Plugin, services::git::Git, settings::Settings};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    split_tracing::init();

    let settings = Settings::parse().await?;

    if settings.socket.exists() {
        fs::remove_file(&settings.socket).await?;
    }

    let git = Git::init().await?;
    let plugin = Plugin::new(&settings.mount_path, git).into_router();
    let listener = UnixListener::bind(&settings.socket)?;
    println!("listening on {:?}", listener.local_addr().unwrap());

    serve(listener, plugin).await?;

    Ok(())
}
