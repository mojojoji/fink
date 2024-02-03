pub mod pokemon;

use pokemon::{Pokemon, POKEMON_FINALIZER};

use crate::{errors::Error, state::AppState, utils::Result};
use futures::StreamExt;
use kube::{
    api::{Api, ListParams, ResourceExt},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        finalizer::{finalizer, Event as Finalizer},
        watcher::Config,
    },
};
use std::sync::Arc;
use tokio::time::Duration;
use tracing::*;

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
}

async fn reconcile(pokemon: Arc<Pokemon>, ctx: Arc<Context>) -> Result<Action> {
    let ns = pokemon.namespace().unwrap(); // doc is namespace scoped
    let pokemons: Api<Pokemon> = Api::namespaced(ctx.client.clone(), &ns);

    info!("Reconciling Pokemon \"{}\" in {}", pokemon.name_any(), ns);
    finalizer(&pokemons, POKEMON_FINALIZER, pokemon, |event| async {
        match event {
            Finalizer::Apply(pokemon) => pokemon.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(pokemon) => pokemon.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}
fn error_policy(_pokemon: Arc<Pokemon>, error: &Error, _ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    Action::requeue(Duration::from_secs(5 * 60))
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(state: AppState) {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");
    let pokemons = Api::<Pokemon>::all(client.clone());

    if let Err(e) = pokemons.list(&ListParams::default().limit(1)).await {
        error!("CRD is not queryable; {e:?}. Is the CRD installed?");
        info!("Installation: cargo run --bin crdgen | kubectl apply -f -");
        std::process::exit(1);
    }
    Controller::new(pokemons, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, state.to_context(client))
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}
