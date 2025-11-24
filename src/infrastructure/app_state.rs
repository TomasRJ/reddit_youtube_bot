use std::sync::Arc;

use crate::infrastructure::settings::Settings;

#[derive(Clone)]
pub struct AppState {
    pub hmac_secret: String,
}

impl AppState {
    pub async fn new(settings: Settings) -> Arc<Self> {
        let hmac_secret = settings.hmac_secret.clone();

        Arc::new(Self { hmac_secret })
    }
}
