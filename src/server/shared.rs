use std::sync::OnceLock;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_textual::DisplaySerde;
use utoipa::ToSchema;

use crate::server::ApiError;

// Structs
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RedditAuthorization {
    pub r#type: FormType,
    pub client_id: String,
    pub secret: String,
    pub user_agent: String,
    pub redirect_url: String,
    pub duration: RedditAuthorizeDuration,
    pub scopes: String,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct YouTubeSubscription {
    pub r#type: FormType,
    pub topic_url: String,
    pub channel_id: String,
    pub hmac_secret: String,
    pub callback_url: String,
    pub post_shorts: bool,
}

#[derive(Deserialize, ToSchema, Debug)]
pub struct Verification {
    #[serde(rename = "hub.mode")]
    pub mode: VerificationMode,
    #[serde(rename = "hub.topic")]
    pub topic: String,
    #[serde(rename = "hub.challenge")]
    pub challenge: String,
    #[serde(rename = "hub.lease_seconds")]
    pub lease_seconds: Option<i64>,
}

// Enums
#[derive(Deserialize, ToSchema, Debug)]
pub enum VerificationMode {
    #[serde(rename = "subscribe")]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
}

#[derive(Serialize, Deserialize, ToSchema, DisplaySerde, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum RedditAuthorizeDuration {
    Permanent,
    Temporary,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum FormType {
    Reddit,
    Youtube,
}

pub enum SubCommand {
    Schedule {
        subscription_id: String,
        wait_secs: i64,
    },
}

// Static vars
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

// Functions
pub fn get_http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(Client::new)
}

pub fn extract_channel_id_from_topic_url(topic_url: &String) -> Result<&str, ApiError> {
    if let Some(("https://www.youtube.com/xml/feeds/videos.xml?channel_id", channel_id)) =
        topic_url.split_once('=')
    {
        Ok(channel_id.trim())
    } else {
        Err(ApiError::BadRequest(format!(
            "The topic URL has to contain 'https://www.youtube.com/xml/feeds/videos.xml?channel_id=', the input was: {:}",
            topic_url
        )))
    }
}

pub async fn subscribe_to_channel(
    callback_url: &String,
    channel_id: &String,
    hmac_secret: &String,
) -> Result<(), ApiError> {
    let subscription_client = get_http_client();

    let topic_url = format!(
        "https://www.youtube.com/xml/feeds/videos.xml?channel_id={}",
        &channel_id
    );

    let subscription_res = subscription_client
        .post("https://pubsubhubbub.appspot.com/subscribe")
        .form(&[
            ("hub.callback", callback_url),
            ("hub.mode", &"subscribe".to_string()),
            ("hub.topic", &topic_url),
            ("hub.secret", hmac_secret),
        ])
        .send()
        .await?;

    Ok(match subscription_res.error_for_status() {
        Ok(_) => println!(
            "Successfully sent Google PubSubHubbub subscription request, now waiting for verification"
        ),
        Err(err) => return Err(err.into()),
    })
}
