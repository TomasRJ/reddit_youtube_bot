use clap::{Parser, Subcommand, command};
use thiserror::Error;

use crate::server::{ApiError, serve};

#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start the API at default port 3000, can be changed with --port.
    Start {
        #[arg(long, default_value = "3000")]
        port: u16,
    },
}

impl Cli {
    pub fn initialize() -> Self {
        Cli::parse()
    }

    pub async fn handle(self) -> Result<(), CommandError> {
        match self.command {
            Commands::Start { port } => {
                if !(1024..=65535).contains(&port) {
                    return Err(CommandError::InvalidPort(port));
                }
                serve(port).await?;
            }
        }
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Invalid port number {0}")]
    InvalidPort(u16),
    #[error("API error: {0}")]
    ApiError(#[from] ApiError),
}
