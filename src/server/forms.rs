use std::{
    collections::HashSet,
    sync::{Arc, LazyLock},
};

use axum::{Form, extract::State, response::Redirect};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    infrastructure::AppState,
    server::{
        ApiError,
        repository::{register_subreddit_form, register_subscription_link, save_form_data},
        shared::{
            FormType, RedditAuthorization, RedditAuthorizeDuration, YouTubeSubscription,
            extract_channel_id_from_topic_url, subscribe_to_channel,
        },
    },
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(reddit_authorize_submission))
        .routes(routes!(youtube_channel_subscribe))
        .routes(routes!(register_subreddit))
        .routes(routes!(link_subscription))
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        ApiError::InternalError(format!(
            "Error deserializing/serializing JSON data: {}",
            error
        ))
    }
}

static REDDIT_SCOPES: LazyLock<HashSet<&str>> = LazyLock::new(|| {
    HashSet::from([
        "edit",
        "flair",
        "history",
        "identity",
        "modconfig",
        "modflair",
        "modlog",
        "modposts",
        "modwiki",
        "mysubreddits",
        "privatemessages",
        "read",
        "report",
        "save",
        "submit",
        "subscribe",
        "vote",
        "wikiedit",
        "wikiread",
    ])
});

#[derive(Serialize, Deserialize, ToSchema)]
pub struct RedditAuthorizeForm {
    pub moderate_submissions: bool,
    pub duration: RedditAuthorizeDuration,
    pub scopes: String,
}

impl RedditAuthorizeForm {
    fn validate(authorize_form_data: &Self) -> Result<RedditAuthorization, ApiError> {
        let scopes = authorize_form_data.scopes.trim().trim_matches(',').trim();

        if scopes.is_empty() {
            return Err(ApiError::BadRequest(
                "client_id, redirect_url or scope input was empty".into(),
            ));
        }

        let mut input_scopes = HashSet::new();

        for scope in scopes
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
        {
            if !&REDDIT_SCOPES.contains(scope.as_str()) {
                return Err(ApiError::BadRequest(format!("Invalid scope: {}", scope)));
            }

            if !input_scopes.insert(scope.clone()) {
                return Err(ApiError::BadRequest(format!("Duplicate scope: {}", scope)));
            }
        }

        if input_scopes.is_empty() {
            return Err(ApiError::BadRequest("No scopes provided".into()));
        }

        if !input_scopes.contains("identity") {
            return Err(ApiError::BadRequest("'identity' scope needed".into()));
        }

        Ok(RedditAuthorization {
            r#type: FormType::Reddit,
            moderate_submissions: authorize_form_data.moderate_submissions,
            duration: authorize_form_data.duration.clone(),
            scopes: scopes.to_string(),
        })
    }
}

/// Reddit authorize URL redirect
#[utoipa::path(
        post,
        request_body(content = RedditAuthorizeForm, description = "Create the Reddit authorize URL from Reddit authorize form", content_type = "application/x-www-form-urlencoded"),
        path = "/reddit",
        description = "Redirect to Reddit authorize URL via from input",
        responses(
            (status = 303, description = "Reddit authorize URL redirect."),
            (status = 400, description = "Invalid form data."),
            (status = 500, description = "Internal server error."),
        ),
        tag = "forms"
    )]
#[axum::debug_handler]
async fn reddit_authorize_submission(
    State(state): State<Arc<AppState>>,
    Form(form_input): Form<RedditAuthorizeForm>,
) -> Result<Redirect, ApiError> {
    let reddit_authorization = RedditAuthorizeForm::validate(&form_input)?;

    let uuid = Uuid::new_v4();
    let authorize_submission_json_str = serde_json::to_string(&reddit_authorization)?;
    save_form_data(
        &state.db_pool,
        &uuid.to_string(),
        &authorize_submission_json_str,
    )
    .await?;

    let authorize_url = format!(
        "https://www.reddit.com/api/v1/authorize?client_id={client_id}&response_type=code&state={state_string}&redirect_uri={redirect_url}&duration={duration}&scope={scope_string}",
        client_id = state.reddit_credentials.client_id,
        state_string = uuid,
        redirect_url = format!("{}/reddit/callback", &state.base_url),
        duration = reddit_authorization.duration,
        scope_string = reddit_authorization.scopes
    );

    Ok(Redirect::to(&authorize_url))
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct YouTubeSubscribeForm {
    pub topic_url: String,
    pub hmac_secret: String,
    pub post_shorts: bool,
}

impl YouTubeSubscribeForm {
    fn validate(subscription: &Self) -> Result<(YouTubeSubscription, String), ApiError> {
        let topic_url = subscription.topic_url.trim();
        let hmac_secret = subscription.hmac_secret.trim();
        let channel_id = extract_channel_id_from_topic_url(&subscription.topic_url)?;

        if topic_url.is_empty() || hmac_secret.is_empty() || channel_id.is_empty() {
            return Err(ApiError::BadRequest(format!(
                "Topic URL, HMAC secret, callback url or channel id input was empty. Inputted Topic URL: '{}' HMAC secret: '{}' channel id '{}'",
                topic_url, hmac_secret, channel_id
            )));
        }

        let uuid_str = Uuid::now_v7().to_string();

        Ok((
            YouTubeSubscription {
                r#type: FormType::Youtube,
                topic_url: topic_url.to_string(),
                channel_id: channel_id.to_string(),
                hmac_secret: hmac_secret.to_string(),
                post_shorts: subscription.post_shorts,
            },
            uuid_str,
        ))
    }
}

/// YouTube channel subscribe
#[utoipa::path(
        post,
        request_body(content = YouTubeSubscribeForm, description = "Create the Reddit authorize URL from Reddit authorize form", content_type = "application/x-www-form-urlencoded"),
        path = "/subscribe",
        description = "Subscribe to a YouTube channel via form input",
        responses(
            (status = 303, description = "Successfully subscribed to Youtube channel redirect to home page."),
            (status = 400, description = "Invalid form data."),
            (status = 500, description = "Internal server error."),
        ),
        tag = "forms"
    )]
