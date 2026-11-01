import { DocBody, type DocPage } from "@ejm/ui";
import { InlineCode, InlineLink } from "@ejm/ui/layout/SiteContent";
import { head } from "./host-auth-head";

const page: DocPage = {
    ...head,
    sections: [
        {
            id: "status",
            title: "Availability",
            blocks: [
                {
                    type: "p",
                    children: (
                        <>
                            These APIs are prepared for source review. This guide does not announce
                            a package release, native platform certification or live service. Source
                            checks and local previews do not prove that this page has been deployed.
                        </>
                    ),
                },
            ],
        },
        {
            id: "setup",
            title: "Key service setup",
            blocks: [
                {
                    type: "p",
                    children: (
                        <>
                            The vault key stays in macOS Keychain, Windows Credential Manager or an
                            unlocked Linux Secret Service in the current user's D-Bus session. Linux
                            development builds require <InlineCode>{"pkg-config"}</InlineCode> and
                            D-Bus development headers. Debian and Ubuntu use the packages{" "}
                            <InlineCode>{"pkg-config"}</InlineCode> and{" "}
                            <InlineCode>{"libdbus-1-dev"}</InlineCode>.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            Headless Linux users must provision and unlock a user Secret Service.
                            Missing or locked key services return an error. The harness never writes
                            an unprotected key beside the vault or changes to plaintext storage.
                            Hosts can offer explicit{" "}
                            <InlineCode>{"FileStore::memory()"}</InlineCode> or{" "}
                            <InlineCode>{"SecretVault::memory()"}</InlineCode>; these do not persist
                            or import credentials.
                        </>
                    ),
                },
            ],
        },
        {
            id: "vault",
            title: "Vault APIs and limits",
            blocks: [
                {
                    type: "p",
                    children: (
                        <>
                            <InlineCode>{"FileStore::open(path)"}</InlineCode> remains the
                            provider-state entry point and adapts the encrypted vault. Generic host
                            secrets use <InlineCode>{"SecretVault::open(path)"}</InlineCode> with{" "}
                            <InlineCode>{"SecretId::new(product, purpose, identity)"}</InlineCode>.
                            Include the account and normalized origin in the identity where needed.
                            Products can share the implementation while using separate physical
                            vaults.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            <InlineCode>{"lock(cancel).await"}</InlineCode> returns an exclusive
                            transaction. Use <InlineCode>{"get"}</InlineCode>,{" "}
                            <InlineCode>{"set"}</InlineCode>, <InlineCode>{"delete"}</InlineCode>,{" "}
                            <InlineCode>{"migrated"}</InlineCode> and{" "}
                            <InlineCode>{"mark_migrated"}</InlineCode> as needed, then call{" "}
                            <InlineCode>{"commit"}</InlineCode> explicitly. Dropping without commit
                            does not save mutations. <InlineCode>{"VaultKeyStore"}</InlineCode>{" "}
                            supplies an injected protected-key boundary for hosts and offline
                            fixtures. Synchronous <InlineCode>{"lock_blocking"}</InlineCode> and
                            registry operations belong off an async or UI event thread.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            Async lock acquisition stops on cancellation or after 30 seconds from
                            the initial request, including native key access. An already-started OS
                            key call cannot be stopped: its worker keeps the stable file lock until
                            it returns and checks cancellation and the deadline before later changes
                            or transaction delivery. A cancelled first key save may still finish;
                            preserve its bootstrap marker so retry reuses that key. Runtime shutdown
                            can wait for the native call. Synchronous{" "}
                            <InlineCode>{"lock_blocking"}</InlineCode> cannot interrupt OS key calls
                            and belongs off async and UI event threads.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            Provider state is limited to 1 MiB and 64 registrations. The vault
                            permits 128 generic secrets of 16 KiB each, 128 permanent migration
                            markers, 4 MiB total plaintext and 8 MiB encrypted input. Corrupt,
                            substituted, unsupported or oversized state fails before credentials
                            become available.
                        </>
                    ),
                },
            ],
        },
        {
            id: "migration",
            title: "Migration and recovery",
            blocks: [
                {
                    type: "p",
                    children: (
                        <>
                            A valid private legacy provider <InlineCode>{"state.json"}</InlineCode>{" "}
                            migrates under its existing stable <InlineCode>{".lock"}</InlineCode>.
                            The store commits and reopens encrypted state before publishing a
                            nonsecret version barrier that old readers reject. Preserve that barrier
                            and lock. A completed migration refuses stale plaintext snapshots and
                            missing ciphertext.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            Registry sessions use <InlineCode>{"save_registry_token"}</InlineCode>,{" "}
                            <InlineCode>{"load_registry_token"}</InlineCode> and{" "}
                            <InlineCode>{"delete_registry_token"}</InlineCode> with the exact
                            registry origin. The encrypted store lives under the package root's{" "}
                            <InlineCode>{"credentials"}</InlineCode> directory. Legacy plaintext is
                            never an automatic fallback.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            Stop older registry clients before calling{" "}
                            <InlineCode>{"migrate_registry_token(root, origin, true)"}</InlineCode>.
                            That call imports a legacy file only when the file's origin matches the
                            requested origin. A session for the retired meetgerry.ai registry is
                            never saved, loaded or imported. An unconfirmed handoff fails before
                            source changes. Migration verifies privacy and origin, commits a
                            permanent destination marker, reopens encrypted state and then retires
                            the source. Retry interrupted cleanup; logout and replacement retain the
                            marker so stale credentials cannot return.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            When key access fails, preserve the vault and unlock or restore the
                            original OS key record. Do not delete ciphertext, barriers or bootstrap
                            markers to suppress an error. Missing or replaced keys never trigger an
                            automatic reset. This version has no passphrase recovery, cloud sync or
                            portable backup format. Moving a vault changes its authenticated path
                            identity.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            Use trusted local storage. Unix files require private owner permissions;
                            macOS extended allow ACLs are refused. Windows uses protected native ACL
                            and handle APIs. Native Windows acceptance remains pending. OS-user
                            protection does not isolate arbitrary code running as the same user or
                            prevent replay of an entire valid older vault. Deleting a legacy file
                            does not erase snapshots or storage hardware.
                        </>
                    ),
                },
            ],
        },
        {
            id: "claude",
            title: "Official Claude Code access",
            blocks: [
                {
                    type: "p",
                    children: (
                        <>
                            Install an unmodified official Claude Code client separately.{" "}
                            <InlineCode>{"resolve_claude_code_program"}</InlineCode> preserves an
                            explicit executable override, then checks the user's native installation
                            and PATH. Use an absolute executable path or a bare program name.
                            Automatic Windows discovery selects{" "}
                            <InlineCode>{"claude.exe"}</InlineCode>.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            <InlineCode>{"ClaudeCodeClient"}</InlineCode> delegates sign-in, bounded
                            status and sign-out to the official client. It never extracts
                            subscription tokens. Suspend the host's terminal reader before
                            interactive sign-in and restore it after the operation finishes. Sign-in
                            has a ten-minute deadline; captured status and sign-out use fifteen
                            seconds. Cancellation and dropped callers retain child cleanup
                            ownership. Successful captured Unix cleanup retires its numeric group
                            identity after termination and child reaping, before later output-drain
                            waits. Pending cleanup and errors retain ownership; Windows Job behavior
                            is unchanged.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            <InlineCode>{"ClaudeCodeProvider::new(config, executor)"}</InlineCode>{" "}
                            uses the harness's shared executor governor. Before each inference it
                            requires the official client's subscription source, clears known API-key
                            and routing overrides and rejects other billing sources.{" "}
                            <InlineCode>{"CLAUDE_CONFIG_DIR"}</InlineCode>, when set, must be
                            absolute. Existing Anthropic and OpenAI-compatible API-key routes remain
                            separate.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            Readable conversation, system instructions and tool definitions travel
                            over private stdin, capped at 8 MiB. The provider disables ordinary
                            native tools, MCP tools, customizations and session persistence. Managed
                            provider policy can still run hooks. Native execution is not a sandbox.
                            The harness validates proposed tools through its normal grant and
                            execution loop.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            The adapter emits text after the complete structured reply passes
                            validation. It has process startup cost and no token streaming. Output
                            allows at most 64 KiB of text and sixteen tool proposals within a
                            configured total budget of at most 1 MiB. Failed, cancelled, timed-out,
                            malformed or truncated results stop the request without exposing raw
                            output.
                        </>
                    ),
                },
            ],
        },
        {
            id: "provider",
            title: "Provider changes between turns",
            blocks: [
                {
                    type: "p",
                    children: (
                        <>
                            <InlineCode>{"Harness::with_optional_provider"}</InlineCode> lets a host
                            open before sign-in. A missing provider rejects a submitted turn before
                            journaling its prompt.{" "}
                            <InlineCode>{"configure_provider(provider, binding).await"}</InlineCode>{" "}
                            changes or removes the shared provider only while all managed work and
                            queues are idle.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            The host supplies a stable, nonsecret binding of 1 to 1024 printable
                            UTF-8 bytes covering provider, account, model and endpoint. A changed
                            binding atomically removes opaque continuation while retaining readable
                            history and tool evidence. The same binding preserves continuation
                            across restart. Failed validation or persistence leaves the previous
                            configuration intact.
                        </>
                    ),
                },
            ],
        },
        {
            id: "completion",
            title: "Exact run completion for hosts",
            blocks: [
                {
                    type: "p",
                    children: (
                        <>
                            The prepared source API{" "}
                            <InlineCode>
                                {"Harness::subscribe_run(session_id, run_id).await"}
                            </InlineCode>{" "}
                            returns a handle only for that exact current run, including a successor
                            started by the local queue. <InlineCode>{"None"}</InlineCode> means
                            absent or superseded, not successful completion. Use durable replay for
                            old results.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            <InlineCode>{"RunHandle"}</InlineCode> is clonable. Its wait includes
                            durable completion and bounded run-end observers; a journaled{" "}
                            <InlineCode>{"RunFinished"}</InlineCode> event or session{" "}
                            <InlineCode>{"Idle"}</InlineCode> can arrive earlier. Clones share
                            completion and cancellation. Dropping a handle does not cancel work. Use{" "}
                            <InlineCode>{"wait_completion"}</InlineCode> to obtain{" "}
                            <InlineCode>{"RunCompletion"}</InlineCode> with its summary and atomic{" "}
                            <InlineCode>{"queued_successor"}</InlineCode> decision. Discard
                            dependent prompts when that flag is true, even if the successor already
                            finished. A false flag is not proof of current idleness. Summary-only{" "}
                            <InlineCode>{"wait"}</InlineCode> stays compatible. Bound retained
                            handles and recheck the current target, revocation and expiry after
                            waiting outside a host's admission lock. This additive API still needs
                            its fresh local gate, hosted review, protected publication and consumer
                            adoption; this source page does not prove live publication.
                        </>
                    ),
                },
            ],
        },
        {
            id: "verification",
            title: "Verification and source references",
            blocks: [
                {
                    type: "p",
                    children: (
                        <>
                            The corrected runtime source tree{" "}
                            <InlineCode>{"de80bf8d9a67d54041cb7d167ece5faf55c42ecc"}</InlineCode>{" "}
                            passed the root-owned local harness gate on macOS with Rust 1.99.1, Node
                            26.12.0 and Python SDK 3.14.2: 202 Rust tests passed with 3 ignored,
                            including all four new native-key wait regressions; 15 TypeScript SDK, 4
                            policy, 11 Python language-host and 16 Python SDK tests passed, along
                            with the required lint, dependency, documentation and build checks. The
                            ordered 87 Rust inputs have receipt SHA-256{" "}
                            <InlineCode>
                                {"752e0b103d746df4e527892062a7728d5113fe6d2f43471bc519034cdfcd0a97"}
                            </InlineCode>
                            . The log is{" "}
                            <InlineCode>
                                {"ejm-harness-vault-cancel-full-gate-20261019.log"}
                            </InlineCode>
                            , SHA-256{" "}
                            <InlineCode>
                                {"ec8daa2e0b5e38edfdfdd4962cc9fedf26ce06a9821b7a2a5fe3101a64280e0e"}
                            </InlineCode>
                            .
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            The earlier publication snapshot passed 198 Rust tests with 3 ignored.
                            Temporary CLI and IDE source copies passed locked compile checks after
                            resolving their own dependency locks, and the CLI dependency policy
                            check passed; active host checkouts were unchanged. Those results remain
                            historical. The Linux D-Bus setup and strict review caller prerequisite
                            merged as protected commit{" "}
                            <InlineCode>{"99ba4575ad9070a7a0f3296a35e746858af904b0"}</InlineCode>.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            The initial API merge's normal hook failed a paused-clock deadline
                            fixture before commit. The fixture-only follow-up resumed its clock,
                            leaving the other 86 runtime inputs unchanged. Its focused deadline test
                            and normal local hook then passed as commit{" "}
                            <InlineCode>{"90aa7350e30eaad908d14e8b3260610d5f15f2fe"}</InlineCode>,
                            tree{" "}
                            <InlineCode>{"a2a65b0fd54cd666d25c863c184d2f69a1f6af0b"}</InlineCode>;
                            the successful hook does not report individual suite counts. Hosted run{" "}
                            <InlineCode>{"39299507686"}</InlineCode> found two medium issues: an
                            ordinary registry test required an OS key service, and a legacy FIFO
                            open could block before descriptor validation. Linux logs confirm the
                            first failure; the equivalent injected-key test passed.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            The registry correction removed only that duplicate native-store test
                            and rejected Unix FIFO sources without waiting for a writer, with a
                            bounded Linux regression. Native key-service tests remain opt-in, and
                            the original 30-second vault deadline remains unchanged. Hosted run{" "}
                            <InlineCode>{"39302646399"}</InlineCode> reviewed corrected head{" "}
                            <InlineCode>{"86677bbf89f34275614963eb26b91e05fcb7c80b"}</InlineCode>:
                            Linux, macOS, registry and standards passed, while Windows was still
                            running at the retained snapshot. Both static AI reviews completed; pass
                            1 found a retained Unix auth process-group identity and pass 2 found no
                            defects.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            The new successor retires that identity only after successful cleanup
                            and adds a bounded real-child regression. Its local gate and fresh
                            exact-head hosted acceptance remain pending; task 3.5 stays open. Host
                            lock adoption and full host gates, native terminal and PTY acceptance,
                            package release and docs publication remain separate. The{" "}
                            <InlineLink
                                href="https://github.com/ejmlabs/ejm-harness/blob/main/docs/host-auth-verification.md"
                                external
                            >
                                verification record
                            </InlineLink>{" "}
                            retains earlier failures, repeats, toolchain warnings and synthetic
                            measurements. These results do not prove real sign-in, paid inference,
                            native key-service acceptance, another operating system or docs
                            deployment.
                        </>
                    ),
                },
                {
                    type: "p",
                    children: (
                        <>
                            Source readers can use the canonical guides for the{" "}
                            <InlineLink
                                href="https://github.com/ejmlabs/ejm-harness/blob/main/docs/credential-vault.md"
                                external
                            >
                                vault format and package decision
                            </InlineLink>
                            ,{" "}
                            <InlineLink
                                href="https://github.com/ejmlabs/ejm-harness/blob/main/docs/claude-code.md"
                                external
                            >
                                official-client contract
                            </InlineLink>{" "}
                            and{" "}
                            <InlineLink
                                href="https://github.com/ejmlabs/ejm-harness/blob/main/docs/implementation-contract.md"
                                external
                            >
                                public Rust APIs
                            </InlineLink>
                            . Repository access may be required. See the{" "}
                            <InlineLink
                                href="https://code.claude.com/docs/en/cli-reference"
                                external
                            >
                                official Claude Code CLI reference
                            </InlineLink>{" "}
                            for the separately installed client.
                        </>
                    ),
                },
            ],
        },
    ],
};

/** The guide body. Rendered only on the server, inside the page's Static part. */
export default function HostAuthGuide() {
    return <DocBody page={page} />;
}
