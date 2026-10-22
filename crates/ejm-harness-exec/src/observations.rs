//! Bounded, read-only observations. These values never enforce native limits.

use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sysinfo::{CpuRefreshKind, System};

#[cfg(any(target_os = "linux", all(test, unix)))]
#[path = "observations/linux.rs"]
mod linux;

const MAX_AGE_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationAvailability {
    Measured,
    Estimated,
    Unavailable,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSource {
    NativeSystem,
    SchedulerAvailability,
    ProcessResourceLimit,
    LinuxCgroupV2,
}

/// Missing values and stale samples remain distinct from a measured zero.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Observation<T> {
    pub value: Option<T>,
    pub source: ObservationSource,
    pub availability: ObservationAvailability,
    pub age_ms: u64,
    pub max_age_ms: u64,
}

impl<T> Observation<T> {
    #[must_use]
    pub fn is_fresh(&self) -> bool {
        self.value.is_some()
            && self.age_ms <= self.max_age_ms
            && matches!(
                self.availability,
                ObservationAvailability::Measured | ObservationAvailability::Estimated
            )
    }

    fn measured(value: T, source: ObservationSource) -> Self {
        Self {
            value: Some(value),
            source,
            availability: ObservationAvailability::Measured,
            age_ms: 0,
            max_age_ms: MAX_AGE_MS,
        }
    }

    fn missing(source: ObservationSource, availability: ObservationAvailability) -> Self {
        Self {
            value: None,
            source,
            availability,
            age_ms: 0,
            max_age_ms: MAX_AGE_MS,
        }
    }
}

/// Visible ancestor constraints. IDs contain native scope identities, not paths.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResourceDomainObservation {
    pub id: String,
    pub parent: Option<String>,
    pub cpu_millis: Option<u32>,
    pub memory_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub max_processes: Option<u64>,
    pub available_processes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HostObservations {
    #[serde(skip, default = "unknown_sample_time")]
    #[schemars(skip)]
    pub sampled_at: Instant,
    pub cpu_millis: Observation<u32>,
    pub cpu_usage_percent: Observation<f32>,
    pub memory_bytes: Observation<u64>,
    pub available_memory_bytes: Observation<u64>,
    /// The soft Unix per-user process limit; it is not free process capacity.
    pub process_limit: Observation<u64>,
    /// The soft per-process file-descriptor limit, never a shared host budget.
    pub descriptor_limit: Observation<u64>,
    pub domains: Vec<ResourceDomainObservation>,
    /// Unsupported or incomplete ancestor visibility prevents safe growth.
    pub domain_visibility: ObservationAvailability,
}

fn unknown_sample_time() -> Instant {
    // A remote or saved snapshot has no local monotonic clock anchor. Keep it
    // diagnostic; it must be refreshed before governing new capacity.
    Instant::now()
        .checked_sub(Duration::from_millis(MAX_AGE_MS + 1))
        .unwrap_or_else(Instant::now)
}

impl HostObservations {
    /// Recompute age before using a stored sample for an admission decision.
    pub fn refresh_age(&mut self) {
        let age = u64::try_from(self.sampled_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.cpu_millis.age_ms = age;
        self.cpu_usage_percent.age_ms = age;
        self.memory_bytes.age_ms = age;
        self.available_memory_bytes.age_ms = age;
        self.process_limit.age_ms = age;
        self.descriptor_limit.age_ms = age;
    }
}

/// Reuse this sampler on a bounded control interval and outside async workers.
/// It opens no persistent process handles and creates no background task.
pub struct HostSampler {
    system: System,
    previous_cpu: Option<Instant>,
}

impl Default for HostSampler {
    fn default() -> Self {
        Self {
            system: System::new(),
            previous_cpu: None,
        }
    }
}

impl HostSampler {
    /// Stable native boot identity when a maintained native API exposes one.
    /// Windows stays unsupported rather than using a wall-clock approximation.
    #[must_use]
    pub fn boot_identity() -> Option<String> {
        static BOOT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        BOOT.get_or_init(boot_identity).clone()
    }

    /// Observe this process's native birth identity without inspecting content.
    /// Identity is a recovery witness, not proof that descendants have ended.
    #[must_use]
    pub fn owner_identity() -> Option<ProcessIdentity> {
        process_identity(std::process::id())
    }

