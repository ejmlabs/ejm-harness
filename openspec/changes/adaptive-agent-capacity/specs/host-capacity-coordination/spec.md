# Spec Delta

## Purpose

Share a local host resource budget across participating harness processes without double allocation, a permanent product broker, or a journal shared between owners. Use native supervision trees and measured recovery across macOS, Windows and Linux without a durable per-job execution ledger.

## ADDED Requirements

### Requirement: Participating processes share the applicable host envelope

Participating CLI and worker processes for the same OS user and host SHALL use one protected local registry independent of session-state directories. New grants and increases SHALL fit all applicable shared envelopes. An external limit decrease SHALL preserve existing charges as visible drain debt, prevent conflicting increases, and request reductions. Per-process restrictions SHALL remain enforced at their own scope.

#### Scenario: Two clients use different state directories

- **WHEN** both clients join automatic capacity on the same host and OS user
- **THEN** their grants charge the same applicable shared envelope
- **AND** separate journals cannot multiply the budget

#### Scenario: Processes run in nested constrained domains

- **WHEN** participants share a host or container ancestor but have different tighter local limits
- **THEN** every grant fits both the shared ancestor budgets and the participant's applicable limits
- **AND** unavailable domain visibility is reported instead of claiming coordination across an unseen boundary

#### Scenario: A container limit tightens while clients are busy

- **WHEN** the new effective limit falls below already committed shared grants
- **THEN** those grants remain charged and the excess is reported as drain debt
- **AND** conflicting new admission stops while cooperative reductions drain the debt without making released credits spendable twice

### Requirement: Grant transfers cannot double spend resources

Shared allocation SHALL be atomic and generation-checked. A recipient SHALL obtain charged credits before using them. A donor's credits SHALL remain charged until its local admission target has shrunk, outstanding reservations fit the smaller grant, and the donor has committed the release. Repeated or stale operations SHALL NOT create credits.

#### Scenario: Capacity moves between busy clients

- **WHEN** a coordinator requests credits from one client for another
- **THEN** the recipient waits for the donor's committed release
- **AND** running donor work remains charged throughout the transfer

#### Scenario: A client crashes during a grant change

- **WHEN** the allocation transaction or local acknowledgement is interrupted
- **THEN** recovery retains the last committed grant and safely repeats or rejects the operation
- **AND** no partial transfer becomes spendable twice

### Requirement: Resource-domain changes fence live admission

A detected change in applicable resource domains SHALL advance the environment generation and freeze affected admission. Transition accounting SHALL charge newly applicable ancestors before releasing old charges, counting common ancestors once. Insufficient new-domain capacity SHALL become visible drain debt. Unknown ancestry SHALL NOT create an independent root budget.

#### Scenario: A live process moves into a tighter cgroup

- **WHEN** its observed resource-domain identity changes while it owns grants
- **THEN** old-generation admission stops and the transition retains charges in all applicable domains until reconciled
- **AND** a fresh process registration cannot bypass the new ancestor limit

### Requirement: Recovery fences live scheduling authority

Heartbeat expiry SHALL stop new admission without releasing a live owner's grant. Recovery SHALL establish owner death from a retained OS lifetime witness or a verified boot change. A paused or uninspectable owner SHALL retain its grant. Verified death SHALL retire its future scheduling allowance under measured recovery without certifying descendant cleanup or replaying uncertain effects.

#### Scenario: A client pauses and misses heartbeats

- **WHEN** its heartbeat expires while its lifetime witness is still held
- **THEN** its grant remains charged and other clients cannot spend those credits
- **AND** it revalidates its generation before further admission after resuming

#### Scenario: An owner dies with uncertain descendants

- **WHEN** the OS lifetime witness establishes that the owner cannot admit more work
- **THEN** recovery retires its scheduling allowance and uses fresh host observations for conservative new admission
- **AND** possible survivors remain unverified external load rather than a proven empty scope
- **AND** interrupted actions are not replayed and workspace safety is not inferred

#### Scenario: The host reboots

- **WHEN** the recorded boot identity no longer matches the current host
- **THEN** former native owners cannot authorize new work
- **AND** unexpired provider charges survive until their independent reset windows

### Requirement: Host sharing is fair and bounded

