use std::sync::Arc;

use handlebars::Handlebars;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::{
    infrastructure::{connect::get_pool, settings::Settings},
    server::SubCommand,
};

#[derive(Clone)]
pub struct AppState {
    pub db_pool: SqlitePool,
    pub hb: Handlebars<'static>,
    pub scheduler_sender: mpsc::Sender<SubCommand>,
}

impl AppState {
    pub async fn new(settings: Settings) -> (Arc<Self>, mpsc::Receiver<SubCommand>) {
        let db_pool = get_pool(&settings)
            .await
            .expect("Error connecting to local SQLite DB.");

        let mut hb = Handlebars::new();
        hb.register_template_file("whole_document", "frontend/base_layout.html")
            .expect("Error parsing base_layout template");

        let (scheduler_sender, scheduler_receiver) = mpsc::channel(100);

        (
            Arc::new(Self {
                db_pool,
                hb,
                scheduler_sender,
            }),
            scheduler_receiver,
        )
    }
}
