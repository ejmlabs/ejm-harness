//! Bounded feedback from ordinary work. No synthetic work or per-agent history.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const SUB_BUCKET_BITS: u32 = 4;
const SUB_BUCKETS: usize = 1 << SUB_BUCKET_BITS;
const BINS: usize = SUB_BUCKETS * (u64::BITS as usize - SUB_BUCKET_BITS as usize + 1);
const CLASSES: usize = 8;
const MIN_SAMPLES: u64 = 8;
const HEALTHY_WINDOWS: u8 = 3;
const COOLDOWN_WINDOWS: u8 = 5;

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TimingSummary {
    pub samples: u64,
    pub p50_micros: u64,
    pub p95_micros: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LaneSnapshot {
    pub successful: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub queue: TimingSummary,
    pub preparation: TimingSummary,
    pub provider_wait: TimingSummary,
    pub service: TimingSummary,
    pub successful_per_second: f64,
}

#[derive(Debug, Clone)]
struct Histogram {
    bins: [u64; BINS],
    samples: u64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            bins: [0; BINS],
            samples: 0,
        }
    }
}

impl Histogram {
    fn record(&mut self, elapsed: Duration) {
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        let shift = micros.bit_width().saturating_sub(SUB_BUCKET_BITS + 1);
        let index =
            usize::try_from(micros >> shift).unwrap_or(BINS - 1) + shift as usize * SUB_BUCKETS;
        self.bins[index] = self.bins[index].saturating_add(1);
        self.samples = self.samples.saturating_add(1);
    }

    fn percentile_bounds(&self, percent: u64) -> (u64, u64) {
        if self.samples == 0 {
            return (0, 0);
        }
        let threshold = self.samples.saturating_mul(percent).div_ceil(100);
        let mut count = 0_u64;
        for (index, samples) in self.bins.iter().enumerate() {
            count = count.saturating_add(*samples);
            if count >= threshold {
                if index < SUB_BUCKETS {
                    return (index as u64, index as u64);
                }
                let shift = index / SUB_BUCKETS - 1;
                let lower = ((SUB_BUCKETS + index % SUB_BUCKETS) as u64) << shift;
                return (lower, lower.saturating_add((1_u64 << shift) - 1));
            }
        }
        (u64::MAX, u64::MAX)
    }

    fn percentile(&self, percent: u64) -> u64 {
        self.percentile_bounds(percent).1
    }

    fn snapshot(&self) -> TimingSummary {
        TimingSummary {
            samples: self.samples,
            p50_micros: self.percentile(50),
            p95_micros: self.percentile(95),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Successful,
    Failed,
    Cancelled,
}

#[derive(Default, Clone)]
pub(super) struct Timings {
    pub queue: Duration,
    pub preparation: Duration,
    pub provider_wait: Duration,
    pub service: Duration,
}

/// Numeric cost buckets contain no tool names, model names, or request text.
pub(super) fn workload_class(bytes: u64) -> usize {
    match bytes {
        0..=4_096 => 0,
        4_097..=16_384 => 1,
        16_385..=65_536 => 2,
        65_537..=262_144 => 3,
        262_145..=1_048_576 => 4,
        1_048_577..=16_777_216 => 5,
        16_777_217..=268_435_456 => 6,
        _ => 7,
    }
}

#[derive(Default)]
struct Window {
    successes: u64,
    failures: u64,
    classes: [u64; CLASSES],
    service: Histogram,
    elapsed: Duration,
    healthy: u8,
    span: Duration,
}

impl Window {
    fn accept(&mut self, interval: &Self, elapsed: Duration) {
        self.successes = self.successes.saturating_add(interval.successes);
        self.failures = self.failures.saturating_add(interval.failures);
        for (total, count) in self.classes.iter_mut().zip(&interval.classes) {
            *total = total.saturating_add(*count);
        }
        for (total, count) in self.service.bins.iter_mut().zip(&interval.service.bins) {
            *total = total.saturating_add(*count);
        }
        self.service.samples = self
            .service
            .samples
            .saturating_add(interval.service.samples);
        self.elapsed = self.elapsed.saturating_add(elapsed);
        self.healthy = self.healthy.saturating_add(1);
        self.span = self.span.max(elapsed);
    }
}

#[derive(Clone, Copy)]
struct Point {
    target: usize,
    rate: f64,
    p95: u64,
    p95_lower: u64,
    class: usize,
}

#[derive(Clone, Copy)]
enum Submission {
    Unsent,
    Pending(i64),
    Considered,
}

struct Probe {
    baseline: Point,
    submission: Submission,
}

/// Only stable, validated evidence is persisted. An unaccepted probe is a hint
/// about nothing and must never become the next process's starting target.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Evidence {
    pub samples: u64,
    pub target: usize,
    pub rate: f64,
    pub p95_micros: u64,
    pub class: usize,
}

struct Lane {
    target: usize,
    totals: LaneSnapshot,
    queue: Histogram,
    preparation: Histogram,
    provider_wait: Histogram,
    service: Histogram,
    interval: Window,
    window: Window,
    class_totals: [u64; CLASSES],
    stable: Option<Point>,
    probe: Option<Probe>,
    cooldown: u8,
    probe_step: usize,
}

impl Lane {
    fn new(target: usize) -> Self {
        Self {
            target,
            totals: LaneSnapshot::default(),
            queue: Histogram::default(),
            preparation: Histogram::default(),
            provider_wait: Histogram::default(),
            service: Histogram::default(),
            interval: Window::default(),
            window: Window::default(),
            class_totals: [0; CLASSES],
            stable: None,
            probe: None,
            cooldown: 0,
            probe_step: 0,
        }
    }

