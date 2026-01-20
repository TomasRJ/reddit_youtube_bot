use std::{collections::HashSet, sync::Arc};

use axum::{Form, extract::State, response::Redirect};

use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    infrastructure::AppState,
    server::{
        ApiError,
        repository::save_form_data,
        shared::{FormType, RedditAuthorizeForm},
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

impl RedditAuthorizeForm {
    fn validate(authorize_form_data: &Self) -> Result<Self, ApiError> {
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

        Ok(Self {
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
        description = "Main landing page",
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
    Form(authorize): Form<RedditAuthorizeForm>,
) -> Result<Redirect, ApiError> {
    let form_data = RedditAuthorizeForm::validate(&authorize)?;

    let uuid = Uuid::new_v4();
    let authorize_submission_json_str = serde_json::to_string(&form_data)?;
    save_form_data(
        &state.db_pool,
        &uuid.to_string(),
        &authorize_submission_json_str,
    )
    .await?;

    let authorize_url = format!(
        "https://www.reddit.com/api/v1/authorize?client_id={client_id}&response_type=code&state={state_string}&redirect_uri={redirect_url}&duration={duration}&scope={scope_string}",
        client_id = form_data.client_id,
        state_string = uuid,
        redirect_url = form_data.redirect_url,
        duration = form_data.duration,
        scope_string = form_data.scopes
    );

    Ok(Redirect::to(&authorize_url))
}