Shared grants SHALL respond to bounded demand summaries with fair access between participating processes and finite progress for fitting older demand under sustained available capacity. Coordination SHALL exchange aggregate grants rather than per-token events or one registration per managed agent. Inactive participants SHALL cause no permanent monitor or service.

#### Scenario: One client submits thousands of agents

- **WHEN** another participant has fitting queued demand
- **THEN** the large client's agent count alone cannot claim the whole shared envelope indefinitely
- **AND** registry size and refresh work depend on bounded participants and active grants rather than total event volume

### Requirement: Local work uses a cached valid grant

Ordinary local leases SHALL use a cached committed grant and bounded contact deadline without per-operation registry I/O. Updates SHALL be limited to bounded registration, grant, demand, renewal, control and owner recovery. Native job lifecycles SHALL NOT require shared durable records. Expired grants SHALL stop new admission while retaining cancellation and cleanup.

#### Scenario: Many jobs reuse one shared process grant

- **WHEN** ordinary local leases acquire and release while that grant and contact deadline remain valid
- **THEN** they do not each read or write the shared registry
- **AND** expiry stops later admission even if the registry cannot be reached
- **AND** queued agents, native jobs and streamed events create no execution recovery ledger records

### Requirement: Coordination failures preserve existing ownership

When shared coordination is unavailable or incompatible, automatic mode SHALL stop new affected admission, retain running ownership and accounting, and report the condition. It SHALL NOT silently create a private replacement budget or replace an incompatible live registry. Cancelling and draining existing work SHALL remain available.

#### Scenario: Shared storage becomes busy or unreadable

- **WHEN** a participant cannot renew or validate the shared grant before its local contact deadline
- **THEN** new affected work waits with a coordination reason within its existing deadline
- **AND** input, cancellation, cleanup, and already-owned work continue without blocking on an unbounded registry wait

#### Scenario: A newer registry protocol is active

- **WHEN** an older participant cannot safely interpret that protocol
- **THEN** it reports incompatibility and does not reset shared state or fall back to independent automatic admission

### Requirement: Default activation requires a supported crash recovery path

Automatic coordinated defaults SHALL be enabled only after idle and busy owner-crash fixtures demonstrate safe recovery and useful progress within a documented bound on each claimed platform. The supported procedure SHALL preserve live commitments, distinguish measured recovery from cleanup proof, and SHALL NOT depend on deleting the registry or releasing a paused owner's grant. An unmet gate SHALL require lifecycle-design revision before activation.

#### Scenario: Busy-owner recovery cannot be established

- **WHEN** a claimed platform cannot recover enough safely accounted capacity after the busy-owner crash fixture
- **THEN** automatic-default activation fails its release gate
- **AND** the lifecycle design is revised instead of shipping indefinite stranded capacity as ordinary recovery

### Requirement: Native helpers have explicit lifetime ownership

Controlled native helpers SHALL have one lifetime owner and a private owner-loss link. Owner loss SHALL stop admission and trigger cancellation of known scopes. Native execution SHALL retain bounded live ownership through supported cleanup and expose platform capabilities truthfully. Waiting agents SHALL create no helper process. A VM, container runtime or durable per-job ledger SHALL NOT be required.

#### Scenario: A controlled helper loses its owner

- **WHEN** its private lifetime link closes
- **THEN** it stops new work and cancels and reaps its known child scopes
- **AND** unrelated descendants do not retain the owner's endpoint and suppress loss detection

#### Scenario: An arbitrary native child detaches on macOS

- **WHEN** it escapes its process group before both owner and observer die
- **THEN** recovery reports incomplete cleanup and resumes only under measured host admission
- **AND** it makes no claim of enforced descendant death, total memory bounds or workspace safety

### Requirement: Native owner replacement requires exclusive OS ownership

Execution owners SHALL share one stable exclusive OS ownership identity within a compatible user/resource domain. A paused owner SHALL keep ownership. Incompatible protocols or domains SHALL fail before launch. Owner replacement SHALL NOT occur merely because a heartbeat or request times out. An idle owner SHALL exit after owned work drains.

#### Scenario: Two clients start native work concurrently

- **WHEN** both request an execution owner in the same compatible scope
- **THEN** only one owner admits their work and the other launch attempt connects or exits
- **AND** a protocol mismatch does not create a parallel owner

#### Scenario: An owner is suspended

- **WHEN** new requests time out while the OS ownership witness remains held
- **THEN** callers remain bounded and cancellable without starting a second owner