    fn record(&mut self, outcome: Outcome, timing: &Timings, class: usize) {
        self.queue.record(timing.queue);
        self.preparation.record(timing.preparation);
        self.provider_wait.record(timing.provider_wait);
        self.service.record(timing.service);
        match outcome {
            Outcome::Successful => {
                self.totals.successful = self.totals.successful.saturating_add(1);
                self.interval.successes = self.interval.successes.saturating_add(1);
                self.interval.classes[class.min(CLASSES - 1)] =
                    self.interval.classes[class.min(CLASSES - 1)].saturating_add(1);
                self.class_totals[class.min(CLASSES - 1)] =
                    self.class_totals[class.min(CLASSES - 1)].saturating_add(1);
                self.interval.service.record(timing.service);
            }
            Outcome::Failed => {
                self.totals.failed = self.totals.failed.saturating_add(1);
                self.interval.failures = self.interval.failures.saturating_add(1);
            }
            Outcome::Cancelled => self.totals.cancelled = self.totals.cancelled.saturating_add(1),
        }
    }

    fn snapshot(&self) -> LaneSnapshot {
        LaneSnapshot {
            queue: self.queue.snapshot(),
            preparation: self.preparation.snapshot(),
            provider_wait: self.provider_wait.snapshot(),
            service: self.service.snapshot(),
            ..self.totals.clone()
        }
    }

    fn reduce(&mut self) {
        self.target = self.target.div_ceil(2).max(1);
        self.reset_evidence();
        self.cooldown = COOLDOWN_WINDOWS;
    }

    fn reset_evidence(&mut self) {
        self.probe_step = 0;
        self.stable = None;
        self.probe = None;
        self.interval = Window::default();
        self.window = Window::default();
    }

    fn abort_probe(&mut self) -> bool {
        let Some(probe) = self.probe.take() else {
            return false;
        };
        self.target = self.target.min(probe.baseline.target).max(1);
        self.reset_evidence();
        self.cooldown = COOLDOWN_WINDOWS;
        true
    }

    fn observe_plan(&mut self, requested: usize, planned: usize, epoch: i64, refreshed: bool) {
        let Some(probe) = self.probe.as_mut() else {
            return;
        };
        if requested != self.target {
            return;
        }
        probe.submission = match probe.submission {
            Submission::Unsent if refreshed => Submission::Considered,
            Submission::Unsent => Submission::Pending(epoch),
            Submission::Pending(submitted) if refreshed || epoch > submitted => {
                Submission::Considered
            }
            Submission::Considered => Submission::Considered,
            Submission::Pending(_) => return,
        };
        if matches!(probe.submission, Submission::Considered) && planned < self.target {
            self.abort_probe();
        }
    }

    fn await_grant(&mut self) {
        // Discard ungranted work, retaining earlier fully granted intervals
        // and a pending probe's baseline through plan and donor delay.
        self.interval = Window::default();
        if self.probe.is_none() {
            self.stable = None;
        }
    }

    fn collect_interval(
        &mut self,
        elapsed: Duration,
        demand: bool,
        clear: bool,
        granted: usize,
    ) -> bool {
        let interval = std::mem::take(&mut self.interval);
        if self.window.healthy > 0 {
            self.window.span = self.window.span.saturating_add(elapsed);
            let horizon = super::CONTROL_INTERVAL * u32::from(HEALTHY_WINDOWS + COOLDOWN_WINDOWS);
            if self.window.span > horizon {
                self.window = Window::default();
            }
        }
        if !demand || !clear {
            self.window = Window::default();
            return false;
        }
        if granted < self.target {
            self.await_grant();
            return false;
        }
        self.window.accept(&interval, elapsed);
        true
    }

