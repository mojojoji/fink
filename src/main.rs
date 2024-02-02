use std::future::IntoFuture;

use axum::{response::Html, routing::get, Json, Router};

use controller::State;
use serde_json::{json, Value};

#[tokio::main]
async fn main() {
    let state = State::default();

    // build our application with a route
    let app: Router = Router::new()
        .route("/metrics", get(metrics))
        .route("/health", get(health));

    // run it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());

    let server = axum::serve(listener, app).into_future();

    let controller_run = controller::run(state.clone());

    tokio::select! {
        _ = server => println!("Axum server stopped"),
        _ = controller_run => println!("Controller stopped"),
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "healthy": true}))
}

async fn metrics() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}