    #[must_use]
    pub fn sample(&mut self) -> HostObservations {
        use ObservationAvailability::{Measured, Unavailable, Unsupported};
        use ObservationSource::{NativeSystem, ProcessResourceLimit, SchedulerAvailability};
        let now = Instant::now();
        self.system.refresh_memory();
        if self.system.cpus().is_empty() {
            self.system.refresh_cpu_list(CpuRefreshKind::nothing());
        }
        self.system.refresh_cpu_usage();
        let cpu_ready = self.previous_cpu.is_some_and(|previous| {
            now.duration_since(previous) >= sysinfo::MINIMUM_CPU_UPDATE_INTERVAL
        });
        self.previous_cpu = Some(now);
        let result = HostObservations {
            sampled_at: now,
            cpu_millis: std::thread::available_parallelism()
                .ok()
                .and_then(|count| u32::try_from(count.get()).ok())
                .map_or_else(
                    || Observation::missing(SchedulerAvailability, Unavailable),
                    |count| {
                        Observation::measured(count.saturating_mul(1_000), SchedulerAvailability)
                    },
                ),
            cpu_usage_percent: if cpu_ready && sysinfo::IS_SUPPORTED_SYSTEM {
                Observation::measured(self.system.global_cpu_usage(), NativeSystem)
            } else {
                Observation::missing(NativeSystem, Unavailable)
            },
            memory_bytes: positive_memory(self.system.total_memory()),
            available_memory_bytes: if sysinfo::IS_SUPPORTED_SYSTEM {
                Observation::measured(self.system.available_memory(), NativeSystem)
            } else {
                Observation::missing(NativeSystem, Unsupported)
            },
            process_limit: Observation::missing(ProcessResourceLimit, Unsupported),
            descriptor_limit: Observation::missing(ProcessResourceLimit, Unsupported),
            domains: vec![ResourceDomainObservation {
                id: "host".into(),
                parent: None,
                cpu_millis: u32::try_from(self.system.cpus().len())
                    .ok()
                    .map(|count| count.saturating_mul(1_000)),
                memory_bytes: (self.system.total_memory() > 0).then(|| self.system.total_memory()),
                available_memory_bytes: sysinfo::IS_SUPPORTED_SYSTEM
                    .then(|| self.system.available_memory()),
                max_processes: None,
                available_processes: None,
            }],
            domain_visibility: if sysinfo::IS_SUPPORTED_SYSTEM {
                Measured
            } else {
                Unsupported
            },
        };
        #[cfg(unix)]
        let mut result = result;
        #[cfg(unix)]
        {
            use rustix::process::{Resource, getrlimit};
            result.process_limit = finite_limit(getrlimit(Resource::Nproc).current);
            result.descriptor_limit = finite_limit(getrlimit(Resource::Nofile).current);
        }
        #[cfg(target_os = "linux")]
        linux::apply(&mut result);
        result
    }
}

fn positive_memory(value: u64) -> Observation<u64> {
    if value > 0 {
        Observation::measured(value, ObservationSource::NativeSystem)
    } else {
        Observation::missing(
            ObservationSource::NativeSystem,
            ObservationAvailability::Unavailable,
        )
    }
}

#[cfg(unix)]
fn finite_limit(value: Option<u64>) -> Observation<u64> {
    value.map_or_else(
        || {
            Observation::missing(
                ObservationSource::ProcessResourceLimit,
                ObservationAvailability::Unavailable,
            )
        },
        |value| Observation::measured(value, ObservationSource::ProcessResourceLimit),
    )
}

/// Native birth identity. Linux uses raw start ticks; other targets retain
/// whole start seconds for compatibility with existing shared witnesses.
/// This token does not prove absence across PID reuse or descendant cleanup.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
    pub boot_id: String,
}

/// Obtain a bounded native observation for one process.
///
/// Missing or unreadable processes return `None`. Neither absence nor matching
/// identity proves that children have ended or permits refunding a shared grant.
#[must_use]
pub fn process_identity(pid: u32) -> Option<ProcessIdentity> {
    let boot_id = HostSampler::boot_identity()?;
    let process = crate::processes::Sampler::default().observe(pid).ok()?;
    if process.zombie {
        return None;
    }
    // Internal macOS cleanup compares the native microsecond birth timestamp.
    // Keep this public and persisted token in its original whole-second units.
    #[cfg(target_os = "macos")]
    let start_time = process.start / 1_000_000;
    #[cfg(not(target_os = "macos"))]
    let start_time = process.start;
    Some(ProcessIdentity {
        pid,
        start_time,
        boot_id,
    })
}

fn boot_identity() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        use std::io::Read;
        let mut bytes = String::new();
        std::fs::File::open("/proc/sys/kernel/random/boot_id")
            .ok()?
            .take(129)
            .read_to_string(&mut bytes)
            .ok()?;
        uuid::Uuid::parse_str(bytes.trim())
            .ok()
            .map(|value| value.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        use sysctl::Sysctl;
        let value = sysctl::Ctl::new("kern.bootsessionuuid")
            .ok()?
            .value_string()
            .ok()?;
        uuid::Uuid::parse_str(value.trim())
            .ok()
            .map(|value| value.to_string())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}
