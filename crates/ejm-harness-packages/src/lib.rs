//! Trusted native extensions with explicit grants and shared, bounded language hosts.
//!
//! Node and Python packages execute arbitrary host code. Their grants control
//! registration in ejm-harness, not operating system containment.

mod adapters;
mod manager;
mod registry;
mod runtime;
mod source;
mod transport;

pub use ejm_harness_package_spec::{HookEvent, RuntimeKind};
pub use manager::{PackageManager, PackageRecord, pack_directory, read_bounded};
pub use registry::{
    DISCARD_RETIRED_REGISTRY_COMMAND, DeviceCode, PUBLIC_REGISTRY_ORIGIN, RegistryClient,
    RetiredRegistrySession, VersionRecord, delete_registry_token,
    discard_retired_registry_sessions, force_discard_legacy_registry_session, load_registry_token,
    locate_retired_registry_sessions, migrate_registry_token, registry_origin_is_public,
    retired_registry_session_notice, save_registry_token, validate_registry_origin,
};
pub use runtime::{
    InstalledPackage, PackageContext, PackageGrants, RuntimeConfig, RuntimeEvent, RuntimePool,
};
pub use source::install_source;
