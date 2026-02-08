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
        repository::save_form_data,
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
        redirect_url = state.reddit_credentials.redirect_url,
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
    pub callback_url: String,
}

impl YouTubeSubscribeForm {
    fn validate(subscription: &Self) -> Result<(YouTubeSubscription, String), ApiError> {
        let topic_url = subscription.topic_url.trim();
        let hmac_secret = subscription.hmac_secret.trim();
        let callback_url = subscription.callback_url.trim().trim_matches('/').trim();
        let channel_id = extract_channel_id_from_topic_url(&subscription.topic_url)?;

        if topic_url.is_empty()
            || hmac_secret.is_empty()
            || callback_url.is_empty()
            || channel_id.is_empty()
        {
            return Err(ApiError::BadRequest(format!(
                "Topic URL, HMAC secret, callback url or channel id input was empty. Inputted Topic URL: '{}' HMAC secret: '{}' callback url: '{}' channel id '{}'",
                topic_url, hmac_secret, callback_url, channel_id
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
                callback_url: format!(
                    "{origin}/google/subscription/{id}",
                    origin = callback_url,
                    id = uuid_str
                ),
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
            (status = 200, description = "Successfully subscribed to Youtube channel."),
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
        &subscription.callback_url,
        &subscription.channel_id,
        &subscription.hmac_secret,
    )
    .await?;

    // The callback_url value in form_input is the window.location.href inserted value
    Ok(Redirect::to(
        form_input.callback_url.trim().trim_matches('/').trim(),
    ))
}