### Requirement: Permanent native locks preserve one private identity

Permanent native locks SHALL use exclusive final-name creation under a pinned private directory and return the creator's verified handle. Existing entries SHALL pass regular-file, single-link and owner-only checks on the opened handle. Failed validation SHALL retain the name without repair or replacement. Windows creator handles SHALL omit DELETE access. Creation SHALL add no sync or power-loss claim.

#### Scenario: A peer opens the first visible permanent lock

- **WHEN** a creator pauses immediately after the final name becomes visible and before its post-create checks finish
- **THEN** a concurrent peer can open the same private single-link file and contend on the same OS lock identity
- **AND** no temporary second hard link or Windows DELETE access prevents that peer from opening it

#### Scenario: A creator dies after the permanent name becomes visible

- **WHEN** the creator process dies at that creation boundary
- **THEN** the final name remains one private file that a later client can open and lock
- **AND** recovery needs no unlink, replacement or elapsed-time inference

#### Scenario: Post-create privacy validation rejects a lock

- **WHEN** the newly created final file fails a same-handle owner-only privacy check
- **THEN** its name and identity remain in place
- **AND** a later strict reader rejects that unsafe entry without repairing or replacing it

### Requirement: The temporary filesystem repair has a verified removal path

The temporary `rightkit-fs` repair SHALL retain checksum-pinned archive, patch and source provenance, licenses and the dependency graph. Harness and path-based hosts SHALL use the same reviewed source. Cargo registry publication SHALL require a verified registry dependency. GitHub source and native builds MAY use the approved public `ejm-rightkit-fs` fork at an exact full commit SHA, with a repository-specific dependency allowlist and verified public consumers. 

#### Scenario: The maintained fork supplies the contract first

- **WHEN** ejmlabs publishes the reviewed patch under `ejm-rightkit-fs`
- **THEN** consumers use an exact registry version and distinct package identity with retained licenses and source provenance
- **AND** the upstream contribution and migration path remain recorded
- **AND** an exact Git pin may support source/native builds but does not satisfy registry publication

#### Scenario: A maintained upstream release provides the same lock contract

- **WHEN** that release passes deterministic lock concurrency, creator-death and privacy checks and public consumer checks
- **THEN** the upstream version replaces the temporary vendor override and patch
- **AND** the provenance record retains the verification and licensing history

### Requirement: Native launch attempts are generation scoped

Native requests SHALL use an owner generation and bounded operation identity. Reconnection SHALL query only the same live generation. A replacement owner SHALL reject uncertain earlier launches and expired operation retries. Private input and the caller's environment SHALL remain bounded and absent from shared recovery metadata.

#### Scenario: A launch reply is lost

- **WHEN** a caller reconnects to the same live owner
- **THEN** it can reconcile the original admitted operation without starting it again
- **AND** replacement by another owner does not authorize replay

### Requirement: Native scopes retain resource and mutation ownership

Execution ownership SHALL include child processes, pipes, resource allowances and workspace mutation guards through supported cleanup. Resident package generations SHALL retain their idle memory and process charges and account for reload overlap. Caller environment and resource-domain semantics SHALL be preserved or rejected explicitly before effects.

#### Scenario: A caller dies during package import

- **WHEN** its controlled runtime owner detects lifetime-link closure
- **THEN** that owner cancels known startup/import work and retains the protected scope through cleanup
- **AND** possible escaped native effects remain uncertain if the execution owner also dies

#### Scenario: A client has incompatible placement constraints

- **WHEN** the shared owner cannot preserve the client's applicable native resource domain
- **THEN** execution rejects the unsupported placement before launch
- **AND** it does not silently inherit another client's execution domain

### Requirement: Native diagnostics have no launch side effect

Public capability and cached status reads SHALL report native admission, lifetime links, cancellation and cleanup limits without starting helpers, installing software or changing host limits. Automatic defaults SHALL depend on actual platform recovery evidence and measured owner cost.

#### Scenario: A user inspects doctor before native work

- **WHEN** the host reads native capability and capacity status
- **THEN** no execution owner or tool process is started by that read
- **AND** unsupported or unverified restrictions remain explicit

### Requirement: Coordination has a truthful local trust scope

