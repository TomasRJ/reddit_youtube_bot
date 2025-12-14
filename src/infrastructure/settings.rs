use std::env::{self, VarError};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub database_url: String,
}

impl Settings {
    pub fn new() -> Result<Self, SettingsError> {
        dotenvy::dotenv()?;

        Ok(Self {
            database_url: env::var("DATABASE_URL")?,
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
