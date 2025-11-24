use std::env::{self, VarError};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub hmac_secret: String,
}

impl Settings {
    pub fn new() -> Result<Self, SettingsError> {
        dotenvy::dotenv()?;

        Ok(Self {
            hmac_secret: env::var("HMAC_SECRET")?,
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