The registry SHALL use owner-only local state and bounded nonsecret metadata. Status SHALL distinguish participating-process reservations from observed external load and identify the coordination scope. Other users, inaccessible containers, legacy clients, and unrelated applications SHALL NOT be described as reserved participants.

#### Scenario: Another application consumes memory

- **WHEN** nonparticipating work increases host pressure
- **THEN** operating targets react to supported observations while status keeps that load distinct from shared grants
- **AND** coordination makes no claim of kernel-enforced memory or CPU isolation

### Requirement: Shared native helpers retain separate capacity commitments

Each coordinated native helper generation SHALL obtain one coarse shared
capacity commitment before process creation. Compatible clients SHALL share
that commitment. The registry SHALL bound occupied helper slots, including
pending startup and draining generations. Helper allowances SHALL charge
every applicable ancestor once and SHALL remain independent of client grants.
No helper allowance SHALL be reserved before native demand.

#### Scenario: Clients share one compatible helper

- **WHEN** several participating clients use the same compatible execution owner
- **THEN** its baseline is charged once across their applicable shared envelopes
- **AND** commands, callbacks and streamed events create no helper records

#### Scenario: A constrained client uses only remote models

- **WHEN** it creates no native helper
- **THEN** no helper baseline is deducted from its applicable envelopes
- **AND** unused helper pool slots do not consume its memory grant

#### Scenario: First native requests saturate participant grants

- **WHEN** a helper commitment needs capacity held by participant grants
- **THEN** admission obtains that capacity through the existing reduction,
  drain and committed-release rules before helper process creation
- **AND** requests do not retain dependent execution reservations in a cycle
  that prevents the helper commitment from fitting

#### Scenario: Distinct helper startups compete for one useful turn

- **WHEN** several pending helper allowances would consume the room needed by every staged dependent request
- **THEN** plans select a safe subset of pending allowances that preserves a feasible claimant through actual drain and committed release
- **AND** an ineligible pending allowance occupies its bounded slot without subtracting its baseline from all participant plans

#### Scenario: An older compatible client waits on a younger startup

- **WHEN** an older staged client shares the native domain of another client's pending helper
- **THEN** the pending helper can protect the older client's useful turn as its selected beneficiary
- **AND** the helper allowance is counted once while the initiating client obtains the startup permit
- **AND** neither client must wait for the other to spend a grant before that shared helper can start

#### Scenario: An idle helper is reused for a larger request

- **WHEN** existing helper baselines prevent the newly staged request from activating
- **THEN** no caller retains unused readiness across that resource wait
- **AND** idle helpers can exit normally so witness recovery and shared replanning restore useful progress without resetting the staged ticket

#### Scenario: Retained residents or ancestor limits change

- **WHEN** later resident work or a tighter ancestor changes the next feasible working set
- **THEN** plans use current aggregate demand and preserve every actual helper and participant charge as needed
- **AND** job completion does not simulate release of a shared helper or count a working-set hint as another actual reservation

#### Scenario: A helper occupies a nested constrained domain

- **WHEN** its baseline commitment is admitted
- **THEN** that commitment fits every applicable ancestor envelope
- **AND** sharing clients and common ancestors do not multiply its charge

### Requirement: Helper startup commitments have kernel lifetime witnesses

A helper startup commitment SHALL retain a supported OS witness from before
process creation through helper exit. The witness SHALL survive launcher
death and remain excluded from user descendants. Timeout, heartbeat expiry,
launcher death and failed replies SHALL NOT release a held commitment.
Release SHALL require supported witness-release evidence or a verified boot
change. Unsupported platforms SHALL reject shared startup before effects.

#### Scenario: The launcher dies before process creation

- **WHEN** no helper was created and the startup witness is released
- **THEN** recovery can retire the unused helper commitment

#### Scenario: The launcher dies after process creation

- **WHEN** the helper retains the inherited witness
- **THEN** its commitment remains charged through startup, work, cleanup and exit
- **AND** pausing the helper before dispatch does not permit replacement

#### Scenario: A user command outlives its helper

- **WHEN** helper ownership is released
- **THEN** the command cannot retain the helper witness
- **AND** witness release does not certify arbitrary descendant cleanup

#### Scenario: An older helper lacks the accounting contract

- **WHEN** a client discovers that incompatible helper
- **THEN** it rejects reuse and preserves the stable ownership identity
- **AND** it does not start a parallel owner to bypass the incompatibility
