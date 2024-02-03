use std::{sync::Arc, time::Duration};

use crate::{controller::Context, errors::Error, utils::Result};

use kube::{
    api::{Api, Patch, PatchParams, ResourceExt},
    client::Client,
    runtime::controller::Action,
    CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

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

impl Pokemon {
    // Reconcile (for non-finalizer related changes)
    pub async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let client: Client = ctx.client.clone();

        let ns = self.namespace().unwrap();
        let name = self.name_any();
        let docs: Api<Pokemon> = Api::namespaced(client, &ns);

        let should_alive = self.spec.health > 0;
        if !self.is_alive() && should_alive {
            println!("Pokemon \"{}\" is alive", name);

            // ==============================
            // This is where you would send events to kubernetes
            // ==============================
        }

        // ==============================
        // Do the actual work here to apply changes to the kubernetes
        // ==============================
        // always overwrite status object with what we saw
        let new_status = Patch::Apply(json!({
            "apiVersion": "pokemon.rs/v1",
            "kind": "Pokemon",
            "status": PokemonStatus {
                alive: should_alive,
            }
        }));

        let ps: PatchParams = PatchParams::apply("cntrlr").force();
        let _o = docs
            .patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)?;

        // If no events were received, check back every 5 minutes
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    // Finalizer cleanup (the object was deleted, ensure nothing is orphaned)
    pub async fn cleanup(&self, _ctx: Arc<Context>) -> Result<Action> {
        // ==============================
        // Publish event about cleanup
        // Do any cleanup if needed
        // ==============================

        Ok(Action::await_change())
    }
}
