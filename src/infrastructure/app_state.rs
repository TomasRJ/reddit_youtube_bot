use std::sync::Arc;

use handlebars::Handlebars;
use sqlx::SqlitePool;

use crate::infrastructure::{connect::get_pool, settings::Settings};

#[derive(Clone)]
pub struct AppState {
    pub db_pool: SqlitePool,
    pub hb: Handlebars<'static>,
}

impl AppState {
    pub async fn new(settings: Settings) -> Arc<Self> {
        let db_pool = get_pool(&settings)
            .await
            .expect("Error connecting to local SQLite DB.");

        let mut hb = Handlebars::new();
        hb.register_template_file("whole_document", "frontend/base_layout.html")
            .expect("Error parsing base_layout template");

        Arc::new(Self { db_pool, hb })
    }
}
