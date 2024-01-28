use kube::CustomResourceExt;
fn main() {
    print!(
        "{}",
        serde_yaml::to_string(&controller::Pokemon::crd()).unwrap()
    )
}
