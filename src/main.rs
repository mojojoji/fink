use axum::{response::Html, routing::get, Json, Router};

use serde_json::{json, Value};
use tokio::join;

#[tokio::main]
async fn main() {
    let controller_run = tokio::spawn(async {
        controller::run().await;
    });

    // build our application with a route
    let app: Router = Router::new()
        .route("/metrics", get(metrics))
        .route("/health", get(health));

    // run it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();

    join!(controller_run).0.unwrap();
}

async fn health() -> Json<Value> {
    Json(json!({ "healthy": true}))
}

async fn metrics() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}
