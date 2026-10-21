//! Shared package contracts. This crate has no executor, runtime, network, or UI dependency.

mod archive;
mod manifest;

pub use archive::{ArchiveFile, PackageArchive, archive_sha256, read_archive, validate_archive};
pub use manifest::{
    CommandExport, Exports, HookEvent, HookExport, Manifest, NamedExport, Runtime, RuntimeKind,
    ToolEffect, ToolExport, validate_name, validate_relative_path,
};

pub const MANIFEST_FILE: &str = "ejm.package.json";
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;
pub const MAX_ARCHIVE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_EXPANDED_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_ARCHIVE_FILES: usize = 1024;