    fn tick(
        &mut self,
        elapsed: Duration,
        demand: bool,
        clear: bool,
        granted: usize,
        ceiling: usize,
        can_probe: bool,
    ) -> bool {
        self.target = self.target.min(ceiling).max(1);
        self.cooldown = self.cooldown.saturating_sub(1);
        if !self.collect_interval(elapsed, demand, clear, granted) {
            return !demand && self.abort_probe();
        }
        if self.window.healthy < HEALTHY_WINDOWS || self.window.successes < MIN_SAMPLES {
            return false;
        }
        let window = std::mem::take(&mut self.window);
        let (class, count) = window
            .classes
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| **count)
            .map_or((0, 0), |(index, count)| (index, *count));
        if count.saturating_mul(5) < window.successes.saturating_mul(4) || window.failures != 0 {
            if let Some(probe) = self.probe.take() {
                self.target = probe.baseline.target;
            }
            self.stable = None;
            self.probe_step = 0;
            self.cooldown = COOLDOWN_WINDOWS;
            return true;
        }
        let rate = f64::from(u32::try_from(window.successes).unwrap_or(u32::MAX))
            / window.elapsed.as_secs_f64().max(0.001);
        self.totals.successful_per_second = rate;
        let point = Point {
            target: self.target,
            rate,
            p95: window.service.percentile(95),
            p95_lower: window.service.percentile_bounds(95).0,
            class,
        };
        if let Some(probe) = self.probe.take() {
            let baseline = probe.baseline;
            let latency_ok = point.p95 <= baseline.p95_lower.saturating_add(baseline.p95_lower / 4);
            if baseline.class != class {
                self.target = baseline.target;
                self.stable = None;
                self.probe_step = 0;
            } else if rate <= baseline.rate * 1.10 || !latency_ok {
                self.target = baseline.target;
                self.stable = Some(baseline);
                self.probe_step = if latency_ok {
                    point
                        .target
                        .saturating_sub(baseline.target)
                        .saturating_mul(2)
                        .min(8)
                } else {
                    0
                };
            } else {
                self.stable = Some(point);
                self.probe_step = 0;
            }
            self.cooldown = COOLDOWN_WINDOWS;
            return true;
        }
        if self.stable.is_some_and(|stable| stable.class != class) {
            self.stable = Some(point);
            self.probe_step = 0;
            self.cooldown = COOLDOWN_WINDOWS;
            return false;
        }
        self.stable = Some(point);
        if can_probe && self.cooldown == 0 && self.target < ceiling {
            self.probe = Some(Probe {
                baseline: point,
                submission: Submission::Unsent,
            });
            self.target = self
                .target
                .saturating_add((self.target / 4).clamp(1, 8).max(self.probe_step))
                .min(ceiling);
            return true;
        }
        false
    }

