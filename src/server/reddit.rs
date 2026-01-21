use std::sync::Arc;

use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use serde_textual::DisplaySerde;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    infrastructure::AppState,
    server::{
        ApiError,
        repository::{fetch_form_data, save_reddit_oauth_token},
        shared::{RedditAuthorization, RedditOAuthToken},
    },
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(reddit_callback))
}

impl From<uuid::Error> for ApiError {
    fn from(error: uuid::Error) -> Self {
        ApiError::BadRequest(format!(
            "The state string from the Reddit authorize callback was not a valid UUID: {:?}",
            error
        ))
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(error: reqwest::Error) -> Self {
        ApiError::BadRequest(format!("Web request failed: {}", error))
    }
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct RedditCallback {
    pub code: String,
    pub state: String,
    pub error: Option<RedditCallbackErrors>,
}
impl RedditCallback {
    fn validate(
        state_str: &String,
        callback_errors: &Option<RedditCallbackErrors>,
    ) -> Result<Uuid, ApiError> {
        if let Some(error) = callback_errors {
            match error {
                RedditCallbackErrors::AccessDenied => {
                    return Err(ApiError::BadRequest(
                        "The user denied access to their account.".into(),
                    ));
                }
                _ => {
                    return Err(ApiError::BadRequest(format!(
                        "Reddit callback had following error: {}",
                        error
                    )));
                }
            }
        }

        let uuid = Uuid::try_parse(state_str)?;

        Ok(uuid)
    }
}

#[derive(Serialize, Deserialize, ToSchema, DisplaySerde, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RedditCallbackErrors {
    AccessDenied,
    UnsupportedResponseType,
    InvalidScope,
    InvalidRequest,
}

/// Reddit authorize URL redirect
#[utoipa::path(
        get,
        params(
            ("code" = String, Query, description = "A one-time use code that may be exchanged for a bearer token."),
            ("state" = String, Query, description = "This value should be the same as the one sent in the initial authorization request."),
            ("error" = Option<RedditCallbackErrors>, Query, description = "One of following error codes: access_denied, unsupported_response_type, invalid_scope or invalid_request")
        ),
        path = "/callback",
        description = "Reddit authorize URL redirect used to retrieve the Reddit OAuth token for a given Reddit account.",
        responses(
            (status = 200, description = "Reddit OAuth token received and stored."),
            (status = 400, description = "Invalid form data."),
            (status = 500, description = "Internal server error."),
        ),
        tag = "reddit"
    )]
#[axum::debug_handler]
async fn reddit_callback(
    State(state): State<Arc<AppState>>,
    Query(callback): Query<RedditCallback>,
) -> Result<(), ApiError> {
    let state_uuid = RedditCallback::validate(&callback.state, &callback.error)?;

    let reddit_auth_form_data: RedditAuthorization =
        fetch_form_data(&state.db_pool, &state_uuid.to_string()).await?;

    let client = reqwest::Client::new();

    let oauth_token: RedditOAuthToken = client
        .post("https://www.reddit.com/api/v1/access_token")
        .basic_auth(
            &reddit_auth_form_data.client_id,
            Some(&reddit_auth_form_data.secret),
        )
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &callback.code),
            ("redirect_uri", &reddit_auth_form_data.redirect_url),
        ])
        .send()
        .await?
        .json()
        .await?;

    if !oauth_token.scope.contains("identity") {
        return Err(ApiError::BadRequest(
            "'identity' Reddit API scope needed for to get username".into(),
        ));
    }

    // uses serde_json::Value since the 'name' property is the only value wanted
    let reddit_user_name = client
        .get("https://oauth.reddit.com/api/v1/me")
        .bearer_auth(&oauth_token.access_token)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?["name"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or({
            ApiError::InternalError(
                "'name' field missing from reddit.com/api/v1/me response.".into(),
            )
        })?;

    save_reddit_oauth_token(
        &state.db_pool,
        &reddit_user_name,
        &oauth_token,
        reddit_auth_form_data,
    )
    .await?;

    Ok(())
}
