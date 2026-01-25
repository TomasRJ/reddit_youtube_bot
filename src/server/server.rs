use axum::response::IntoResponse;
use sqlx::migrate::MigrateError;
use thiserror::Error;

use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_rapidoc::RapiDoc;

use crate::{
    infrastructure::{AppState, Settings, handle_scheduler},
    server::{forms, frontend, google, reddit, shared},
};

impl From<MigrateError> for ApiError {
    fn from(error: MigrateError) -> Self {
        ApiError::InternalError(format!("SQL Migration failed: {:?}", error))
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(),
    components(schemas(
        shared::VerificationMode,
        reddit::RedditCallbackErrors
    )),
    servers((url = "", description = "Reddit YouTube bot")),
)]
pub struct ApiDoc;

const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub async fn serve(port: u16, app_settings: Settings) -> Result<(), ApiError> {
    let (state, scheduler_rx) = AppState::new(app_settings).await;

    sqlx::migrate!().run(&state.db_pool).await?;

    handle_scheduler(&state, scheduler_rx).await?;

    let (router, _api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(frontend::router())
        .nest("/google", google::router())
        .nest("/forms", forms::router())
        .nest("/reddit", reddit::router())
        .with_state(state)
        .split_for_parts();

    let router =
        router.merge(RapiDoc::with_openapi("/api-docs/openapi.json", _api).path("/rapidoc"));

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(ApiError::TcpListenerError)?;

    println!("Serving {} on: http://{}", APP_NAME, addr);
    println!("\t - API docs on: http://{}/rapidoc", addr);

    axum::serve(listener, router.into_make_service()).await?;
    Ok(())
}

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Axum server start error: {0}")]
    AxumError(#[from] std::io::Error),

    #[error("TCP listener bind error: {0}")]
    TcpListenerError(std::io::Error),

    #[error("Internal server error: {0}")]
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
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    format!("Server error: {}", error),
                )
            }
            ApiError::TcpListenerError(error) => {
                println!("TCP listener error: {}", error);
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Server error: {}", error),
                )
            }
            ApiError::InternalError(message) => {
                println!("Internal server error: {}", message);
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Internal server error: {}", message),
                )
            }
            ApiError::NotFound(message) => {
                println!("Not found error: {}", message);
                (
                    axum::http::StatusCode::NOT_FOUND,
                    format!("Not found error: {}", message),
                )
            }
            ApiError::BadRequest(message) => {
                println!("Bad request error: {}", message);
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    format!("Bad request error: {}", message),
                )
            }
        };
        (status, message).into_response()
    }
}
