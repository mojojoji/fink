use std::sync::Arc;

use kube::Client;

use crate::controller::Context;

#[derive(Clone, Default)]
pub struct AppState {}

impl AppState {
    // Create a Controller Context that can update State
    pub fn to_context(&self, client: Client) -> Arc<Context> {
        Arc::new(Context { client })
    }
}
