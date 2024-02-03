#![allow(unused_imports)]

use crate::{controller::Context, errors::Error, utils::Result};
use std::{sync::Arc, time::Duration};

use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{Api, Patch, PatchParams, PostParams, ResourceExt},
    client::Client,
    runtime::controller::Action,
    CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::*;

pub static VIRTUAL_MACHINE_FINALIZER: &str = "vm.codesandbox.io";

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
pub enum VirtualMachineDesiredState {
    #[default]
    STOPPED,
    STARTED,
    HIBERNATED,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
pub enum VirtualMachineCurrentState {
    #[default]
    STOPPED,
    STOPPING,
    STARTED,
    STARTING,
    HIBERNATING,
    HIBERNATED,
}

#[derive(CustomResource, Debug, Serialize, Deserialize, Default, Clone, JsonSchema)]
#[kube(
    group = "codesandbox.io",
    version = "v1alpha1",
    kind = "VirtualMachine",
    namespaced
)]
#[kube(status = "VirtualMachineStatus", shortname = "poke")]
pub struct VirtualMachineSpec {
    pub image: String,
    pub state: VirtualMachineDesiredState,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct VirtualMachineStatus {
    pub state: VirtualMachineCurrentState,
}

impl VirtualMachine {
    // Reconcile (for non-finalizer related changes)
    pub async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let client: Client = ctx.client.clone();

        let ns = self.namespace().unwrap();
        let name = self.name_any();
        // let vms: Api<VirtualMachine> = Api::namespaced(client, &ns);

        info!("Reconciling VirtualMachine {} in {}", name, ns);

        // Create a pod in the ns
        let pod = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "name": self.metadata.name,
            },
            "spec": {
                "containers": [{
                    "name": self.metadata.name,
                    "image": self.spec.image,
                }],
            },
        }))
        .unwrap();

        let pods: Api<Pod> = Api::namespaced(client, &ns);
        let _o = pods
            .create(&PostParams::default(), &pod)
            .await
            .map_err(Error::KubeError)?;

        // let should_alive = self.spec.health > 0;
        // if !self.is_alive() && should_alive {
        //     println!("Pokemon \"{}\" is alive", name);

        //     // ==============================
        //     // This is where you would send events to kubernetes
        //     // ==============================
        // }

        // ==============================
        // Do the actual work here to apply changes to the kubernetes
        // ==============================
        // always overwrite status object with what we saw
        // let new_status = Patch::Apply(json!({
        //     "apiVersion": "pokemon.rs/v1",
        //     "kind": "Pokemon",
        //     "status": PokemonStatus {
        //         alive: should_alive,
        //     }
        // }));

        // let ps: PatchParams = PatchParams::apply("cntrlr").force();
        // let _o = docs
        //     .patch_status(&name, &ps, &new_status)
        //     .await
        //     .map_err(Error::KubeError)?;

        // If no events were received, check back every 5 minutes
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    // Finalizer cleanup (the object was deleted, ensure nothing is orphaned)
    pub async fn cleanup(&self, _ctx: Arc<Context>) -> Result<Action> {
        info!("Cleaning up VirtualMachine");

        // ==============================
        // Publish event about cleanup
        // Do any cleanup if needed
        // ==============================

        Ok(Action::await_change())
    }
}
