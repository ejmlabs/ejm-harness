//! Private native files. The vault supplies encrypted bytes and owns transactions.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(super) use unix::{PrivateDirectory, RawTransaction};
#[cfg(windows)]
pub(super) use windows::{PrivateDirectory, RawTransaction};
