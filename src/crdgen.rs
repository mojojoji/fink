pub mod controller;
pub mod errors;
pub mod state;
pub mod utils;

use kube::CustomResourceExt;

fn main() {
    print!(
        "{}",
        serde_yaml::to_string(&controller::pokemon::Pokemon::crd()).unwrap()
    )
}
