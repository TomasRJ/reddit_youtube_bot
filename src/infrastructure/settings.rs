use std::env::{self, VarError};

use thiserror::Error;

use crate::server::RedditCredentials;

#[derive(Debug, Clone)]
pub struct Settings {
    pub database_url: String,
    pub reddit_credentials: RedditCredentials,
    pub base_url: String,
}

impl Settings {
    pub fn new() -> Result<Self, SettingsError> {
        dotenvy::dotenv()?;

        Ok(Self {
            database_url: env::var("DATABASE_URL")?,
            reddit_credentials: RedditCredentials {
                client_id: env::var("CLIENT_ID")?,
                client_secret: env::var("CLIENT_SECRET")?,
            },
            base_url: env::var("BASE_URL")?,
        })
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("Environment file error: {0}")]
    EnvFile(#[from] dotenvy::Error),
    #[error("Environment variable error: {0}")]
    ConfigError(#[from] VarError),
}
