pub mod virtualmachine;

use crate::{
    controller::virtualmachine::VIRTUAL_MACHINE_FINALIZER, errors::Error, state::AppState,
    utils::Result,
};
use futures::StreamExt;
use k8s_openapi::api::core::v1::{Pod, Service};
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

use self::virtualmachine::VirtualMachine;

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
}

async fn reconcile(vm: Arc<VirtualMachine>, ctx: Arc<Context>) -> Result<Action> {
    let ns = vm.namespace().unwrap(); // doc is namespace scoped
    let vms: Api<VirtualMachine> = Api::namespaced(ctx.client.clone(), &ns);

    info!("Reconciling \"{}\" in {}", vm.name_any(), ns);
    finalizer(&vms, VIRTUAL_MACHINE_FINALIZER, vm, |event| async {
        match event {
            Finalizer::Apply(vm) => vm.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(vm) => vm.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}
fn error_policy(_vm: Arc<VirtualMachine>, error: &Error, _ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    Action::requeue(Duration::from_secs(5 * 60))
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(state: AppState) {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");
    let vms = Api::<VirtualMachine>::all(client.clone());
    let pods = Api::<Pod>::all(client.clone());
    let services = Api::<Service>::all(client.clone());

    if let Err(e) = vms.list(&ListParams::default().limit(1)).await {
        error!("CRD is not queryable; {e:?}. Is the CRD installed?");
        info!("Installation: cargo run --bin crdgen | kubectl apply -f -");
        std::process::exit(1);
    }
    Controller::new(vms, Config::default().any_semantic())
        .owns(pods, Config::default().any_semantic())
        .owns(services, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, state.to_context(client))
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}
