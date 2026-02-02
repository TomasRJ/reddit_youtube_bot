use std::sync::Arc;

use axum::{extract::State, response::Html};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    infrastructure::AppState,
    server::{
        ApiError,
        repository::{Subscription, fetch_reddit_accounts, fetch_subscriptions},
        shared::RedditAccountDTO,
    },
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(main_landing_page))
}

impl From<handlebars::RenderError> for ApiError {
    fn from(error: handlebars::RenderError) -> Self {
        ApiError::InternalError(format!(
            "Error when rendering data on HTML template: {:?}",
            error
        ))
    }
}

impl From<handlebars::TemplateError> for ApiError {
    fn from(error: handlebars::TemplateError) -> Self {
        ApiError::InternalError(format!("Error on parsing HTML template: {:?}", error))
    }
}

const DATE_FORMAT_STR: &str = "%Y-%m-%d %H:%M:%S (UTC)";

mod date_format {
    use chrono::{DateTime, Utc};
    use serde::{self, Serializer};

    use crate::server::frontend::DATE_FORMAT_STR;

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(DATE_FORMAT_STR));
        serializer.serialize_str(&s)
    }
}

#[derive(Serialize)]
struct FrontendRedditAccountData {
    pub username: String,
    #[serde(with = "date_format")]
    pub expires_at: DateTime<Utc>,
}

impl FrontendRedditAccountData {
    fn convert(reddit_account: &RedditAccountDTO) -> Result<Self, ApiError> {
        Ok(FrontendRedditAccountData {
            username: reddit_account.username.clone(),
            expires_at: DateTime::from_timestamp_secs(reddit_account.expires_at).ok_or(
                ApiError::InternalError(format!(
                        "Could not parse reddit account expires_at value, out-of-range number of seconds: {}",
                        reddit_account.expires_at
                    )),
            )?,
        })
    }
}

mod optional_date_format {
    use chrono::{DateTime, Utc};
    use serde::{self, Serializer};

    use crate::server::frontend::DATE_FORMAT_STR;

    pub fn serialize<S>(date: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = match date {
            Some(d) => format!("{}", d.format(DATE_FORMAT_STR)),
            None => String::from("No expiration date"),
        };
        serializer.serialize_str(&s)
    }
}

#[derive(Serialize)]
struct FrontendSubscriptionsData {
    pub channel_id: String,
    pub channel_name: String,
    #[serde(with = "optional_date_format")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl FrontendSubscriptionsData {
    fn convert(subscription: &Subscription) -> Result<Self, ApiError> {
        Ok(FrontendSubscriptionsData {
            channel_id: subscription.channel_id.clone(),
            channel_name: subscription.channel_name.clone(),
            expires_at: match subscription.expires {
                Some(expires_at) => Some(DateTime::from_timestamp_secs(expires_at).ok_or(
                    ApiError::InternalError(format!(
                        "Could not parse subscription expires_at value, out-of-range number of seconds: {}",
                        expires_at
                    )),
                )?),
                None => None,
            },
        })
    }
}

/// Main landing page
#[utoipa::path(
        get,
        path = "/",
        description = "Main landing page",
        responses(
            (status = 200, description = "Main landing page html.", content_type = "text/html; charset=utf-8")
        ),
        tag = "frontend"
    )]
#[axum::debug_handler]
async fn main_landing_page(State(state): State<Arc<AppState>>) -> Result<Html<String>, ApiError> {
    let mut local_hb = state.hb.clone();

    let subscriptions = fetch_subscriptions(&state.db_pool)
        .await?
        .iter()
        .map(FrontendSubscriptionsData::convert)
        .collect::<Result<Vec<FrontendSubscriptionsData>, ApiError>>()?;

    let reddit_accounts = fetch_reddit_accounts(&state.db_pool)
        .await?
        .iter()
        .map(FrontendRedditAccountData::convert)
        .collect::<Result<Vec<FrontendRedditAccountData>, ApiError>>()?;

    local_hb.register_template_file("subscriptions", "frontend/subscriptions.html")?;

    local_hb.register_template_file("reddit_accounts", "frontend/reddit_accounts.html")?;

    local_hb.register_template_file("body_content", "frontend/landing_page.html")?;

    let data = json!({
        "reddit_accounts": reddit_accounts,
        "subscriptions": subscriptions
    });

    let whole_document = local_hb.render("whole_document", &data)?;

    Ok(Html(whole_document))
}
