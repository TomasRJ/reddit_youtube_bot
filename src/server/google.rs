use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac, digest::crypto_common};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    infrastructure::AppState,
    server::{ApiError, repository::get_subscription_details},
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(new_video_published))
        .routes(routes!(subscription_callback))
}

impl From<quick_xml::DeError> for ApiError {
    fn from(error: quick_xml::DeError) -> Self {
        ApiError::BadRequest(format!("Invalid XML request: {:?}", error))
    }
}

impl From<crypto_common::InvalidLength> for ApiError {
    fn from(error: crypto_common::InvalidLength) -> Self {
        ApiError::InternalError(format!("Error handling HMAC secret: {:?}", error))
    }
}

impl From<axum::http::header::ToStrError> for ApiError {
    fn from(error: axum::http::header::ToStrError) -> Self {
        ApiError::InternalError(format!(
            "The X-Hub-Signature header is not a valid string: {:?}",
            error
        ))
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Feed {
    #[serde(rename = "link")]
    pub links: Vec<Link>,
    pub title: String,
    pub updated: DateTime<Utc>,
    pub entry: Entry,
}

type HmacSha1 = Hmac<sha1::Sha1>;

impl Feed {
    fn validate(hmac_secret: &String, headers: HeaderMap, body: String) -> Result<Feed, ApiError> {
        match headers.get("X-Hub-Signature") {
            Some(signature) => {
                let signature = if let Some(("sha1", hash)) = signature.to_str()?.split_once('=') {
                    hash
                } else {
                    return Err(ApiError::BadRequest(format!(
                        "Invalid X-Hub-Signature header format: {:?}, expected sha1=signature",
                        signature
                    )));
                };

                if signature.len() != 40 {
                    return Err(ApiError::BadRequest(format!(
                        "Invalid SHA1 signature: {}, length in bytes: {}",
                        signature,
                        signature.len()
                    )));
                }

                let mut hasher = HmacSha1::new_from_slice(hmac_secret.as_bytes())?;
                hasher.update(body.as_bytes());
                let hash = hasher.finalize();

                let hash_string = format!("{:x}", hash.into_bytes()); // format the bytes to a lowercase hex string

                if signature.ne(&hash_string) {
                    return Err(ApiError::BadRequest(
                        "The signature in the header does not match the calculated signature"
                            .to_string(),
                    ));
                }

                let feed: Feed = quick_xml::de::from_str(&body)?;

                Ok(feed)
            }
            None => Err(ApiError::BadRequest(
                "The new video request has no X-Hub-Signature header.".to_string(),
            )),
        }
    }
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

/// New video published
#[utoipa::path(
        post,
        path = "/subscription/{id}",
        request_body(content = Feed, description = "Google PubSubHubbub XML request", content_type = "application/atom+xml"),
        params(
            ("X-Hub-Signature" = String, Header, description = "Google PubSubHubbub HMAC signature for the request body in the form of \"sha1=signature\" where the signature is a 40-byte, hexadecimal representation of a SHA1 signature. Source https://pubsubhubbub.github.io/PubSubHubbub/pubsubhubbub-core-0.4.html#rfc.section.8", example = "sha1=e7667dbb6b9dc356ac8dd767560926d5403be497"),
            ("id" = String, Path, description = "Subscription id", example = "019ba504-70f5-7f35-9c2c-2f02b992af7e")
        ),
        responses(
            (status = 200, description = "Successful request."),
            (status = 400, description = "Bad request, possible malformed XML or X-Hub-Signature header."),            
            (status = 404, description = "Subscription doesn't exists."),
        ),
    )]
#[axum::debug_handler]
async fn new_video_published(
    State(state): State<Arc<AppState>>,
    Path(subscription_id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<(), ApiError> {
    let subscription = get_subscription_details(&state.db_pool, &subscription_id).await?;
    let feed = Feed::validate(&subscription.hmac_secret, headers, body)?;

    // Shorts are only posted when the user has explicitly set post_shorts to true.
    if feed.entry.link.href.contains("shorts") && !subscription.post_shorts {
        return Ok(());
    }

    Ok(())
}

#[derive(Debug, Deserialize, ToSchema)]
pub enum VerificationMode {
    #[serde(rename = "subscribe")]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
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
async fn subscription_callback(
    Query(verification): Query<Verification>,
) -> Result<String, ApiError> {
    println!(
        "New YouTube video verification request received: {:?}",
        &verification
    );

    Ok(verification.challenge)
}
