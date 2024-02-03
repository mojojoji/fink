#![allow(unused_imports)]

use crate::{controller::Context, errors::Error, utils::Result};
use std::{sync::Arc, time::Duration};

use k8s_openapi::api::core::v1::{
    Container, Pod, PodSpec, PodStatus, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::error::ErrorResponse;
use kube::{
    api::{Api, Patch, PatchParams, PostParams, ResourceExt},
    client::Client,
    core::ObjectMeta,
    runtime::controller::Action,
    CustomResource, Resource,
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
    namespaced,
    doc = "A VirtualMachine resource for FinK",
    singular = "virtualmachine",
    plural = "virtualmachines",
    shortname = "vm",
    status = "VirtualMachineStatus",
    printcolumn = r#"{"name":"Image", "type":"string", "description":"VM rootfs image", "jsonPath":".spec.image"}"#
)]
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
        // let vms: Api<VirtualMachine> = Api::namespaced(client, &ns);

        match self.spec.state {
            VirtualMachineDesiredState::STOPPED => self.stop(ctx).await?,
            VirtualMachineDesiredState::STARTED => self.start(ctx).await?,
            VirtualMachineDesiredState::HIBERNATED => self.hibernate(ctx).await?,
        }

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

    async fn update_status(&self, ctx: Arc<Context>, status: VirtualMachineStatus) -> Result<()> {
        let ns = self.namespace().unwrap();
        let vm_name = self.metadata.name.as_ref().unwrap();

        let vms: Api<VirtualMachine> = Api::namespaced(ctx.client.clone(), &ns);
        let patch = Patch::Merge(status);
        let _o = vms
            .patch_status(vm_name, &PatchParams::default(), &patch)
            .await
            .map_err(Error::KubeError)?;
        Ok(())
    }

    async fn start(&self, ctx: Arc<Context>) -> Result<()> {
        let client: Client = ctx.client.clone();
        let ns = self.namespace().unwrap();

        let owner_reference = self.controller_owner_ref(&()).unwrap();

        let vm_name = self.metadata.name.as_ref().unwrap();
        let image = self.spec.image.clone();
        let vm_label_key = "vms.codesandbox.io/name".to_string();

        let mut labels = self.metadata.labels.clone().unwrap_or_default();
        labels.insert(vm_label_key, vm_name.to_string());

        // Create a pod in the ns
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some(vm_name.to_string()),
                owner_references: Some(vec![owner_reference.clone()]),
                labels: Some(labels.clone()),
                ..ObjectMeta::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "vm-container".to_string(),
                    image: Some(image),
                    ..Container::default()
                }],
                ..PodSpec::default()
            }),
            ..Pod::default()
        };

        let service = Service {
            metadata: ObjectMeta {
                name: Some(vm_name.to_string()),
                owner_references: Some(vec![owner_reference]),
                labels: Some(labels.clone()),
                ..ObjectMeta::default()
            },
            spec: Some(k8s_openapi::api::core::v1::ServiceSpec {
                selector: Some(labels.clone()),
                ports: Some(vec![k8s_openapi::api::core::v1::ServicePort {
                    protocol: Some("TCP".to_string()),
                    port: 80,
                    target_port: Some(IntOrString::Int(80)),
                    ..ServicePort::default()
                }]),
                ..ServiceSpec::default()
            }),
            ..Service::default()
        };

        let services: Api<Service> = Api::namespaced(client.clone(), &ns);
        let existing_service = services.get(vm_name).await;
        if let Err(kube::Error::Api(ErrorResponse { code, .. })) = existing_service {
            if code == 404 {
                let _o = services
                    .create(&PostParams::default(), &service)
                    .await
                    .map_err(Error::KubeError)?;
            }
        }

        let pods: Api<Pod> = Api::namespaced(client.clone(), &ns);
        let existing_pod = pods.get(vm_name).await;
        if let Err(kube::Error::Api(ErrorResponse { code, .. })) = existing_pod {
            if code == 404 {
                let _o = pods
                    .create(&PostParams::default(), &pod)
                    .await
                    .map_err(Error::KubeError)?;
            }
        }

        if let Ok(Pod {
            status:
                Some(PodStatus {
                    container_statuses: Some(container_statuses),
                    ..
                }),
            ..
        }) = existing_pod
        {
            let all_started = container_statuses
                .into_iter()
                .all(|cs| cs.started.unwrap_or(false));

            if all_started {
                self.update_status(
                    ctx.clone(),
                    VirtualMachineStatus {
                        state: VirtualMachineCurrentState::STARTED,
                    },
                )
                .await?;
            }
        } else {
            self.update_status(
                ctx.clone(),
                VirtualMachineStatus {
                    state: VirtualMachineCurrentState::STARTING,
                },
            )
            .await?;
        }

        Ok(())
    }

    async fn stop(&self, ctx: Arc<Context>) -> Result<()> {
        let client: Client = ctx.client.clone();

        let ns = self.namespace().unwrap();
        let name = self.name_any();
        info!("Creating VirtualMachine {} in {}", name, ns);
        let vm_name = self.metadata.name.as_ref().unwrap();

        let pods: Api<Pod> = Api::namespaced(client.clone(), &ns);
        let existing_pod = pods.get(vm_name).await;
        if existing_pod.is_ok() {
            let _o = pods
                .delete(vm_name, &Default::default())
                .await
                .map_err(Error::KubeError)?;
        }

        let services: Api<Service> = Api::namespaced(client, &ns);
        let existing_service = services.get(vm_name).await;
        if existing_service.is_ok() {
            let _o = services
                .delete(vm_name, &Default::default())
                .await
                .map_err(Error::KubeError)?;
        }

        info!("Stopping VirtualMachine {}", self.name_any());
        Ok(())
    }
    async fn hibernate(&self, _ctx: Arc<Context>) -> Result<()> {
        info!("Hibernating VirtualMachine {}", self.name_any());
        Ok(())
    }
}
