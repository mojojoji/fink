use crate::{telemetry, Error, Metrics, Result};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        events::{Event, EventType, Recorder, Reporter},
        finalizer::{finalizer, Event as Finalizer},
        watcher::Config,
    },
    CustomResource, Resource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::{sync::RwLock, time::Duration};
use tracing::*;

pub static POKEMON_FINALIZER: &str = "pokemon.pokemon.rs";

#[derive(CustomResource, Debug, Serialize, Deserialize, Default, Clone, JsonSchema)]
#[kube(group = "pokemon.rs", version = "v1", kind = "Pokemon", namespaced)]
#[kube(status = "PokemonStatus", shortname = "poke")]
pub struct PokemonSpec {
    pub name: String,
    pub health: u16,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct PokemonStatus {
    pub alive: bool,
}

impl Pokemon {
    fn is_alive(&self) -> bool {
        self.status.as_ref().map(|s| s.alive).unwrap_or_default()
    }
}

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
    /// Diagnostics read by the web server
    pub diagnostics: Arc<RwLock<Diagnostics>>,
    /// Prometheus metrics
    pub metrics: Metrics,
}

#[instrument(skip(ctx, pokemon), fields(trace_id))]
async fn reconcile(pokemon: Arc<Pokemon>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let _timer = ctx.metrics.count_and_measure();
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
fn error_policy(pokemon: Arc<Pokemon>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.metrics.reconcile_failure(&pokemon, error);
    Action::requeue(Duration::from_secs(5 * 60))
}

impl Pokemon {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        println!("Reconciling Pokemon \"{}\"", self.name_any());
        let client: Client = ctx.client.clone();
        let recorder = ctx.diagnostics.read().await.recorder(client.clone(), self);
        let ns = self.namespace().unwrap();
        let name = self.name_any();
        let docs: Api<Pokemon> = Api::namespaced(client, &ns);

        let should_alive = self.spec.health > 0;
        if !self.is_alive() && should_alive {
            // send an event once per hide
            recorder
                .publish(Event {
                    type_: EventType::Normal,
                    reason: "Alive requested".into(),
                    note: Some(format!("Aliving `{name}`")),
                    action: "Aliving".into(),
                    secondary: None,
                })
                .await
                .map_err(Error::KubeError)?;
        }

        // always overwrite status object with what we saw
        let new_status = Patch::Apply(json!({
            "apiVersion": "pokemon.rs/v1",
            "kind": "Pokemon",
            "status": PokemonStatus {
                alive: should_alive,
            }
        }));
        let ps = PatchParams::apply("cntrlr").force();
        let _o = docs
            .patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)?;

        // If no events were received, check back every 5 minutes
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    // Finalizer cleanup (the object was deleted, ensure nothing is orphaned)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        println!("Cleaning up Pokemon \"{}\"", self.name_any());
        let recorder = ctx
            .diagnostics
            .read()
            .await
            .recorder(ctx.client.clone(), self);
        // Document doesn't have any real cleanup, so we just publish an event
        recorder
            .publish(Event {
                type_: EventType::Normal,
                reason: "DeleteRequested".into(),
                note: Some(format!("Delete `{}`", self.name_any())),
                action: "Deleting".into(),
                secondary: None,
            })
            .await
            .map_err(Error::KubeError)?;
        Ok(Action::await_change())
    }
}

/// Diagnostics to be exposed by the web server
#[derive(Clone, Serialize)]
pub struct Diagnostics {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_event: Utc::now(),
            reporter: "doc-controller".into(),
        }
    }
}
impl Diagnostics {
    fn recorder(&self, client: Client, pokemon: &Pokemon) -> Recorder {
        Recorder::new(client, self.reporter.clone(), pokemon.object_ref(&()))
    }
}

/// State shared between the controller and the web server
#[derive(Clone, Default)]
pub struct State {
    /// Diagnostics populated by the reconciler
    diagnostics: Arc<RwLock<Diagnostics>>,
    /// Metrics registry
    registry: prometheus::Registry,
}

/// State wrapper around the controller outputs for the web server
impl State {
    /// Metrics getter
    pub fn metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }

    /// State getter
    pub async fn diagnostics(&self) -> Diagnostics {
        self.diagnostics.read().await.clone()
    }

    // Create a Controller Context that can update State
    pub fn to_context(&self, client: Client) -> Arc<Context> {
        Arc::new(Context {
            client,
            metrics: Metrics::default().register(&self.registry).unwrap(),
            diagnostics: self.diagnostics.clone(),
        })
    }
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(state: State) {
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
