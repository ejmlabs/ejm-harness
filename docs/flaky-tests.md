# Rust test reliability

The reliability workflow runs on main and daily. It runs the Rust workspace
tests twice through Cargo and twice through cargo-nextest 0.9.124, and reports
each test with both a failure and a pass. Every run must complete, and any
failure leaves the job failed. Doctests also run separately. The human-admitted gate
is unchanged; this workflow does not run unadmitted pull requests.

To reproduce locally, install the pinned tool with
`cargo install --locked --version 0.9.124 cargo-nextest`, then run
`python3 scripts/flake_check.py /tmp/harness-flakes` with a new output directory.
The bounded report transport requires POSIX and fails closed on Windows before
starting the report producer. Windows behavior has not been validated.
The detector retains JUnit reports and bounded logs locally. The workflow uploads
bounded inventory, JUnit reports, logs and summary for 14 days. Nextest runs individual
tests in separate processes, so it does not replace the existing Cargo gate or
cover interference between tests in one process. The detector also compares identifiable
per-test results from the two Cargo runs. Logs are available in CI.

Nextest's platform-specific test list is the inventory for both reports. A
missing, filtered, ignored, skipped, or extra test makes the comparison
incomplete. Oversized JUnit reports retain bounded test identities, favoring
failures and their earlier counterparts, and are marked incomplete. Bounded
summaries prioritize mixed and failed identities and mark any omitted results.
The workflow provisions an isolated Linux Secret Service for the
real credential test and builds the native helper needed by executor tests.
Doctests run separately through Cargo. Each attempt checks all workspace library
targets, rejects ignored or filtered cases, and compares individual outcomes.
Cargo and doctest outcomes are collected while output is read. Each attempt
retains at most 20,000 identities and 3 MiB of outcomes, favoring failures and
their counterparts from earlier attempts. If a result cannot fit, the run is
incomplete and omitted identities cannot be compared. Logs retain a bounded
prefix and tail plus excerpts around recognizable failure markers. Arbitrary
unmarked output between those excerpts can be lost, and log overflow fails the
run rather than claiming complete diagnostics.
The separate scheduled registry job repeats Rust, schema, registry Worker and
authentication Worker tests. Each suite uses independent attempts and an
inventory; incomplete, skipped or failed cases keep the job failed. The job
uploads bounded summaries, including excerpts of Worker failure messages, and
schema outcomes. Worker results are recorded as each test finishes, so an
overflow marks the attempt incomplete. Worker and schema reports prioritize
failed identities and their counterparts from earlier attempts, then retain
other identities in deterministic order. A mixed outcome can be unavailable
if its earlier passing result did not fit. Any omitted identity or truncated
diagnostic marks the attempt incomplete.
Duplicate tracking uses exact identities up to its bound and then a fixed-size
probabilistic filter. A collision can exclude an unrelated identity; it marks
evidence incomplete and does not report that identity as a comparable result.
Hosted timing, service cleanup and artifact upload need verification after push.
