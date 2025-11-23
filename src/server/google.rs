use axum::{Json, extract::Query, http::HeaderMap};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::server::ApiError;

pub fn router() -> OpenApiRouter {
    OpenApiRouter::new()
    .routes(routes!(new_video_published))
    .routes(routes!(subscription_callback))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Feed {
    #[serde(rename = "link")]
    pub links: Vec<Link>,
    pub title: String,
    pub updated: DateTime<Utc>,
    pub entry: Entry,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Link {
    #[serde(rename = "@rel")]
    pub rel: String,
    #[serde(rename = "@href")]
    pub href: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Entry {
    pub id: String,
    #[serde(rename = "videoId")]
    pub yt_video_id: String,
    #[serde(rename = "channelId")]
    pub yt_channel_id: String,
    pub title: String,
    pub link: Link,
    pub author: Author,
    pub published: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Author {
    pub name: String,
    pub uri: String,
}

impl From<quick_xml::DeError> for ApiError {
    fn from(error: quick_xml::DeError) -> Self {
        ApiError::BadRequest(format!("Invalid XML request: {:?}", error))
    }
}

/// New video published
#[utoipa::path(
        post,
        path = "/",
        request_body(content = Feed, description = "Google PubSubHubbub XML request", content_type = "application/atom+xml"),
        responses(
            (status = 200, body = Feed),
            (status = 400, description = "Bad request, possible malformed XML or X-Hub-Signature header is missing."),            
        ),        
    )]
#[axum::debug_handler]
async fn new_video_published(headers: HeaderMap, body: String) -> Result<Json<Feed>, ApiError> {
    println!("New YouTube video published");
    let xml: Feed = quick_xml::de::from_str(&body)?;

    let signature = headers.get("X-Hub-Signature");
    println!("signature: {:?}", signature);

    Ok(Json(xml))
}

#[derive(Debug, Deserialize, ToSchema)]
pub enum VerificationMode {
    #[serde(rename = "subscribe")]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe
}

#[derive(Debug, Deserialize, ToSchema)]
struct Verification {
    #[serde(rename = "hub.mode")]
    pub mode: VerificationMode,
    #[serde(rename = "hub.topic")]
    pub topic: String,
    #[serde(rename = "hub.challenge")]
    pub challenge: String,
    #[serde(rename = "hub.lease_seconds")]
    pub lease_seconds: u64,
}

/// Hub verification request
#[utoipa::path(
        get,
        path = "/",
        description = "Google PubSubHubbub subscription verification",
        params(
            ("hub.mode" = VerificationMode, Query, description = "The literal string \"subscribe\" or \"unsubscribe\", which matches the original request to the hub from the subscriber.", example = "subscribe"),
            ("hub.topic" = String, Query, description = "The topic URL given in the corresponding subscription request.", example = "https://www.youtube.com/xml/feeds/videos.xml?channel_id=UCBR8-60-B28hp2BmDPdntcQ"),
            ("hub.challenge" = String, Query, description = "A hub-generated, random string that MUST be echoed by the subscriber to verify the subscription.", example = "14828210609622910347"),
            ("hub.lease_seconds" = Option<u64>, Query, description = "The hub-determined number of seconds that the subscription will stay active before expiring, measured from the time the verification request was made from the hub to the subscriber. This parameter MAY be present for unsubscribe requests and MUST be ignored by subscribers during unsubscription.", example = 432000)
        ),
        responses(
            (status = 200, description = "The challenge string.", body = String),
            (status = 400, description = "Missing required query arguments."),            
        ),        
    )]
#[axum::debug_handler]
async fn subscription_callback(Query(verification): Query<Verification>) -> Result<String, ApiError> {
    println!("New YouTube video verification request received: {:?}", &verification);
    

    Ok(verification.challenge)
}
