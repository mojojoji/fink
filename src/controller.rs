use k8s_openapi::{
    api::apps::v1::Deployment,
    apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
};
use kube::{
    api::ListParams, runtime::Controller, Api, Client, Config, CustomResource, CustomResourceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::Sleep;
use tracing::*;

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
}

#[derive(CustomResource, Debug, Serialize, Deserialize, Default, Clone, JsonSchema)]
#[kube(group = "pokemon.rs", version = "v1", kind = "Pokemon", namespaced)]
pub struct PokemonSpec {
    name: String,
    health: u16,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct PokemonStatus {
    pub alive: bool,
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run() {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");
    let docs = Api::<Pokemon>::all(client.clone());
    if let Err(e) = docs.list(&ListParams::default().limit(1)).await {
        error!("CRD is not queryable; {e:?}. Is the CRD installed?");
        info!("Installation: cargo run --bin crdgen | kubectl apply -f -");
        std::process::exit(1);
    }

    println!("Starting controller");
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    println!("Done controller");

    // TODO: This is where we would add our reconciler logic

    // Controller::new(docs, Config::default().any_semantic())
    //     .shutdown_on_signal()
    //     .run(reconcile, error_policy, state.to_context(client))
    //     .filter_map(|x| async move { std::result::Result::ok(x) })
    //     .for_each(|_| futures::future::ready(()))
    //     .await;
}
