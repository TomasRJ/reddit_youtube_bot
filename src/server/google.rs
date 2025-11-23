use axum::{Json, http::HeaderMap};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::server::ApiError;

pub fn router() -> OpenApiRouter {
    OpenApiRouter::new().routes(routes!(new_video_published))
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
