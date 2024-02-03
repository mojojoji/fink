pub mod controller;
pub mod errors;
pub mod state;
pub mod utils;

use std::future::IntoFuture;

use axum::{routing::get, Json, Router};

use serde_json::{json, Value};

#[tokio::main]
async fn main() {
    use tracing_subscriber::FmtSubscriber;

    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .pretty()
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let state = state::AppState::default();

    let app: Router = Router::new().route("/health", get(health));

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
