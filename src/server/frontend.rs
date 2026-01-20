use std::sync::Arc;

use axum::{extract::State, response::Html};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{infrastructure::AppState, server::ApiError};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(main_landing_page))
}

impl From<handlebars::RenderError> for ApiError {
    fn from(error: handlebars::RenderError) -> Self {
        ApiError::InternalError(format!(
            "Error when rendering data on HTML template: {:?}",
            error
        ))
    }
}

impl From<handlebars::TemplateError> for ApiError {
    fn from(error: handlebars::TemplateError) -> Self {
        ApiError::InternalError(format!("Error on parsing HTML template: {:?}", error))
    }
}

/// Main landing page
#[utoipa::path(
        get,
        path = "/",
        description = "Main landing page",
        responses(
            (status = 200, description = "Main landing page html.", content_type = "text/html; charset=utf-8")
        ),
        tag = "frontend"
    )]
#[axum::debug_handler]
async fn main_landing_page(State(state): State<Arc<AppState>>) -> Result<Html<String>, ApiError> {
    let mut local_hb = state.hb.clone();

    local_hb.register_template_file("body_content", "src/frontend_templates/landing_page.html")?;

    let whole_document = local_hb.render("whole_document", &{})?;

    Ok(Html(whole_document))
}
