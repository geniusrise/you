use axum::response::IntoResponse;
use axum::routing::post;
use axum::Json;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn client_retries_on_5xx_and_parses() {
    let attempts = std::sync::Arc::new(AtomicUsize::new(0));
    let attempts2 = attempts.clone();

    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<serde_json::Value>| async move {
            let n = attempts2.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            assert_eq!(body["model"], "test-model");
            assert_eq!(body["response_format"]["type"], "json_object");
            let user = body["messages"][1]["content"].as_str().unwrap().to_string();
            Json(serde_json::json!({
                "choices": [{"message": {"content": format!("{{\"echo\": {user:?}}}")}}]
            }))
            .into_response()
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = aiyou::profile::client::LlmClient::new(
        format!("http://{addr}/v1"),
        "test-key".into(),
        "test-model".into(),
    );
    let out = tokio::task::spawn_blocking(move || client.chat_blocking("sys", "hello"))
        .await
        .unwrap()
        .expect("chat should succeed after retry");
    assert!(out.contains("hello"));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn client_fails_after_retries() {
    let attempts = std::sync::Arc::new(AtomicUsize::new(0));
    let attempts2 = attempts.clone();
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(move || async move {
            attempts2.fetch_add(1, Ordering::SeqCst);
            axum::http::StatusCode::BAD_GATEWAY.into_response()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = aiyou::profile::client::LlmClient::new(
        format!("http://{addr}/v1"),
        "k".into(),
        "m".into(),
    );
    let r = tokio::task::spawn_blocking(move || client.chat_blocking("s", "u")).await.unwrap();
    assert!(r.is_err());
    // 1 initial + 2 retries
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}
