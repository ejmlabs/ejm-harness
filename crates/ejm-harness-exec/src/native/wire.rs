use std::{fs::File, io, net::Ipv4Addr, path::Path, process::ExitStatus, time::Duration};

use anyhow::{Context, Result, ensure};
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use rightkit_fs::{StateDir, handle::ReadOptions};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use uuid::Uuid;

use super::{
    NativeCleanupState, NativeCommand, NativeOperation, NativeOperationState, placement::Placement,
};
use crate::{HostObservations, HostSampler};

pub(super) const PROTOCOL: u32 = 2;
pub(super) const TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const IDLE: Duration = Duration::from_secs(2);
pub(super) const PIPE_BYTES: usize = 16 * 1024;
pub(super) const MAX_LAUNCH_BYTES: usize = 256 * 1024;
pub(super) const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_SCOPES: usize = 1024;
pub(super) const MAX_HANDSHAKES: usize = 32;
pub(super) const MAX_READY_CLIENTS: usize = 256;
pub(super) const MAX_COMPLETED: usize = 256;
pub(super) const DISCOVERY: &str = "endpoint.json";
pub(super) type Transport = Framed<TcpStream, LengthDelimitedCodec>;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Advertisement {
    pub protocol: u32,
    pub epoch: Uuid,
    pub capability: Uuid,
    pub pid: u32,
    pub port: u16,
    pub domain: String,
    #[serde(default)]
    pub accounted: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Domain {
    cpu: Option<u32>,
    memory: Option<u64>,
    processes: Option<u64>,
    descriptors: Option<u64>,
    visibility: String,
    ancestry: Vec<DomainAncestor>,
    placement: Option<Placement>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DomainAncestor {
    id: String,
    parent: Option<String>,
    cpu: Option<u32>,
    memory: Option<u64>,
    processes: Option<u64>,
}

impl From<HostObservations> for Domain {
    fn from(value: HostObservations) -> Self {
        Self {
            cpu: value.cpu_millis.value,
            memory: value.memory_bytes.value,
            processes: value.process_limit.value,
            descriptors: value.descriptor_limit.value,
            visibility: format!("{:?}", value.domain_visibility),
            ancestry: value
                .domains
                .into_iter()
                .map(|domain| DomainAncestor {
                    id: domain.id,
                    parent: domain.parent,
                    cpu: domain.cpu_millis,
                    memory: domain.memory_bytes,
                    processes: domain.max_processes,
                })
                .collect(),
            placement: None,
        }
    }
}

pub(super) async fn observe_domain() -> Result<Domain> {
    let placement = Placement::observe()?;
    let observations = tokio::task::spawn_blocking(|| HostSampler::default().sample())
        .await
        .context("observe native execution domain")?;
    #[cfg(target_os = "linux")]
    ensure!(
        matches!(
            observations.domain_visibility,
            crate::ObservationAvailability::Measured | crate::ObservationAvailability::Estimated
        ) && observations.domains.len() > 1,
        "native cgroup placement evidence is unavailable"
    );
    ensure!(
        placement == Placement::observe()?,
        "native placement changed during observation"
    );
    let mut domain = Domain::from(observations);
    domain.placement = Some(placement);
    Ok(domain)
}

#[derive(Serialize, Deserialize)]
pub(super) struct Hello {
    pub protocol: u32,
    pub epoch: Uuid,
    pub capability: Uuid,
    pub domain_key: String,
    pub domain: Option<Domain>,
    pub action: Action,
}

#[derive(Serialize, Deserialize)]
pub(super) enum Action {
    Hold,
    Prepare,
    Query(NativeOperation),
}

#[derive(Serialize, Deserialize)]
pub(super) enum Reply {
    Held,
    Prepared(NativeOperation),
    Started { pid: u32 },
    State(NativeOperationState),
    Rejected(String),
}

#[derive(Serialize, Deserialize)]
pub(super) struct Launch {
    pub operation: NativeOperation,
    pub command: NativeCommand,
    pub domain: Domain,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Completion {
    pub cleanup: NativeCleanupState,
    pub raw_status: Option<i64>,
    pub error: Option<String>,
    pub peak_memory_bytes: u64,
    pub peak_processes: u32,
    pub violation: Option<Violation>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Violation {
    MemoryReservation,
    ProcessReservation,
    SamplingFailed,
    Unknown,
}

impl Violation {
    pub fn from_message(message: &'static str) -> Self {
        match message {
            "observed command tree memory exceeded its reservation" => Self::MemoryReservation,
            "observed command tree process count exceeded its reservation" => {
                Self::ProcessReservation
            }
            "process sampler failed" => Self::SamplingFailed,
            _ => Self::Unknown,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::MemoryReservation => "observed command tree memory exceeded its reservation",
            Self::ProcessReservation => {
                "observed command tree process count exceeded its reservation"
            }
            Self::SamplingFailed => "process sampler failed",
            Self::Unknown => "native scope reported an unknown monitoring violation",
        }
    }
}

impl Completion {
    pub fn uncertain(cleanup: NativeCleanupState) -> Self {
        Self {
            cleanup,
            raw_status: None,
            error: Some(
                match cleanup {
                    NativeCleanupState::Complete => {
                        "native reply lost; effects and exit status are uncertain"
                    }
                    NativeCleanupState::OwnerDied | NativeCleanupState::Pending => {
                        "native owner lost; effects and cleanup are unverified"
                    }
                }
                .into(),
            ),
            peak_memory_bytes: 0,
            peak_processes: 0,
            violation: None,
        }
    }

    pub fn status(&self) -> io::Result<ExitStatus> {
        if let Some(error) = &self.error {
            return Err(io::Error::other(error.clone()));
        }
        let raw = self
            .raw_status
            .ok_or_else(|| io::Error::other("native owner lost; cleanup unverified"))?;
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            Ok(ExitStatus::from_raw(
                i32::try_from(raw).map_err(io::Error::other)?,
            ))
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            Ok(ExitStatus::from_raw(
                u32::try_from(raw).map_err(io::Error::other)?,
            ))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = raw;
            Err(io::Error::other("native exit status unsupported"))
        }
    }
}

pub(super) fn raw_status(status: ExitStatus) -> i64 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        i64::from(status.into_raw())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        i64::from(status.into_raw())
    }
    #[cfg(not(any(unix, windows)))]
    {
        i64::from(status.code().unwrap_or(1))
    }
}

pub(super) fn framed(stream: TcpStream) -> Transport {
    // Most retained control links need only small frames; larger frames grow on demand.
    let mut transport = Framed::with_capacity(
        stream,
        LengthDelimitedCodec::builder()
            .max_frame_length(32 * 1024)
            .new_codec(),
        512,
    );
    transport.set_backpressure_boundary(8 * 1024);
    transport
}

pub(super) fn streaming(transport: Transport) -> Transport {
    let mut parts = transport.into_parts();
    parts.codec.set_max_frame_length(PIPE_BYTES + 1024);
    // Launch frames can be much larger than ordinary pipe chunks. Do not keep
    // their allocation for the lifetime of a resident host.
    parts.read_buf = BytesMut::from(parts.read_buf.as_ref());
    parts.write_buf = BytesMut::from(parts.write_buf.as_ref());
    Framed::from_parts(parts)
}

pub(super) fn json(value: &impl Serialize) -> Result<Bytes> {
    let mut data = vec![b'J'];
    serde_json::to_writer(&mut data, value)?;
    ensure!(
        data.len() <= MAX_FRAME_BYTES,
        "native control frame too large"
    );
    Ok(data.into())
}

pub(super) fn decode<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
    ensure!(data.first() == Some(&b'J'), "expected native control frame");
    serde_json::from_slice(&data[1..]).context("decode native control frame")
}

