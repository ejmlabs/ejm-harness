# Design

## Context

The engine publishes durable `RunFinished` before bounded run-end observers and active-map cleanup. `RunHandle::wait` already supplies the later completion barrier. Internal queued successors replace their completion channel inside the driver, so retaining only handles returned by `start` misses those successors.

## Goals / Non-Goals

Expose that existing exact-run barrier to native hosts. Preserve queue scheduling, journal order, cancellation and driver-drop errors. No protocol, provider or scheduler change is needed.

## Decisions

- Add `Harness::subscribe_run(session_id, run_id).await -> Option<RunHandle>`. Lock the active map, require both identifiers to match and clone the existing handle state. `None` means only that the exact run is no longer present.
- Keep one clone of the existing Tokio watch receiver in each active entry, and replace it when an internal queued successor begins. Keep no completion sender in the entry, so a driver that drops without a result still closes the channel.
- Derive `Clone` for `RunHandle`. All clones observe the same result and cancellation token; dropping them never cancels work. No extra task, channel, dependency or history map is added.
- Use the same watch payload for `RunCompletion { summary, queued_successor }`. Decide `queued_successor` from the next local prompt after draining steering and checking continuation under the active mutex, before notifying waiters. Add `wait_completion`; keep `wait` returning the same summary. The flag records this driver's transition, not current session idleness or authority for a later action.
- Hosts keep their own admission authority. A host may wait outside its own lock, then recheck revocation, expiry and target before starting dependent work. Session `Idle`, a durable event, sleeps and error-string checks are not completion barriers.

## Risks / Trade-offs

- A handle extends observation state until dropped. The engine keeps one receiver per already bounded active entry; consumers must bound retained handles.
- Another queued successor can become current immediately after completion. A host with revocable dependent work must discard it when `queued_successor` is true, even if that successor finishes before replay. Hosts must still perform normal admission and check exact targets after a false flag; direct starts outside their authority remain outside that host contract.
- The additive source API needs normal local and hosted checks and protected publication before a consumer pins it. Local preparation does not establish release or native acceptance.
