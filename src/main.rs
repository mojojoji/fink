use std::future::IntoFuture;

use axum::{extract::State, response::Html, routing::get, Json, Router};

use controller::AppState;
use prometheus::{Encoder, TextEncoder};
use serde_json::{json, Value};

#[tokio::main]
async fn main() {
    let state = AppState::default();

    // build our application with a route
    let app: Router = Router::new()
        .route("/metrics", get(metrics))
        .route("/health", get(health))
        .with_state(state.clone());

    // run it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());

    let server = axum::serve(listener, app).into_future();

    let controller_run = controller::run(state);

    tokio::select! {
        _ = server => println!("Axum server stopped"),
        _ = controller_run => println!("Controller stopped"),
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "healthy": true}))
}

async fn metrics(State(state): State<AppState>) -> Html<String> {
    let metrics = state.metrics();
    let encoder = TextEncoder::new();
    let mut buffer = vec![];
    encoder.encode(&metrics, &mut buffer).unwrap();
    Html(String::from_utf8(buffer).unwrap())
}
