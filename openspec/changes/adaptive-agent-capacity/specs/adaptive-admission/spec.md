# Spec Delta

## Purpose

Find a sustainable operating range for agent work from real demand and system feedback while preserving explicit safety budgets and fair progress.

## ADDED Requirements

### Requirement: Automatic policy uses the effective resource envelope

Automatic admission SHALL account for supported host and container CPU, memory, process and descriptor constraints, external load, and foreground headroom. It SHALL distinguish measured, estimated, stale, and unavailable observations and SHALL use a conservative fallback when required observations are missing.

#### Scenario: A container has tighter limits than its host

- **WHEN** a process can observe applicable container limits below host capacity
- **THEN** automatic admission uses the tighter constraints and reports their source
- **AND** installed RAM or logical CPU count alone cannot override those constraints

#### Scenario: Observation is unavailable or stale

- **WHEN** a capacity observation fails or exceeds its freshness bound
- **THEN** the controller stops upward probing that depends on it and reports degraded observation
- **AND** it does not interpret the missing value as zero pressure

### Requirement: Operating targets remain separate from hard ceilings

Automatic policy SHALL change operating targets beneath owner-controlled hard ceilings. A lower target SHALL stop conflicting new admission while existing leases drain. Tuning SHALL NOT raise tool or shell authorization grants, deadlines, token budgets, artifact quotas, or per-command resource reservations on behalf of an agent.

#### Scenario: Capacity falls with valid work running

- **WHEN** pressure lowers a target beneath current reservations
- **THEN** valid running work retains its accounting and drains under its existing cleanup rules
- **AND** no new conflicting work starts until it fits both the target and hard envelope

#### Scenario: An agent requests more resources

- **WHEN** model output asks the controller to raise a safety ceiling or tool grant
- **THEN** the request cannot change that ceiling or grant

### Requirement: Feedback seeks useful throughput with stable control

The controller SHALL seek the lowest concurrency within the documented tolerance of the best observed successful throughput while meeting latency and resource budgets. It SHALL increase only under real queued demand with sufficient healthy evidence, reduce on pressure, and use bounded steps, hysteresis, and cooldown. It SHALL distinguish queue, provider, journal, and execution time.

#### Scenario: More concurrency yields no useful gain

- **WHEN** a probe adds resource use or tail latency without a meaningful increase in successful work
- **THEN** the controller returns to the lower sustainable target
- **AND** retries, failures, cancelled work, and output volume do not count as added throughput

#### Scenario: Provider time rises without local pressure

- **WHEN** a remote provider slows while local execution remains healthy
- **THEN** the controller attributes the delay to the model lane rather than treating it as proof of local CPU shortage

#### Scenario: No work is waiting

- **WHEN** demand is below the current operating target
- **THEN** the controller performs no synthetic model or tool work to explore capacity

### Requirement: Resource lanes share one memory account

Baseline runtime memory, context preparation, model buffers, native-tool reservations, retained output, and package hosts SHALL consume one accounted process memory grant. Fixed headroom SHALL be reserved once. Model concurrency credits SHALL NOT independently grant the same memory, and sampled available memory SHALL NOT cause live reservations to be subtracted twice.

#### Scenario: Models and native tools compete for memory

- **WHEN** large contexts and native jobs request admission together
- **THEN** their combined estimates and retained state fit the same memory grant after one headroom reservation
- **AND** separate model and tool count limits cannot each spend that full memory grant

### Requirement: Native preparation stages demand without holding resources

Coordinated native and package preparation SHALL stage bounded local demand in the existing governor before establishing helper readiness. A staged request SHALL expose its useful total working set through aggregate admission demand without holding a resource lease. It SHALL retain its original ticket, within-session FIFO, age, bounded bypass, cancellation and overall deadline. Queue metadata SHALL use existing count and byte limits.

#### Scenario: A staged native request waits for shared capacity

- **WHEN** its local grant cannot yet activate the requested working set
- **THEN** aggregate demand includes the retained resident floor and next useful request
- **AND** the waiter holds no job, resident or memory lease for that request and no unused helper keepalive

#### Scenario: A staged caller is cancelled or dropped

- **WHEN** cancellation, its deadline or handle drop occurs before activation
- **THEN** its bounded queue entry is removed promptly without later execution
- **AND** real work and helper commitments retain their independent ownership and accounting