#[axum::debug_handler]
async fn youtube_channel_subscribe(
    State(state): State<Arc<AppState>>,
    Form(form_input): Form<YouTubeSubscribeForm>,
) -> Result<Redirect, ApiError> {
    let (subscription, uuid_str) = YouTubeSubscribeForm::validate(&form_input)?;
    println!(
        "New YouTube subscription request for YouTube channel: https://www.youtube.com/channel/{}",
        &subscription.channel_id
    );

    let subscription_json_str = serde_json::to_string(&subscription)?;

    save_form_data(&state.db_pool, &uuid_str, &subscription_json_str).await?;

    subscribe_to_channel(
        &format!("{}/google/subscription/{}", &state.base_url, uuid_str),
        &subscription.channel_id,
        &subscription.hmac_secret,
    )
    .await?;

    Ok(Redirect::to(&state.base_url))
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
struct RegisterSubredditForm {
    pub subreddit_name: String,
    #[serde(deserialize_with = "empty_string_is_none")]
    pub submission_title_prefix: Option<String>,
    #[serde(deserialize_with = "empty_string_is_none")]
    pub submission_title_suffix: Option<String>,
    #[serde(deserialize_with = "empty_string_is_none")]
    pub submission_flair_id: Option<String>,
}

fn empty_string_is_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    Ok(s.filter(|s| !s.trim().is_empty()))
}

/// Register a new subreddit
#[utoipa::path(
        post,
        request_body(content = RegisterSubredditForm, description = "Register a new subreddit", content_type = "application/x-www-form-urlencoded"),
        path = "/register_subreddit",
        description = "Register a new subreddit to submit videos to.",
        responses(
            (status = 303, description = "Successfully registered a new subreddit."),
            (status = 400, description = "Invalid form data."),
            (status = 500, description = "Internal server error."),
        ),
        tag = "forms"
    )]
#[axum::debug_handler]
async fn register_subreddit(
    State(state): State<Arc<AppState>>,
    Form(form_input): Form<RegisterSubredditForm>,
) -> Result<Redirect, ApiError> {
    register_subreddit_form(
        &state.db_pool,
        &form_input.subreddit_name,
        &form_input.submission_title_prefix,
        &form_input.submission_title_suffix,
        &form_input.submission_flair_id,
    )
    .await?;

    println!(
        "Successfully registered {} to the DB.",
        &form_input.subreddit_name
    );

    Ok(Redirect::to(&state.base_url))
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
struct LinkSubscriptionForm {
    pub subscription_id: String,
    pub reddit_account_id: String,
    pub subreddit_id: i64,
}

/// Link subscription to reddit account with subreddit
#[utoipa::path(
        post,
        request_body(content = LinkSubscriptionForm, description = "Chosen subscription, reddit account and subreddit w/ potential title prefix/suffix and flair id.", content_type = "application/x-www-form-urlencoded"),
        path = "/link_subscription",
        description = "Link a subscription to use  reddit account on to submit videos to subreddit",
        responses(
            (status = 303, description = "Successfully sLink a subscription to use  reddit account on to submit videos to subreddit."),
            (status = 400, description = "Invalid form data."),
            (status = 500, description = "Internal server error."),
        ),
        tag = "forms"
    )]
#[axum::debug_handler]
async fn link_subscription(
    State(state): State<Arc<AppState>>,
    Form(form_input): Form<LinkSubscriptionForm>,
) -> Result<Redirect, ApiError> {
    Uuid::try_parse(&form_input.subscription_id)?;
    Uuid::try_parse(&form_input.reddit_account_id)?;

    println!("link_subscription: {:?}", form_input);

    register_subscription_link(
        &state.db_pool,
        &form_input.subscription_id,
        &form_input.reddit_account_id,
        &form_input.subreddit_id,
    )
    .await?;

    println!("Successfully linked subscription to reddit account and subreddit.");

    Ok(Redirect::to(&state.base_url))
}
