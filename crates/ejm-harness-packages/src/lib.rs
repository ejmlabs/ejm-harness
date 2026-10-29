//! Trusted native extensions with explicit grants and shared, bounded language hosts.
//!
//! Node and Python packages execute arbitrary host code. Their grants control
//! registration in ejm-harness, not operating system containment.

mod adapters;
mod http;
mod manager;
mod registry;
mod runtime;
mod source;
mod transport;

pub use ejm_harness_package_spec::{HookEvent, RuntimeKind};
pub use manager::{PackageManager, PackageRecord, pack_directory, read_bounded};
pub use registry::{
    DeviceCode, PUBLIC_REGISTRY_ORIGIN, RegistryClient, VersionRecord, delete_registry_token,
    load_registry_token, migrate_registry_token, registry_origin_is_public, save_registry_token,
    validate_registry_origin,
};
pub use runtime::{
    InstalledPackage, PackageContext, PackageGrants, RuntimeConfig, RuntimeEvent, RuntimePool,
};
pub use source::install_source;