    fn evidence(&self) -> Option<Evidence> {
        self.stable.map(|point| Evidence {
            samples: self.class_totals[point.class],
            target: point.target,
            rate: point.rate,
            p95_micros: point.p95,
            class: point.class,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Pressure {
    Clear,
    Unknown,
    Busy,
}

pub(super) struct Tuner {
    model: Lane,
    tool: Lane,
    next_model: bool,
    pending_elapsed: Duration,
    pending_pressure: Pressure,
    pending_demand: [bool; 2],
    pending_granted: [usize; 2],
}

impl Tuner {
    pub fn new(models: usize, jobs: usize) -> Self {
        Self {
            model: Lane::new(models),
            tool: Lane::new(jobs),
            next_model: true,
            pending_elapsed: Duration::ZERO,
            pending_pressure: Pressure::Clear,
            pending_demand: [false; 2],
            pending_granted: [usize::MAX; 2],
        }
    }
    pub fn models(&self) -> usize {
        self.model.target
    }
    pub fn jobs(&self) -> usize {
        self.tool.target
    }
    pub fn needs_backoff(&self) -> bool {
        self.pending_pressure == Pressure::Busy
    }
    pub fn model_snapshot(&self) -> LaneSnapshot {
        self.model.snapshot()
    }
    pub fn tool_snapshot(&self) -> LaneSnapshot {
        self.tool.snapshot()
    }
    pub fn record_model(&mut self, outcome: Outcome, timing: &Timings, class: usize) {
        self.model.record(outcome, timing, class);
    }
    pub fn record_tool(&mut self, outcome: Outcome, timing: &Timings, class: usize) {
        self.tool.record(outcome, timing, class);
    }
    pub fn evidence(&self) -> [Option<Evidence>; 2] {
        [self.model.evidence(), self.tool.evidence()]
    }
    pub fn class_samples(&self) -> [[u64; CLASSES]; 2] {
        [self.model.class_totals, self.tool.class_totals]
    }
    pub fn reset_evidence(&mut self) {
        self.model.reset_evidence();
        self.tool.reset_evidence();
    }

    pub fn cancel_probes(&mut self) {
        self.model.abort_probe();
        self.tool.abort_probe();
    }

    pub fn observe_plan(
        &mut self,
        requested: [usize; 2],
        planned: [usize; 2],
        epoch: i64,
        refreshed: bool,
    ) {
        self.model
            .observe_plan(requested[0], planned[0], epoch, refreshed);
        self.tool
            .observe_plan(requested[1], planned[1], epoch, refreshed);
    }

    pub fn apply_hint(&mut self, lane: usize, target: usize) {
        let lane = if lane == 0 {
            &mut self.model
        } else {
            &mut self.tool
        };
        lane.target = target.max(1);
        lane.reset_evidence();
        lane.cooldown = COOLDOWN_WINDOWS;
    }

    pub fn tick(
        &mut self,
        elapsed: Duration,
        demand: [bool; 2],
        pressure: Pressure,
        granted: [usize; 2],
        ceilings: [usize; 2],
    ) {
        // Demand can wake coordination sooner than the feedback interval. Keep
        // every pressure and active-demand grant restriction, but adjust only
        // once per real window. A late wake consumes its time without catch-up
        // steps. Brief idle samples do not erase ordinary completed work.
        self.pending_elapsed = self.pending_elapsed.saturating_add(elapsed);
        self.pending_pressure = self.pending_pressure.max(pressure);
        for (lane, waiting) in demand.into_iter().enumerate() {
            self.pending_demand[lane] |= waiting;
            if waiting {
                self.pending_granted[lane] = self.pending_granted[lane].min(granted[lane]);
            }
        }
        if pressure == Pressure::Busy {
            self.reset_evidence();
        }
        // Installed grants alone govern work and throughput samples. A cached
        // plan or a charged donor can delay an otherwise valid probe grant.
        if demand[0] && granted[0] < self.model.target {
            self.model.await_grant();
        }
        if demand[1] && granted[1] < self.tool.target {
            self.tool.await_grant();
        }
        if self.pending_elapsed < super::CONTROL_INTERVAL {
            return;
        }
        let elapsed = std::mem::take(&mut self.pending_elapsed);
        let pressure = std::mem::replace(&mut self.pending_pressure, Pressure::Clear);
        let window_demand = std::mem::take(&mut self.pending_demand);
        let granted = std::mem::replace(&mut self.pending_granted, [usize::MAX; 2]);
        if pressure == Pressure::Busy {
            self.model.reduce();
            self.tool.reduce();
            return;
        }
        let clear = pressure == Pressure::Clear;
        let probing = [self.model.probe.is_some(), self.tool.probe.is_some()];
        if self.next_model {
            let changed = self.model.tick(
                elapsed,
                window_demand[0],
                clear,
                granted[0],
                ceilings[0],
                demand[0] && self.tool.probe.is_none(),
            );
            self.tool.tick(
                elapsed,
                window_demand[1],
                clear,
                granted[1],
                ceilings[1],
                demand[1] && !changed && self.model.probe.is_none(),
            );
        } else {
            let changed = self.tool.tick(
                elapsed,
                window_demand[1],
                clear,
                granted[1],
                ceilings[1],
                demand[1] && self.model.probe.is_none(),
            );
            self.model.tick(
                elapsed,
                window_demand[0],
                clear,
                granted[0],
                ceilings[0],
                demand[0] && !changed && self.tool.probe.is_none(),
            );
        }
        if !probing[0] && self.model.probe.is_some() {
            self.next_model = false;
        } else if !probing[1] && self.tool.probe.is_some() {
            self.next_model = true;
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Deterministic controller fixtures use direct assertions."
)]
mod tests {
    use super::*;

    fn window(tuner: &mut Tuner, successes: usize, service: Duration, class: usize) {
        for _ in 0..3 {
            for _ in 0..successes {
                tuner.record_model(
                    Outcome::Successful,
                    &Timings {
                        service,
                        ..Timings::default()
                    },
                    class,
                );
            }
            tuner.tick(
                Duration::from_secs(2),
                [true, false],
                Pressure::Clear,
                [tuner.models(), tuner.jobs()],
                [32, 8],
            );
        }
    }

    #[test]
    fn probes_require_demand_and_comparable_gain() {
        let mut tuner = Tuner::new(4, 2);
        for _ in 0..20 {
            tuner.tick(
                Duration::from_secs(2),
                [false, false],
                Pressure::Clear,
                [4, 2],
                [32, 8],
            );
        }
        assert_eq!(tuner.models(), 4);
        window(&mut tuner, 8, Duration::from_millis(10), 1);
        assert_eq!(tuner.models(), 5);
        window(&mut tuner, 8, Duration::from_millis(10), 1);
        assert_eq!(
            tuner.models(),
            4,
            "equal throughput must prefer the smaller target"
        );
        assert_eq!(tuner.evidence()[0].as_ref().unwrap().target, 4);
    }

    #[test]
    fn latency_safe_plateau_retries_a_larger_bounded_probe() {
        let mut tuner = Tuner::new(4, 2);
        window(&mut tuner, 8, Duration::from_millis(10), 1);
        assert_eq!(tuner.models(), 5);
        window(&mut tuner, 8, Duration::from_millis(10), 1);
        assert_eq!(tuner.models(), 4);
        for _ in 0..2 {
            window(&mut tuner, 8, Duration::from_millis(10), 1);
        }
        assert_eq!(tuner.models(), 6);
        window(&mut tuner, 12, Duration::from_millis(10), 1);
        assert_eq!(tuner.models(), 6);
        assert_eq!(tuner.evidence()[0].as_ref().unwrap().target, 6);
        assert_eq!(tuner.model.probe_step, 0);
        tuner.model.probe_step = 8;
        tuner.model.reduce();
        assert_eq!(tuner.model.probe_step, 0);
    }

    #[test]
    fn easier_work_does_not_validate_a_probe() {
        let mut tuner = Tuner::new(4, 2);
        window(&mut tuner, 8, Duration::from_millis(10), 5);
        window(&mut tuner, 30, Duration::from_millis(1), 1);
        assert_eq!(tuner.models(), 4);
        assert!(tuner.evidence()[0].is_none());
    }

    #[test]
    fn latency_budget_survives_histogram_boundaries() {
        for (baseline, probe, accepted) in [(32_767, 32_768, true), (16_384, 32_767, false)] {
            let mut tuner = Tuner::new(4, 2);
            window(&mut tuner, 8, Duration::from_micros(baseline), 1);
            assert_eq!(tuner.models(), 5);
            window(&mut tuner, 30, Duration::from_micros(probe), 1);
            assert_eq!(tuner.models(), if accepted { 5 } else { 4 });
        }
    }

    #[test]
    fn latency_bounds_cover_duration_range_with_bounded_state() {
        assert!(std::mem::size_of::<Tuner>() <= 128 * 1024);
        for exponent in 0..64 {
            let boundary = 1_u64 << exponent;
            for micros in [boundary - 1, boundary, boundary.saturating_add(1), u64::MAX] {
                let mut histogram = Histogram::default();
                histogram.record(Duration::from_micros(micros));
                let (lower, upper) = histogram.percentile_bounds(95);
                assert!(lower <= micros && micros <= upper);
                assert!(upper - lower <= lower / 16);
            }
        }
    }

    #[test]
    fn unknown_samples_block_growth_and_pressure_drains_targets() {
        let mut tuner = Tuner::new(8, 4);
        for _ in 0..10 {
            tuner.record_model(Outcome::Successful, &Timings::default(), 0);
            tuner.tick(
                Duration::from_secs(2),
                [true, true],
                Pressure::Unknown,
                [8, 4],
                [32, 8],
            );
        }
        assert_eq!((tuner.models(), tuner.jobs()), (8, 4));
        tuner.tick(
            Duration::from_secs(2),
            [true, true],
            Pressure::Busy,
            [8, 4],
            [32, 8],
        );
        assert_eq!((tuner.models(), tuner.jobs()), (4, 2));
        assert_eq!(tuner.model_snapshot().successful, 10);
    }

    #[test]
    fn fast_wakes_coalesce_pressure_without_catching_up() {
        let mut tuner = Tuner::new(12, 8);
        let tick = |tuner: &mut Tuner, elapsed, pressure| {
            tuner.tick(
                elapsed,
                [true, true],
                pressure,
                [tuner.models(), tuner.jobs()],
                [32, 16],
            );
        };
        let fast = Duration::from_millis(500);
        for pressure in [Pressure::Busy, Pressure::Clear, Pressure::Clear] {
            tick(&mut tuner, fast, pressure);
            assert_eq!((tuner.models(), tuner.jobs()), (12, 8));
            assert!(tuner.needs_backoff());
        }
        tick(&mut tuner, fast, Pressure::Clear);
        assert_eq!((tuner.models(), tuner.jobs()), (6, 4));
        assert!(!tuner.needs_backoff());
        for _ in 0..3 {
            tick(&mut tuner, fast, Pressure::Busy);
            assert_eq!((tuner.models(), tuner.jobs()), (6, 4));
        }
        tick(&mut tuner, fast, Pressure::Busy);
        assert_eq!((tuner.models(), tuner.jobs()), (3, 2));

        tick(&mut tuner, Duration::from_millis(4_200), Pressure::Busy);
        assert_eq!((tuner.models(), tuner.jobs()), (2, 1));
        tick(&mut tuner, Duration::from_millis(1_900), Pressure::Busy);
        assert_eq!((tuner.models(), tuner.jobs()), (2, 1));
        tick(&mut tuner, fast, Pressure::Clear);
        assert_eq!((tuner.models(), tuner.jobs()), (1, 1));
    }

    fn probe_after_fast_interruption(lane: usize, pressure: Pressure, demand: bool) {
        let mut tuner = Tuner::new(4, 2);
        for window in 0..4 {
            for sample in 0..4 {
                let first = window == 0 && sample == 0;
                for _ in 0..2 {
                    let timing = Timings {
                        service: Duration::from_millis(10),
                        ..Timings::default()
                    };
                    if lane == 0 {
                        tuner.record_model(Outcome::Successful, &timing, 1);
                    } else {
                        tuner.record_tool(Outcome::Successful, &timing, 1);
                    }
                }
                let mut waiting = [false; 2];
                waiting[lane] = !first || demand;
                tuner.tick(
                    Duration::from_millis(500),
                    waiting,
                    if first { pressure } else { Pressure::Clear },
                    [tuner.models(), tuner.jobs()],
                    [32, 8],
                );
            }
            let target = [tuner.models(), tuner.jobs()][lane];
            assert_eq!(target, [4, 2][lane] + usize::from(window == 3));
        }
    }

    #[test]
    fn intermediate_unknown_sample_cannot_validate_a_probe() {
        probe_after_fast_interruption(0, Pressure::Unknown, true);
    }

    #[test]
    fn bursty_demand_recovers_both_lanes_after_pressure() {
        for lane in 0..2 {
            let mut tuner = Tuner::new(8, 4);
            tuner.tick(
                super::super::CONTROL_INTERVAL,
                [true; 2],
                Pressure::Busy,
                [8, 4],
                [32, 8],
            );
            let baseline = [tuner.models(), tuner.jobs()][lane];
            for window in 0..6 {
                for waiting in [true, false, true, true] {
                    for _ in 0..2 {
                        let timing = Timings {
                            service: Duration::from_millis(10),
                            ..Timings::default()
                        };
                        if lane == 0 {
                            tuner.record_model(Outcome::Successful, &timing, 1);
                        } else {
                            tuner.record_tool(Outcome::Successful, &timing, 1);
                        }
                    }
                    let mut demand = [false; 2];
                    demand[lane] = waiting;
                    let granted = if waiting {
                        [tuner.models(), tuner.jobs()]
                    } else {
                        [0; 2]
                    };
                    tuner.tick(
                        Duration::from_millis(500),
                        demand,
                        Pressure::Clear,
                        granted,
                        [32, 8],
                    );
                }
                assert_eq!(
                    [tuner.models(), tuner.jobs()][lane],
                    baseline + usize::from(window == 5),
                );
            }
        }
    }

    #[test]
    fn completed_window_requires_current_demand_to_start_probe() {
        for lane in 0..2 {
            let mut tuner = Tuner::new(4, 2);
            for _ in 0..3 {
                for waiting in [true, true, true, false] {
                    for _ in 0..2 {
                        let timing = Timings {
                            service: Duration::from_millis(10),
                            ..Timings::default()
                        };
                        if lane == 0 {
                            tuner.record_model(Outcome::Successful, &timing, 1);
                        } else {
                            tuner.record_tool(Outcome::Successful, &timing, 1);
                        }
                    }
                    let mut demand = [false; 2];
                    demand[lane] = waiting;
                    let granted = if waiting { [4, 2] } else { [0; 2] };
                    tuner.tick(
                        Duration::from_millis(500),
                        demand,
                        Pressure::Clear,
                        granted,
                        [32, 8],
                    );
                }
            }
            assert_eq!((tuner.models(), tuner.jobs()), (4, 2));
            assert!(tuner.evidence()[lane].is_some());
            assert!(tuner.model.probe.is_none());
            assert!(tuner.tool.probe.is_none());
        }
    }

    #[test]
    fn idle_probe_releases_other_lanes_recovery() {
        let mut tuner = tuner_with_probe();
        for window in 0..3 {
            for _ in 0..8 {
                tuner.record_tool(
                    Outcome::Successful,
                    &Timings {
                        service: Duration::from_millis(10),
                        ..Timings::default()
                    },
                    1,
                );
            }
            tuner.tick(
                super::super::CONTROL_INTERVAL,
                [false, true],
                Pressure::Clear,
                [0, tuner.jobs()],
                [5, 8],
            );
            assert_eq!(tuner.models(), 4);
            assert!(tuner.model.probe.is_none());
            assert_eq!(tuner.jobs(), 2 + usize::from(window == 2));
        }
    }

    #[test]
    fn nonclear_feedback_cancels_unaccepted_growth_before_window_end() {
        for pressure in [Pressure::Unknown, Pressure::Busy] {
            let mut tuner = tuner_with_probe();
            tuner.cancel_probes();
            assert_eq!((tuner.models(), tuner.jobs()), (4, 2));
            assert!(tuner.evidence()[0].is_none());
            tuner.tick(
                Duration::from_millis(500),
                [true; 2],
                pressure,
                [4, 2],
                [32, 8],
            );
            assert_eq!((tuner.models(), tuner.jobs()), (4, 2));
            assert!(tuner.model.probe.is_none());
            assert!(tuner.tool.probe.is_none());
            tuner.tick(
                Duration::from_millis(1_500),
                [false, true],
                Pressure::Clear,
                [4, 2],
                [32, 8],
            );
            let baseline = tuner.jobs();
            for _ in 0..6 {
                for _ in 0..8 {
                    tuner.record_tool(
                        Outcome::Successful,
                        &Timings {
                            service: Duration::from_millis(10),
                            ..Timings::default()
                        },
                        1,
                    );
                }
                tuner.tick(
                    super::super::CONTROL_INTERVAL,
                    [false, true],
                    Pressure::Clear,
                    [0, tuner.jobs()],
                    [32, 8],
                );
                if tuner.tool.probe.is_some() {
                    break;
                }
            }
            assert!(tuner.model.probe.is_none());
            assert!(tuner.tool.probe.is_some());
            assert_eq!(tuner.jobs(), baseline + 1);
        }
    }

    fn tuner_with_probe() -> Tuner {
        let mut tuner = Tuner::new(4, 2);
        for _ in 0..3 {
            for _ in 0..8 {
                let timing = Timings {
                    service: Duration::from_millis(10),
                    ..Timings::default()
                };
                tuner.record_model(Outcome::Successful, &timing, 1);
                tuner.record_tool(Outcome::Successful, &timing, 1);
            }
            tuner.tick(
                super::super::CONTROL_INTERVAL,
                [true, true],
                Pressure::Clear,
                [tuner.models(), tuner.jobs()],
                [32, 8],
            );
        }
        assert_eq!((tuner.models(), tuner.jobs()), (5, 2));
        assert!(tuner.evidence().iter().all(Option::is_some));
        tuner
    }

    #[test]
    fn busy_wake_invalidates_evidence_before_adjustment() {
        let mut tuner = tuner_with_probe();
        tuner.tick(
            Duration::from_millis(500),
            [false, false],
            Pressure::Clear,
            [5, 2],
            [32, 8],
        );
        assert!(tuner.evidence().iter().all(Option::is_some));
        for pressure in [Pressure::Busy, Pressure::Clear] {
            tuner.tick(
                Duration::from_millis(500),
                [true, true],
                pressure,
                [5, 2],
                [32, 8],
            );
            assert!(tuner.evidence().iter().all(Option::is_none));
            assert!(tuner.model.probe.is_none());
            assert_eq!((tuner.models(), tuner.jobs()), (5, 2));
        }
    }

    #[test]
    fn intermediate_grant_limits_restore_probe_baseline_before_retry() {
        let mut tuner = tuner_with_probe();
        for (sample, granted) in [[4, 2], [5, 1], [5, 2], [5, 2]].into_iter().enumerate() {
            tuner.observe_plan([tuner.models(), tuner.jobs()], granted, 1, true);
            tuner.tick(
                Duration::from_millis(500),
                [true, true],
                Pressure::Clear,
                granted,
                [32, 8],
            );
            assert_eq!((tuner.models(), tuner.jobs()), (4, 2));
            assert!(tuner.evidence()[0].is_none());
            assert_eq!(tuner.evidence()[1].is_some(), sample == 0);
        }
        for _ in 0..2 {
            for _ in 0..8 {
                tuner.record_model(Outcome::Successful, &Timings::default(), 1);
                tuner.record_tool(Outcome::Successful, &Timings::default(), 1);
            }
            tuner.tick(
                super::super::CONTROL_INTERVAL,
                [true, true],
                Pressure::Clear,
                [tuner.models(), tuner.jobs()],
                [32, 8],
            );
        }
        assert_eq!((tuner.models(), tuner.jobs()), (4, 2));
        assert_eq!(tuner.evidence()[0].as_ref().unwrap().target, 4);
        window(&mut tuner, 8, Duration::ZERO, 1);
        assert_eq!((tuner.models(), tuner.jobs()), (5, 2));
        assert_eq!(tuner.evidence()[0].as_ref().unwrap().target, 4);
    }

    #[test]
    fn pending_probe_waits_for_plan_and_donor_grants() {
        for denied in [false, true] {
            let mut tuner = tuner_with_probe();
            for (epoch, planned, refreshed) in [(10, 4, false), (10, 4, false), (11, 5, false)] {
                let planned = if denied && epoch == 11 { 4 } else { planned };
                tuner.observe_plan([5, 2], [planned, 2], epoch, refreshed);
                for _ in 0..32 {
                    tuner.record_model(Outcome::Successful, &Timings::default(), 1);
                }
                tuner.tick(
                    super::super::CONTROL_INTERVAL,
                    [true, false],
                    Pressure::Clear,
                    [4, 2],
                    [32, 8],
                );
                let clamped = denied && epoch == 11;
                assert_eq!(tuner.models(), if clamped { 4 } else { 5 });
                assert_eq!(tuner.model.probe.is_some(), !clamped);
                if !clamped {
                    assert_eq!(tuner.evidence()[0].as_ref().unwrap().target, 4);
                    assert_eq!(tuner.model.window.successes, 0);
                }
            }
            if denied {
                assert!(tuner.evidence()[0].is_none());
            } else {
                // Only fully installed, comparable work can validate growth.
                window(&mut tuner, 16, Duration::from_millis(10), 1);
                assert_eq!(tuner.models(), 5);
                assert!(tuner.model.probe.is_none());
                assert_eq!(tuner.evidence()[0].as_ref().unwrap().target, 5);
            }
        }
    }

    #[test]
    fn temporary_grant_waits_preserve_only_fully_granted_intervals() {
        for lane in 0..2 {
            let mut tuner = Tuner::new(4, 2);
            for eligible in [true, false, true, false, true] {
                for _ in 0..if eligible { 8 } else { 1_000 } {
                    let timing = Timings {
                        service: Duration::from_millis(if eligible { 10 } else { 1 }),
                        ..Timings::default()
                    };
                    if lane == 0 {
                        tuner.record_model(Outcome::Successful, &timing, 1);
                    } else {
                        tuner.record_tool(Outcome::Successful, &timing, 1);
                    }
                }
                let mut demand = [false; 2];
                demand[lane] = true;
                let mut granted = [tuner.models(), tuner.jobs()];
                granted[lane] -= usize::from(!eligible);
                tuner.tick(
                    super::super::CONTROL_INTERVAL,
                    demand,
                    Pressure::Clear,
                    granted,
                    [32, 8],
                );
            }
            assert_eq!([tuner.models(), tuner.jobs()][lane], [5, 3][lane]);
            let evidence = tuner.evidence()[lane].take().unwrap();
            assert_eq!(evidence.target, [4, 2][lane]);
            assert!((evidence.rate - 4.0).abs() < f64::EPSILON);
            assert!(evidence.p95_micros >= 10_000);
        }
    }

    #[test]
    fn expired_partial_windows_require_fresh_healthy_work() {
        let mut tuner = Tuner::new(4, 2);
        for _ in 0..2 {
            for _ in 0..8 {
                tuner.record_model(Outcome::Successful, &Timings::default(), 1);
            }
            tuner.tick(
                super::super::CONTROL_INTERVAL,
                [true, false],
                Pressure::Clear,
                [4, 2],
                [32, 8],
            );
        }
        tuner.tick(
            Duration::from_secs(18),
            [true, false],
            Pressure::Clear,
            [3, 2],
            [32, 8],
        );
        for fresh in 1..=3 {
            for _ in 0..8 {
                tuner.record_model(Outcome::Successful, &Timings::default(), 1);
            }
            tuner.tick(
                super::super::CONTROL_INTERVAL,
                [true, false],
                Pressure::Clear,
                [4, 2],
                [32, 8],
            );
            assert_eq!(tuner.models(), 4 + usize::from(fresh == 3));
        }
    }

    #[test]
    fn completed_trials_give_the_other_ready_lane_a_turn() {
        let mut tuner = tuner_with_probe();
        for control_window in 1..=9 {
            for _ in 0..8 {
                let timing = Timings {
                    service: Duration::from_millis(10),
                    ..Timings::default()
                };
                tuner.record_model(Outcome::Successful, &timing, 1);
                tuner.record_tool(Outcome::Successful, &timing, 1);
            }
            tuner.tick(
                super::super::CONTROL_INTERVAL,
                [true; 2],
                Pressure::Clear,
                [tuner.models(), if control_window >= 7 { 2 } else { 0 }],
                [32, 8],
            );
            assert!(tuner.models() <= 5);
        }
        assert_eq!(tuner.jobs(), 3, "tool recovery must receive a probe turn");
    }
}