pub(super) async fn receive<T: DeserializeOwned>(transport: &mut Transport) -> Result<T> {
    let data = tokio::time::timeout(TIMEOUT, transport.next())
        .await
        .context("native owner reply timed out")?
        .context("native owner disconnected")??;
    decode(&data)
}

pub(super) async fn send(transport: &mut Transport, value: &impl Serialize) -> Result<()> {
    tokio::time::timeout(TIMEOUT, transport.send(json(value)?))
        .await
        .context("native owner send timed out")??;
    Ok(())
}

pub(super) async fn connect(ad: &Advertisement) -> Result<Transport> {
    ensure!(ad.protocol == PROTOCOL, "native owner protocol mismatch");
    let stream = tokio::time::timeout(TIMEOUT, TcpStream::connect((Ipv4Addr::LOCALHOST, ad.port)))
        .await
        .context("connect native owner timed out")??;
    stream.set_nodelay(true)?;
    Ok(framed(stream))
}

pub(super) fn directory(root: &Path, key: &str) -> Result<StateDir> {
    ensure!(
        !key.is_empty()
            && key.len() <= 128
            && key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "invalid native owner domain"
    );
    Ok(StateDir::create(root)?.child("native")?.child(key)?)
}

/// This inode is permanent. Only endpoint discovery is atomically replaced.
pub(super) fn stable_lock(directory: &StateDir, name: &str) -> Result<File> {
    match directory.create_new_empty(name) {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == Some(io::ErrorKind::AlreadyExists) => Ok(directory
            .open_read(
                name,
                &ReadOptions::new()
                    .require_single_link(true)
                    .require_owner_only(true),
            )?),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn packet(tag: u8, bytes: &[u8]) -> Bytes {
    let mut data = BytesMut::with_capacity(bytes.len() + 1);
    data.extend_from_slice(&[tag]);
    data.extend_from_slice(bytes);
    data.freeze()
}
