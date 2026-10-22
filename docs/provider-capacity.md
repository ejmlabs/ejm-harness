# Provider capacity

Coordinated harnesses can share provider in-flight, request-rate, and token-rate
capacity on one host. The source includes this path and offline fixtures. Release,
live-provider acceptance, and throughput on each supported machine remain separate
checks. Existing uncoordinated constructors and custom providers keep their APIs.

Provider limits constrain model work independently of CPU, local execution,
context memory, tool authorization, and managed session count. A free model slot
does not make an exhausted rate budget available.

## Declare the shared account

HTTP providers accept `.with_capacity(ProviderCapacity)` without changing existing
configuration literals. `QuotaDomain::new(origin, account_uuid, family)` builds an
opaque domain from the HTTP origin, a host-assigned nonsecret account UUID, and an
explicit quota family. Reuse the UUID and family across cooperating processes
which share an account quota. Different model names do not establish independent
quotas. Credentials, prompts, session paths, and raw HTTP material must never
supply the account identity.

```rust,no_run
use ejm_harness::{ProviderCapacity, ProviderRate, QuotaDomain};

fn shared_policy() -> anyhow::Result<ProviderCapacity> {
    Ok(ProviderCapacity {
        domain: Some(QuotaDomain::new(
            "https://api.openai.com",
            "00000000-0000-0000-0000-000000000001",
            "shared",
        )?),
        max_in_flight: Some(8),
        requests: Some(ProviderRate { limit: 120, window_ms: 60_000 }),
        tokens: Some(ProviderRate { limit: 120_000, window_ms: 60_000 }),
    })
}
```

The UUID above is illustrative. Generate and retain an account UUID in the host's
account settings. `OpenAiProvider`, `AnthropicProvider`, and `ResponsesProvider`
accept the policy through their builder. `Harness::with_capacity` configures the
provider gate from `Provider::capacity()`. Hosts which prepare package providers
early may call `CapacityContext::configure_provider` before final composition.
Changing a policy requires idle streams and quota waits. Uncommitted throttle or
usage feedback must reach shared coordination before a different policy can
replace it. Reapplying the same policy preserves cooldown and charges.

`max_in_flight` accepts 1 through 256. Each explicit rate has a positive `limit`
and a `window_ms` from 1000 through 86,400,000. Processes sharing a domain use the
smallest declared ceiling. Different explicit periods for the same resource and
domain reject with an actionable configuration error, including while old
charges remain unexpired. Use one agreed rate period for each quota family.

Without a domain, supported feedback and configured rates apply locally and
status reports an uncoordinated provider scope. This cannot promise a shared
account budget. The host registry covers participating processes for the same
user and visible host scope. Other users, machines, external clients, and remote
execution lifetimes are outside that guarantee.

## Typed feedback and supported headers

`Provider::stream_with_capacity` is additive. Its default calls the existing
`stream`, so a custom provider without observations remains usable. A custom
provider can implement `capacity()` and send validated aggregates to
`ProviderCapacityObserver`. The observer must remain bounded and perform no
registry or network I/O. A callback after its request lease ends is ignored.

The three built-in HTTP transports provide typed feedback without enabling HTTP
debug capture:

- Status 429 or 503 reports throttling. The failed request is not retried.
- Standard `Retry-After` accepts integer seconds and HTTP dates. A valid deadline
  longer than the 15 minute local wait horizon remains in force and fails the
  waiting run with guidance. An unrepresentable deadline blocks until an explicit
  host reset. An unknown or malformed hint uses a conservative ten second pause.
- OpenAI-style `x-ratelimit-{limit,remaining,reset}-{requests,tokens}` and
  Anthropic-style `anthropic-ratelimit-{requests,tokens}-{limit,remaining,reset}`
  accept a complete bounded tuple. Reset accepts epoch seconds or an HTTP date,
  at most one day ahead. Vendor duration strings and RFC3339 reset values remain
  unknown. Missing or unsupported hints never become an invented rate limit.
- Usage is eligible for reconciliation only after complete successful decoding
  and checked totals. Cached input and reasoning output must remain subsets of
  their respective totals. Missing, malformed, aborted, and uncertain usage
  retains the conservative charge.

Headers are not copied into learned state. Capacity feedback retains only opaque
identity, integer ceilings, usage totals, deadlines, and aggregate timing/outcome
information. Error bodies and human-readable error strings do not drive quota
admission.

