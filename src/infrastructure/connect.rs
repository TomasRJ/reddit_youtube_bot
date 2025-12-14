use std::str::FromStr;

use sqlx::{Error, SqlitePool, sqlite::SqliteConnectOptions};
use thiserror::Error;

use crate::infrastructure::Settings;

pub async fn get_pool(settings: &Settings) -> Result<SqlitePool, DbError> {
    let options = SqliteConnectOptions::from_str(&settings.database_url)?.create_if_missing(true);

    let pool = SqlitePool::connect_with(options).await?;

    Ok(pool)
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] Error),
}