### Requirement: Staged activation preserves queue and grant identity

Activation SHALL atomically validate the staged ticket, eligibility and expected grant generation before converting it into actual leases. A failed activation SHALL retain queue order without spending resources. Callers SHALL release unused native readiness before any further resource wait. Preparation retries SHALL occur only before launch effects, and a returned readiness guard SHALL remain bound to its original helper domain and epoch.

#### Scenario: The grant changes during helper preparation

- **WHEN** readiness completes after the staged request loses eligibility or its grant generation changes
- **THEN** atomic activation leaves the original ticket queued and creates no lease
- **AND** the caller drops unused readiness before waiting again

#### Scenario: A younger request fits during repeated preparation

- **WHEN** an older staged request must prepare again before activation
- **THEN** it retains its original age and FIFO position
- **AND** repeated preparation cannot reset its starvation protection

### Requirement: Coordinated startup precedes package execution

Automatic coordinated hosts SHALL prepare one public capacity context before language-host spawn, imports or heavy context allocation and reuse it through final harness construction. Startup, reload and persistent host memory SHALL share later agent work's grant and governor. Idle hosts SHALL retain memory charges without active job slots. APIs SHALL be additive.

#### Scenario: A package provides the selected model or hook

- **WHEN** a host loads that package before constructing the final harness
- **THEN** package import and lazy Node/Python startup obtain admission from the prepared context
- **AND** final harness construction reuses that context instead of creating an independent governor or memory budget

#### Scenario: Startup admission waits or is cancelled

- **WHEN** a coordinated package load cannot obtain startup capacity or is cancelled while waiting
- **THEN** it cannot start a language child without the required reservation
- **AND** cancellation remains responsive without waiting for final harness construction

#### Scenario: Package startup or final composition fails

- **WHEN** import, reload or final harness construction fails after native work starts
- **THEN** native ownership and its memory charge remain until cleanup establishes release
- **AND** reload accounts for overlapping old and replacement hosts without retaining an active job slot for an idle host

#### Scenario: Package startup converts its combined working set

- **WHEN** a new interpreter stages 64 MiB resident memory and 16 MiB management scratch
- **THEN** activation accounts for their combined 80 MiB above any retained old interpreter floor
- **AND** conversion transfers those same bytes once into resident and management leases, retaining real ownership through cleanup

### Requirement: Mixed work progresses without starvation

Automatic admission SHALL preserve FIFO within each session and fair rotation between sessions. It SHALL bound how often later fitting work bypasses an older blocked session head, then reserve progress for that head. A request that fits the hard envelope SHALL have a finite path to admission when sustained capacity permits it.

#### Scenario: Heavy work waits beside continuous small jobs

- **WHEN** an old heavy request can fit once conflicting work drains and small requests keep arriving
- **THEN** small work can use otherwise idle capacity only within the bounded bypass policy
- **AND** conflicting new work eventually pauses so the old request can start

#### Scenario: The head cannot fit the hard envelope

- **WHEN** a request exceeds a hard resource ceiling
- **THEN** admission rejects it with the limiting dimension instead of blocking the queue indefinitely

### Requirement: Capacity decisions are observable without hot-path tracing

Public capacity status SHALL report mode, hard ceilings, operating targets, running and waiting work by lane, limiting reasons, observation source and freshness, and coordination availability. Status collection SHALL use bounded aggregate data and SHALL NOT require retaining per-agent telemetry or enabling HTTP debug capture.

#### Scenario: Work waits under memory pressure

- **WHEN** a host reads capacity status while memory prevents admission
- **THEN** it can identify the affected lane, target, ceiling, and memory-related reason
- **AND** unknown observed usage remains distinguishable from zero usage

### Requirement: Existing fixed integrations remain compatible

Existing constructors, configuration literals, provider implementations, event subscribers, and fixed admission behavior SHALL remain usable without adopting automatic policy. Automatic policy SHALL be available through additive public contracts and SHALL NOT require a consuming frontend dependency.

#### Scenario: An existing host uses its current constructor

- **WHEN** a host builds against the new harness without enabling automatic capacity
- **THEN** its existing configuration and fixed concurrency semantics remain valid
- **AND** no shared capacity registry is opened solely by inspecting static executor capabilities
