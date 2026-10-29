# Rust test reliability

The reliability workflow runs on main and daily. It runs the Rust workspace
tests twice through Cargo and twice through cargo-nextest 0.9.124, and reports
each test with both a failure and a pass. Every run must complete, and any
failure leaves the job failed. Doctests also run separately. The human-admitted gate
is unchanged; this workflow does not run unadmitted pull requests.

To reproduce locally, install the pinned tool with
`cargo install --locked --version 0.9.124 cargo-nextest`, then run
`python3 scripts/flake_check.py /tmp/harness-flakes` with a new output directory.
The detector retains JUnit reports and bounded logs locally. The workflow uploads
bounded inventory, JUnit reports, logs and summary for 14 days. Nextest runs individual
tests in separate processes, so it does not replace the existing Cargo gate or
cover interference between tests in one process. Logs are available in CI.

Nextest's platform-specific test list is the inventory for both reports. A
missing, filtered, ignored, skipped, or extra test makes the comparison
incomplete. The workflow provisions an isolated Linux Secret Service for the
real credential test and builds the native helper needed by executor tests.
Doctests run separately through Cargo. Each attempt checks all workspace library
targets, rejects ignored or filtered cases, and compares individual outcomes.
The separate scheduled registry job repeats Rust, schema, registry Worker and
authentication Worker tests. Each suite uses independent attempts and an
inventory; incomplete, skipped or failed cases keep the job failed. The job
uploads bounded summaries, including excerpts of Worker failure messages, and
schema outcomes.
Hosted timing, service cleanup and artifact upload need verification after push.
