use td_companion::testing::TestCompanion;

include!(concat!(env!("OUT_DIR"), "/embedded_ui.rs"));

fn security_headers(response: &reqwest::Response, td: &TestCompanion) {
    for (key, value) in [
        ("x-frame-options", "DENY"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
    ] {
        assert_eq!(response.headers()[key], value);
    }
    let port = td.url.rsplit(':').next().unwrap();
    assert_eq!(
        response.headers()["content-security-policy"],
        format!(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' blob: data:; connect-src 'self' ws://127.0.0.1:{port}; \
         font-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; \
         frame-ancestors 'none'"
        )
    );
}

#[tokio::test]
async fn index_and_client_routes_work_without_a_session() {
    let td = TestCompanion::start().await;
    let index = UI_FILES
        .iter()
        .find(|(p, ..)| *p == "/index.html")
        .unwrap()
        .3;
    for path in [
        "/",
        "/index.html",
        "/devices/x",
        "/apiary",
        "/missing.js",
        "/?reload=1",
    ] {
        let response = td.raw().get(path).send().await.unwrap();
        assert_eq!(response.status(), 200, "{path}");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.headers()["content-type"],
            "text/html; charset=utf-8"
        );
        security_headers(&response, &td);
        assert_eq!(response.bytes().await.unwrap().as_ref(), index);
    }
    let response = td
        .raw()
        .request(reqwest::Method::HEAD, "/devices/x")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-length"],
        index.len().to_string()
    );
    assert!(response.bytes().await.unwrap().is_empty());
}

#[tokio::test]
async fn embedded_assets_have_their_bytes_media_type_and_cache_policy() {
    let td = TestCompanion::start().await;
    if UI_FILES.len() == 1 {
        assert!(
            std::str::from_utf8(UI_FILES[0].3)
                .unwrap()
                .contains("UI not built")
        );
    } else {
        assert!(
            UI_FILES
                .iter()
                .any(|(path, _, hashed, _)| path.ends_with(".js") && *hashed)
        );
        assert!(
            UI_FILES
                .iter()
                .any(|(path, _, hashed, _)| path.ends_with(".css") && *hashed)
        );
    }
    // Exercise every real build asset in CI, and the built-in page in source-only builds.
    for &(path, media_type, hashed, bytes) in UI_FILES {
        let response = td.raw().get(path).send().await.unwrap();
        assert_eq!(response.status(), 200, "{path}");
        assert_eq!(response.headers()["content-type"], media_type);
        assert_eq!(
            response.headers()["cache-control"],
            if hashed {
                "public, max-age=31536000, immutable"
            } else {
                "no-store"
            }
        );
        security_headers(&response, &td);
        assert_eq!(response.bytes().await.unwrap().as_ref(), bytes);
    }
}

#[tokio::test]
async fn static_requests_pass_the_security_guard() {
    let td = TestCompanion::start().await;
    for path in ["/", "/assets/index-12345678.js", "/devices/x"] {
        let response = td
            .raw()
            .get(path)
            .header("host", "evil.example:1")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 421);
        security_headers(&response, &td);
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["error"]["code"],
            "security.host"
        );
    }
    let response = td
        .raw()
        .get("/")
        .header("origin", "http://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let response = td
        .raw()
        .get("/")
        .header("sec-fetch-site", "cross-site")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let response = td
        .raw()
        .get("/devices/x")
        .header("sec-fetch-site", "cross-site")
        .header("sec-fetch-mode", "navigate")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn api_paths_never_fall_back_to_the_ui() {
    let td = TestCompanion::start().await;
    for path in ["/api", "/api/unknown", "/api/v1/unknown"] {
        let response = td.raw().get(path).send().await.unwrap();
        assert_eq!(response.status(), 401);
        let response = td
            .raw()
            .get(path)
            .bearer_auth(&td.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["error"]["code"],
            "route.not_found"
        );
    }
}

#[tokio::test]
async fn static_paths_reject_methods_that_could_change_state() {
    let td = TestCompanion::start().await;
    for method in [
        reqwest::Method::POST,
        reqwest::Method::PUT,
        reqwest::Method::DELETE,
    ] {
        let response = td.raw().request(method, "/devices/x").send().await.unwrap();
        assert_eq!(response.status(), 405);
        security_headers(&response, &td);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["error"]["code"],
            "route.method_not_allowed"
        );
    }
}
