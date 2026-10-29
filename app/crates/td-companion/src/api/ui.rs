//! Static UI files share the API request guard and response headers.
use super::CommonError;
use axum::{
    extract::Request,
    http::Method,
    response::{IntoResponse, Response},
};

include!(concat!(env!("OUT_DIR"), "/embedded_ui.rs"));

pub(super) fn is_api(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

pub(super) async fn serve(request: Request) -> Response {
    if is_api(request.uri().path()) {
        return CommonError::NotFound.into_response();
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return CommonError::MethodNotAllowed.into_response();
    }
    let (_, media_type, immutable, bytes) = UI_FILES
        .iter()
        .find(|(path, ..)| *path == request.uri().path())
        .or_else(|| UI_FILES.iter().find(|(path, ..)| *path == "/index.html"))
        .expect("embedded UI always has index.html");
    let mut response = (
        [
            ("content-type", *media_type),
            (
                "cache-control",
                if *immutable {
                    "public, max-age=31536000, immutable"
                } else {
                    "no-store"
                },
            ),
            ("content-length", &bytes.len().to_string()),
        ],
        *bytes,
    )
        .into_response();
    if request.method() == Method::HEAD {
        *response.body_mut() = axum::body::Body::empty();
    }
    response
}