## Prepaid grants and conservative accounting

The coordinator commits bounded prepaid rate blocks before installing local
credits. Ordinary request admission, stream callbacks, and lease release use
cached memory and do not write SQLite. A block covers prepared demand and up to
2.5 seconds of the known request/token rate, capped at 4096 requests and the
estimated cost of 4096 copies of the largest waiting request. This lets short
requests reuse prepaid capacity across model-slot waves between coordinator
cycles. Available shared quota and competing prepared demand still bound every
grant. Context and request preparation already hold host admission; there is at
most one prepared quota waiter per admitted model slot.

Request cost counts serialized request bytes without allocating a second request
buffer, then adds maximum output tokens and a 256-token margin. It is a
conservative estimate rather than a provider-specific tokenizer. Large estimates
can reduce token-limited throughput. An estimate above an explicit token ceiling
fails before sending. Providers may have additional undocumented accounting.

Prepaid credits last at most 2.5 seconds for explicit rolling rates. Renewal keeps
the oldest pool expiry while unused credits remain. The ledger charges through
the last possible admission time plus the complete rate period, using at most
64 conservative expiry buckets per window. It does not reset rolling limits at
calendar boundaries. Provider absolute-reset hints retain their actual reset.

Validated usage can return unused estimated tokens to the same unexpired pool
and epoch. When a long response finishes after its pool expires, its full estimate
remains charged until the rate expiry. Excess actual usage becomes durable token
debt even without another waiter and survives owner cleanup. Cancellation or
process death never refunds uncertain provider consumption. These choices favor
correct accounting and can leave quota unused. A 60-second explicit period adds
up to about 7.3% conservative holding from the prepaid lifetime and bucket rounding
alone. Short periods can lose substantially more capacity under the coordinator's
500 ms minimum cadence. These are accounting bounds, not measured throughput
claims; the benchmark must report any acceptance miss.

In-flight grants bound locally tracked streams. Lower targets install before
released slot charges become available elsewhere, and active streams retain
their charge. Quarantined owners remain charged. A client exit or host reboot
cannot prove that remote inference stopped; rate charges survive independently
until their actual expiry or reset.

## Cooldown, cancellation, and recovery

A shared throttle stops future admissions in its quota domain. After the deadline,
shared re-entry starts with one in-flight slot and adds at most one slot every
500 ms until the configured ceiling. Unrelated domains can continue within host
budgets. Existing failed requests and tool effects are never replayed.

Cancellation and run deadlines remain active while waiting. A cancelled waiter
leaves the queue without sending a later request. First quota hints, hints below
the cached unused domain reservation, and throttles fence new admission until
coordination commits them. Generous hints coalesce conservatively in bounded
memory while existing prepaid credits remain usable. Installing a larger grant
also rechecks feedback received during its transaction. Failed coordination
retains feedback and allows only previously committed cached admission within
those fences and its contact deadline. It does not create an independent fallback
budget.

`CapacityContext::snapshot().provider` reports opaque domain, coordination state,
target, local streams, waiting count, prepaid credits, excess usage debt, cooldown,
and block reason. A lower remote hint can create drain debt against previously
issued credits. Those charges stay intact and peers learn the changed epoch on
their next bounded renewal. Cached grants cannot instantly revoke another process.

A bounded feedback queue holds at most 256 observations and merges quota hints
using the smallest remaining amounts and latest reset. Overflow blocks the shared
domain rather than silently forgetting a throttle. An owner-authorized
`CapacityContext::clear_provider_cooldown()` clears a block or cooldown through
the shared coordinator. Newer throttle feedback remains effective. It preserves
request/token charges and does not repeat previous work. Keep this method in the
host's local account controls; it is not a tool or remote-controller action.
Do not delete the ledger to recover a provider limit.

## Offline verification

Focused fixtures cover typed HTTP feedback, incomplete usage, long and unknown
retry hints, shared and unrelated domains, rolling boundaries, stale slot
release, explicit-reset races, cancellation, feedback/write failures, excess
usage debt, a thousand short cached requests with generous quota hints, tighter
hint fencing, and real concurrent processes sharing one SQLite ledger. They
verify local contracts without paid calls. Native platform lifecycle, third-party quota
semantics, live account sharing, and fixed-versus-auto throughput need separate
evidence. See [benchmark evidence](benchmarks.md) and the
[SDK quota configuration](sdk.md#capacity-and-bounded-session-views).
