use std::{collections::HashMap, sync::Arc};

use axum::extract::{Query, State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_textual::DisplaySerde;
use sqlx::{Pool, Sqlite};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    infrastructure::AppState,
    server::{
        ApiError,
        repository::{
            fetch_form_data, fetch_reddit_accounts_for_subscription,
            fetch_submissions_on_subreddit, save_reddit_account, update_reddit_oauth_token,
            update_reddit_submission_sticky_state,
        },
        shared::{
            self, HTTP_CLIENT, RedditAccount, RedditAuthorization, RedditOAuthToken,
            RedditSubmissionData, Subreddit,
        },
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
        ApiError::InternalError(format!("Web request failed: {}", error))
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

    let client = &HTTP_CLIENT;

    let oauth_token = client
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
        .text()
        .await?;

    let oauth_token: RedditOAuthToken = serde_json::from_str(&oauth_token).map_err(|e| {
        ApiError::BadRequest(format!(
            "Error parsing Reddit OAuth token response body: {}. Response body was: {}. The form data was: {:?}",
            e, oauth_token, reddit_auth_form_data
        ))
    })?;

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
                "'name' property missing from https://oauth.reddit.com/api/v1/me response.".into(),
            )
        })?;

    save_reddit_account(
        &state.db_pool,
        &reddit_user_name,
        &oauth_token,
        reddit_auth_form_data,
    )
    .await?;

    Ok(())
}

pub async fn get_associated_reddit_accounts_for_subscription(
    state: &Arc<AppState>,
    subscription_id: &String,
) -> Result<Vec<RedditAccount>, ApiError> {
    let raw_reddit_accounts =
        fetch_reddit_accounts_for_subscription(&state.db_pool, subscription_id).await?;
    let mut reddit_accounts = Vec::new();

    for reddit_account in raw_reddit_accounts {
        let mut oauth_token: RedditOAuthToken = serde_json::from_str(&reddit_account.oauth_token)?;

        if let Some(refresh_token) = &oauth_token.refresh_token
            && Utc::now().timestamp() >= reddit_account.expires_at
        {
            println!(
                "The OAuth token for https://www.reddit.com/user/{} has expired, refreshing token.",
                reddit_account.username
            );

            oauth_token = refresh_reddit_oauth_token(
                &reddit_account.client_id,
                &reddit_account.user_secret,
                refresh_token,
            )
            .await?;

            update_reddit_oauth_token(&state.db_pool, &reddit_account.id, &oauth_token).await?;
        }

        reddit_accounts.push(RedditAccount {
            id: reddit_account.id,
            username: reddit_account.username.clone(),
            oauth_token,
            moderate_submissions: reddit_account.moderate_submissions,
        });
    }

    Ok(reddit_accounts)
}

pub async fn refresh_reddit_oauth_token(
    client_id: &String,
    user_secret: &String,
    refresh_token: &String,
) -> Result<RedditOAuthToken, ApiError> {
    let client = &HTTP_CLIENT;

    let oauth_token: RedditOAuthToken = client
        .post("https://www.reddit.com/api/v1/access_token")
        .basic_auth(client_id, Some(user_secret))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?
        .json()
        .await?;

    Ok(oauth_token)
}

pub async fn submit_video_to_subreddit(
    reddit_account: &RedditAccount,
    subreddit: &Subreddit,
    entry: &shared::Entry,
) -> Result<RedditSubmissionData, ApiError> {
    let title = format!(
        "{prefix}{title}{suffix}",
        prefix = &subreddit.title_prefix.clone().unwrap_or("".to_string()),
        title = entry.title,
        suffix = &subreddit.title_suffix.clone().unwrap_or("".to_string())
    );

    let mut submission_form = HashMap::from([
        ("api_type", "json"),
        ("extension", "json"),
        ("kind", "link"),
        ("resubmit", "true"),
        ("sendreplies", "false"),
        ("sr", &subreddit.name),
        ("title", &title),
        ("url", &entry.link.href),
    ]);

    if let Some(flair_id) = &subreddit.flair_id {
        submission_form.insert("flair_id", &flair_id);
    }

    let client = &HTTP_CLIENT;

    let submission_response = client
        .post("https://oauth.reddit.com/api/submit")
        .bearer_auth(reddit_account.oauth_token.access_token.clone())
        .form(&submission_form)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let submission_errors = submission_response["json"]["errors"].as_array();

    if let Some(errors) = submission_errors
        && !errors.is_empty()
    {
        return Err(ApiError::BadRequest(format!(
            "The video (title: '{}' link: {}) from '{}' (link: {}) could not be submitted, got following errors: {:#?}",
            title, entry.link.href, entry.author.name, entry.author.uri, errors
        )));
    }

    let submission_data: RedditSubmissionData =
        serde_json::from_value(submission_response["json"]["data"].clone())?;

    Ok(submission_data)
}

pub async fn moderate_submission(
    state: &Arc<AppState>,
    reddit_account: &RedditAccount,
    subreddit: &Subreddit,
) -> Result<(), ApiError> {
    let subreddit_submissions =
        fetch_submissions_on_subreddit(&state.db_pool, subreddit.id).await?;

    if subreddit_submissions.is_empty() {
        println!(
            "The Reddit account https://www.reddit.com/u/{} has no submissions on the https://www.reddit.com/r/{} subreddit.",
            reddit_account.username, subreddit.name
        );
        return Ok(());
    }

    // subreddit_submissions is ordered by timestamp ascending
    let oldest_stickied_submission = subreddit_submissions.iter().find(|s| s.stickied);

    let previous_submission = subreddit_submissions
        .iter()
        .filter(|s| !s.stickied)
        .rev() // Start from the end (the newest)
        .nth(1); // Skip index 0 (the last), take index 1 (the previous submission)

    let (oldest_stickied_submission, previous_submission) = if let Some(old) =
        oldest_stickied_submission
        && let Some(prev) = previous_submission
    {
        (old, prev)
    } else {
        println!(
            "The Reddit account https://www.reddit.com/u/{} has no submission on the https://www.reddit.com/r/{} subreddit.",
            reddit_account.username, subreddit.name
        );
        return Ok(());
    };

    set_reddit_submission_sticky_state(&state.db_pool, &oldest_stickied_submission.id, &false)
        .await?;
    set_reddit_submission_sticky_state(&state.db_pool, &previous_submission.id, &true).await?;

    Ok(())
}

async fn set_reddit_submission_sticky_state(
    pool: &Pool<Sqlite>,
    submission_id: &String,
    state: &bool,
) -> Result<(), ApiError> {
    let client = &HTTP_CLIENT;

    let sticky_response = client
        .post("https://oauth.reddit.com/api/set_subreddit_sticky")
        .form(&[
            ("api_type", "json"),
            ("id", submission_id),
            ("state", &state.to_string()),
        ])
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let sticky_errors = sticky_response["json"]["errors"].as_array();

    if let Some(errors) = sticky_errors
        && !errors.is_empty()
    {
        return Err(ApiError::BadRequest(format!(
            "Got following errors while trying to change the submissions (link: https://redd.it/{}) sticky state ({}): {:#?}",
            &submission_id[3..],
            state,
            errors
        )));
    }

    update_reddit_submission_sticky_state(&pool, &submission_id, &state).await?;

    Ok(())
}
