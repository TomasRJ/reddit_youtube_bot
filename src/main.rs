use std::net::Ipv4Addr;

use axum::{
    Json,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::net::TcpListener;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_rapidoc::RapiDoc;

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

#[derive(OpenApi)]
#[openapi(
    paths(),
    components(schemas()),
    servers((url = "", description = "Reddit YouTube bot")),
)]
pub struct ApiDoc;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Axum server start error: {0}")]
    AxumError(#[from] std::io::Error),

    #[error("Internal server error 1: {0}")]
    InternalError(String),

    #[error("Not found error: {0}")]
    NotFound(String),

    #[error("Bad request error: {0}")]
    BadRequest(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            ApiError::AxumError(error) => {
                println!("Axum error: {}", error);
                (StatusCode::BAD_REQUEST, format!("Server error: {}", error))
            }
            ApiError::InternalError(message) => {
                println!("Internal server error 2: {}", message);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Internal server error 3: {}", message),
                )
            }
            ApiError::NotFound(message) => {
                println!("Not found error: {}", message);
                (
                    StatusCode::NOT_FOUND,
                    format!("Not found error: {}", message),
                )
            }
            ApiError::BadRequest(message) => {
                println!("Bad request error: {}", message);
                (
                    StatusCode::BAD_REQUEST,
                    format!("Bad request error: {}", message),
                )
            }
        };
        (status, message).into_response()
    }
}

impl From<quick_xml::DeError> for ApiError {
    fn from(error: quick_xml::DeError) -> Self {
        ApiError::BadRequest(format!("Invalid XML request: {:?}", error))
    }
}

#[utoipa::path(
        post,
        path = "/google",
        responses(
            (status = 200, body = Feed),
        ),
        request_body(content = Feed, description = "Google PubSubHubbub XML request", content_type = "application/atom+xml"),
    )]
#[axum::debug_handler]
async fn get_user(headers: HeaderMap, body: String) -> Result<Json<Feed>, ApiError> {
    let xml: Feed = quick_xml::de::from_str(&body)?;

    let signature = headers.get("X-Hub-Signature");
    println!("signature: {:?}", signature);

    Ok(Json(xml))
}

const APP_NAME: &str = env!("CARGO_PKG_NAME");

#[tokio::main()]
async fn main() -> Result<(), ApiError> {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(get_user))
        .split_for_parts();

    let router =
        router.merge(RapiDoc::with_openapi("/api-docs/openapi.json", api).path("/rapidoc"));

    let addr = format!("{}:{}", Ipv4Addr::LOCALHOST, 8080);

    let listener = TcpListener::bind(&addr).await?;

    println!("Serving {} on: http://{}", APP_NAME, addr);
    println!("\t - API docs on: http://{}/rapidoc", addr);

    Ok(axum::serve(listener, router).await?)
}
