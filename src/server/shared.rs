use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct RedditAuthorizeForm {
    pub r#type: FormType,
    pub client_id: String,
    pub secret: String,
    pub user_agent: String,
    pub redirect_url: String,
    pub duration: RedditAuthorizeDuration,
    pub scopes: String,
}

#[derive(Serialize, Deserialize, ToSchema, serde_textual::DisplaySerde, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum RedditAuthorizeDuration {
    Permanent,
    Temporary,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema, Clone, Debug)]
pub enum FormType {
    Reddit,
    Youtube,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RedditOAuthToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub scope: String,
    pub refresh_token: Option<String>,
}
