//! Wire types generated from the TypeScript contracts (`packages/contracts/schema/rpc.schema.json`).

#[allow(clippy::all, dead_code, unused_imports, unused_qualifications)]
#[rustfmt::skip]
pub mod generated;

pub use generated::{
    OrchestrationSubscribeShellPayload as SubscribeShellPayload,
    OrchestrationSubscribeShellSuccess as ShellItem,
    OrchestrationSubscribeThreadPayload as SubscribeThreadPayload, ProjectsEntry as Project,
    ProjectsMutatePayload as ProjectMutation,
    ProjectsMutatePayloadVariant1Type as ProjectMutationType, ProjectsMutateSuccess,
    ThreadsEntry as Thread,
};
