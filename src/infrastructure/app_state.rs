use std::sync::Arc;

use sqlx::SqlitePool;

use crate::infrastructure::{connect::get_pool, settings::Settings};

#[derive(Clone)]
pub struct AppState {
    pub db_pool: SqlitePool,
}

impl AppState {
    pub async fn new(settings: Settings) -> Arc<Self> {
        let db_pool = get_pool(&settings)
            .await
            .expect("Error connecting to local SQLite DB.");

        Arc::new(Self { db_pool })
    }
}
