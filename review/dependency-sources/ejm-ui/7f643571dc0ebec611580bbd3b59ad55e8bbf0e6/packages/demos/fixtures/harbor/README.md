# harbor

A small order library with a retry helper and a TypeScript config loader.
This is sample content written for the demo.

## Layout

- `src/lib.rs` exports the public API.
- `src/main.rs` prints an order total.
- `src/retry.rs` retries a call with exponential backoff.
- `web/config.ts` reads the service config.

## Test

Run `cargo test`.
