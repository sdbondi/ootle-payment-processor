// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::api::context::HandlerContext;
use crate::api::handlers;
use crate::startup::App;
use axum::routing::post;
use axum::{routing::get, Extension, Router};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

const REQUEST_BODY_LIMIT: usize = 4 * 1024 * 1024; // 4 MB

#[derive(OpenApi)]
#[openapi(paths(
    handlers::misc::version,
    handlers::misc::health,
    handlers::payment::create,
    handlers::payment::get,
    handlers::balances::list,
))]
pub struct ApiDoc;

pub fn create_router(app: App) -> Router {
    Router::new()
        .route("/version", get(handlers::misc::version))
        .route("/health", get(handlers::misc::health))
        .nest(
            "/payments",
            Router::new()
                .route("/", post(handlers::payment::create))
                .route("/{payment_id}", get(handlers::payment::get)),
        )
        .route("/balances", get(handlers::balances::list))
        .layer(Extension(HandlerContext::new(app)))
        .layer(CorsLayer::permissive())
        .layer(RequestBodyLimitLayer::new(REQUEST_BODY_LIMIT))
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
}
