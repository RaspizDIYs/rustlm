# LCU reliability and offline testing

Scope: structured WAMP/ready-check parsing, bounded reconnection and fresh
snapshots, HTTP status handling, ARAM bench revalidation, and a loopback-only
HTTP/WebSocket simulator. Preserve settings, UI, polling/UIA and session guards.

Validation: malformed/reordered events, HTTP failures, reconnect snapshots,
bench unlock/expiry and changed sessions; build and launch the desktop app.

Offline fixtures are synthetic, not captures from RU/EUW. They do not establish
compatibility with a live Riot client or prove that a match cannot be dodged.

## Running checks

- `node --test scripts/lcu-simulator.test.mjs` checks the simulator itself.
- `node scripts/test-lcu-integration.mjs` starts an isolated simulator and runs
  the ignored Rust HTTP/WebSocket automation test. Requires the usual Rust/MSVC
  build environment. The temporary connection file is removed afterwards.
  On Windows, the runner uses Windows SDK `mt.exe` from PATH to add Common
  Controls v6 to the test executable (Tauri adds it to the app, not lib tests).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` runs protocol/policy tests.
  For Windows x64, first set PowerShell
  `$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER='node ../scripts/run-rust-test.cjs'`
  with Windows SDK `mt.exe` on PATH. The integration runner sets this automatically.

For manual debug builds: start `node scripts/lcu-simulator.mjs`, set
`RUSTLM_TEST_LCU` to the printed JSON file path, then run a debug LM build.
It uses HTTP/WS on **127.0.0.1 only**; the override is compiled out of release
builds. Stop your normal LM first because the application is single-instance.
Clear the environment variable before returning to real-client testing.
Missing/invalid override files do not fall back to a real League instance.

Use authenticated POST `/__test/scenario` with `{"name":"ready"}`,
`aram-locked`, `aram-open`, `mayhem-open`, `classic` or `idle` to switch scenarios.
`/__test/disconnect` drops WebSockets; `GET /__test/state` shows synthetic actions.
Authenticate with Basic `riot:<password from connection file>`.
Unknown writes return 404. This is not a permissive fake of all Riot endpoints.

Load a saved fixture at startup:
`node scripts/lcu-simulator.mjs artifacts/lcu-simulator.json scripts/fixtures/lcu/aram-open.json`.
An authenticated POST `/__test/fixture` replaces the current snapshot during a
test. Keep region, patch and provenance in `metadata`; the supplied file is
explicitly synthetic, not a recording of a player.

The offline transport deliberately omits TLS and real process discovery.
Actual TLS, client patches, RU/EUW behavior, game timing and UIA still require
live League checks. Never store live auth headers, tokens or account credentials
in fixtures. No third-party application code or recorded player data was copied.
