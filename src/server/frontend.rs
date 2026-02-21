use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::Html,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    infrastructure::AppState,
    server::{
        ApiError,
        repository::{
            Subscription, fetch_reddit_accounts, fetch_subscriptions, get_reddit_account_by_id,
            get_subscription_by_id,
        },
        shared::RedditAccountDTO,
    },
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(main_landing_page))
        .routes(routes!(reddit_account_page))
        .routes(routes!(subscription_account_page))
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
    pub id: String,
    pub username: String,
    pub oauth_token: String,
    pub moderate_submissions: bool,
    #[serde(with = "date_format")]
    pub expires_at: DateTime<Utc>,
}

impl FrontendRedditAccountData {
    fn convert(reddit_account: &RedditAccountDTO) -> Result<Self, ApiError> {
        Ok(FrontendRedditAccountData {
            id: reddit_account.id.clone(),
            username: reddit_account.username.clone(),
            oauth_token: reddit_account.oauth_token.clone(),
            moderate_submissions: reddit_account.moderate_submissions,
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
struct FrontendSubscriptionData {
    pub id: String,
    pub channel_id: String,
    pub channel_name: String,
    pub hmac_secret: String,
    #[serde(with = "optional_date_format")]
    pub expires_at: Option<DateTime<Utc>>,
    pub post_shorts: bool,
}

impl FrontendSubscriptionData {
    fn convert(subscription: &Subscription) -> Result<Self, ApiError> {
        Ok(FrontendSubscriptionData {
            id: subscription.id.clone(),
            channel_id: subscription.channel_id.clone(),
            channel_name: subscription.channel_name.clone(),
            hmac_secret: subscription.hmac_secret.clone(),
            expires_at: match subscription.expires {
                Some(expires_at) => Some(DateTime::from_timestamp_secs(expires_at).ok_or(
                    ApiError::InternalError(format!(
                        "Could not parse subscription expires_at value, out-of-range number of seconds: {}",
                        expires_at
                    )),
                )?),
                None => None,
            },
            post_shorts: subscription.post_shorts
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
        .map(FrontendSubscriptionData::convert)
        .collect::<Result<Vec<FrontendSubscriptionData>, ApiError>>()?;

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

/// Reddit account page
#[utoipa::path(
        get,
        path = "/account/{id}",
        params(
            ("id" = String, Path, description = "Reddit account id", example = "019ba504-70f5-7f35-9c2c-2f02b992af7e"),
        ),
        description = "Reddit account page",
        responses(
            (status = 200, description = "Reddit account page html.", content_type = "text/html; charset=utf-8")
        ),
        tag = "frontend"
    )]
#[axum::debug_handler]
async fn reddit_account_page(
    State(state): State<Arc<AppState>>,
    Path(reddit_account_id): Path<String>,
) -> Result<Html<String>, ApiError> {
    Uuid::try_parse(&reddit_account_id).map_err(|_| ApiError::BadRequest("Invalid ID".into()))?;

    let mut local_hb = state.hb.clone();

    let reddit_account = get_reddit_account_by_id(&state.db_pool, &reddit_account_id)
        .await
        .map_err(|_| ApiError::NotFound("Account doesn't exist".into()))?;

    let reddit_account = FrontendRedditAccountData::convert(&reddit_account)?;

    local_hb.register_template_file("body_content", "frontend/reddit_account.html")?;

    let data = json!({
        "account": reddit_account,
    });

    let whole_document = local_hb.render("whole_document", &data)?;

    Ok(Html(whole_document))
}

/// Subscription page
#[utoipa::path(
        get,
        path = "/subscription/{id}",
        params(
            ("id" = String, Path, description = "Subscription id", example = "019ba504-70f5-7f35-9c2c-2f02b992af7e"),
        ),
        description = "Subscription page",
        responses(
            (status = 200, description = "Subscription page html.", content_type = "text/html; charset=utf-8")
        ),
        tag = "frontend"
    )]
#[axum::debug_handler]
async fn subscription_account_page(
    State(state): State<Arc<AppState>>,
    Path(subscription_account_id): Path<String>,
) -> Result<Html<String>, ApiError> {
    Uuid::try_parse(&subscription_account_id)
        .map_err(|_| ApiError::BadRequest("Invalid ID".into()))?;

    let mut local_hb = state.hb.clone();

    let subscription = get_subscription_by_id(&state.db_pool, &subscription_account_id)
        .await
        .map_err(|_| ApiError::NotFound("Subscription doesn't exist".into()))?;

    let subscription = FrontendSubscriptionData::convert(&subscription)?;

    local_hb.register_template_file("body_content", "frontend/subscription.html")?;

    let data = json!({
        "subscription": subscription,
    });

    let whole_document = local_hb.render("whole_document", &data)?;

    Ok(Html(whole_document))
}
