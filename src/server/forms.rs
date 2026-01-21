use std::{collections::HashSet, sync::Arc};

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
        shared::{FormType, RedditAuthorization, RedditAuthorizeDuration},
    },
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(reddit_authorize_submission))
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        ApiError::InternalError(format!(
            "Error deserializing/serializing JSON data: {}",
            error
        ))
    }
}

const REDDIT_SCOPES: [&'static str; 19] = [
    "identity",
    "edit",
    "flair",
    "history",
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
];

#[derive(Serialize, Deserialize, ToSchema)]
pub struct RedditAuthorizeForm {
    pub client_id: String,
    pub secret: String,
    pub redirect_url: String,
    pub duration: RedditAuthorizeDuration,
    pub scopes: String,
}

impl RedditAuthorizeForm {
    fn validate(authorize_form_data: &Self) -> Result<RedditAuthorization, ApiError> {
        let scopes = authorize_form_data.scopes.trim().trim_matches(',').trim();
        let client_id = authorize_form_data.client_id.trim();
        let redirect_url = authorize_form_data.redirect_url.trim();
        let secret = authorize_form_data.secret.trim();

        if client_id.is_empty() || redirect_url.is_empty() || scopes.is_empty() || secret.is_empty()
        {
            return Err(ApiError::BadRequest(
                "client_id, redirect_url or scope input was empty".into(),
            ));
        }

        let reddit_scope_set: HashSet<&str> = REDDIT_SCOPES.into_iter().collect();
        let mut seen = HashSet::new();

        for scope in scopes.split(',').filter(|s| !s.is_empty()) {
            if !reddit_scope_set.contains(scope) {
                return Err(ApiError::BadRequest(format!("Invalid scope: {}", scope)));
            }

            if !seen.insert(scope) {
                return Err(ApiError::BadRequest(format!("Duplicate scope: {}", scope)));
            }
        }

        if seen.is_empty() {
            return Err(ApiError::BadRequest("No scopes provided".into()));
        }

        if !seen.contains("identity") {
            return Err(ApiError::BadRequest("'identity' scope needed".into()));
        }

        Ok(RedditAuthorization {
            r#type: FormType::Reddit,
            client_id: client_id.to_string(),
            secret: secret.to_string(),
            user_agent: "reddit_youtube_bot v0.1.0 by Tomas R J".to_string(),
            redirect_url: redirect_url.to_string(),
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
        client_id = reddit_authorization.client_id,
        state_string = uuid,
        redirect_url = reddit_authorization.redirect_url,
        duration = reddit_authorization.duration,
        scope_string = reddit_authorization.scopes
    );

    Ok(Redirect::to(&authorize_url))
}
