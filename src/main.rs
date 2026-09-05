use std::net::SocketAddr;

use axum::extract::connect_info::IntoMakeServiceWithConnectInfo;
use clap::{Parser, Subcommand};
use karzoun_ironroute::{config::Config, proxy::{AppState, describe_startup}};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "ironroute", version, about = "Adaptive Rust edge gateway and resilience engine")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value = "ironroute.toml")]
        config: String,
    },
    Check {
        #[arg(long, default_value = "ironroute.toml")]
        config: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ironroute=info,karzoun_ironroute=info")))
        .init();

    match Cli::parse().command {
        Command::Check { config } => {
            Config::from_path(config)?;
            println!("configuration valid");
        }
        Command::Serve { config } => {
            let config = Config::from_path(config)?;
            describe_startup(&config);
            let listen = config.socket_addr()?;
            let app = AppState::new(config)?.router();
            let listener = tokio::net::TcpListener::bind(listen).await?;
            tracing::info!(%listen, "IronRoute listening");
            let service: IntoMakeServiceWithConnectInfo<_, SocketAddr> = app.into_make_service_with_connect_info();
            axum::serve(listener, service)
                .with_graceful_shutdown(shutdown_signal())
                .await?;
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install shutdown signal");
    }
}
