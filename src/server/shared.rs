use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_textual::DisplaySerde;
use utoipa::ToSchema;

use crate::server::ApiError;

// Structs
#[derive(Debug, Clone)]
pub struct RedditCredentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RedditAuthorization {
    pub r#type: FormType,
    pub moderate_submissions: bool,
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

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct Feed {
    #[serde(rename = "link")]
    pub links: Vec<Link>,
    pub title: String,
    pub updated: DateTime<Utc>,
    pub entry: Entry,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct Link {
    #[serde(rename = "@rel")]
    pub rel: String,
    #[serde(rename = "@href")]
    pub href: String,
    #[serde(rename = "@hreflang")]
    pub hreflang: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct Entry {
    pub id: String,
    #[serde(rename = "videoId")]
    pub yt_video_id: String,
    #[serde(rename = "channelId")]
    pub yt_channel_id: String,
    pub title: String,
    #[serde(rename = "link")]
    pub links: Vec<Link>,
    pub author: Author,
    pub published: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[allow(dead_code)]
pub struct SimpleEntry {
    pub id: String,
    pub yt_video_id: String,
    pub yt_channel_id: String,
    pub title: String,
    pub link: Link,
    pub author: Author,
    pub published: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl Into<Option<SimpleEntry>> for &Entry {
    fn into(self) -> Option<SimpleEntry> {
        let entry_link = self
            .links
            .iter()
            .find(|l| l.rel == "alternate" && l.hreflang.is_none())
            .or_else(|| self.links.first());

        match entry_link {
            Some(link) => Some(SimpleEntry {
                id: self.id.clone(),
                yt_video_id: self.yt_video_id.clone(),
                yt_channel_id: self.yt_channel_id.clone(),
                title: self.title.clone(),
                link: link.clone(),
                author: self.author.clone(),
                published: self.published,
                updated: self.updated,
            }),
            None => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct Author {
    pub name: String,
    pub uri: String,
}
pub struct RedditAccountDTO {
    pub id: String,
    pub username: String,
    pub moderate_submissions: bool,
    pub oauth_token: String,
    pub expires_at: i64,
}

pub struct RedditAccount {
    pub id: String,
    pub username: String,
    pub oauth_token: RedditOAuthToken,
    pub moderate_submissions: bool,
}

#[derive(Serialize)]
pub struct Subreddit {
    pub id: i64,
    pub name: String,
    pub title_prefix: Option<String>,
    pub title_suffix: Option<String>,
    pub flair_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RedditSubmissionData {
    pub url: String,
    #[serde(rename = "name")]
    pub id: String,
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

#[derive(Serialize)]
pub struct LinkedSubscription {
    pub subscription_id: String,
    pub channel_name: String,
    pub reddit_account_id: String,
    pub reddit_username: String,
    pub subreddit_id: i64,
    pub subreddit_name: String,
}

// Static vars
pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .user_agent("reddit_youtube_bot v0.1.0 by Tomas R J. Source code: https://github.com/TomasRJ/reddit_youtube_bot")
        .build()
        .expect("Failed to create HTTP client")
});

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
    let subscription_client = &HTTP_CLIENT;

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
