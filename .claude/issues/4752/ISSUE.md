# TOOL-D1-2026-09-22-01: Unauthenticated debug port grants arbitrary local file read + write, on by default in every build including --release

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4752

## Description
`byroredux/Cargo.toml:8-9` sets `default = ["debug-server"]`, and `main.rs:816-822` unconditionally starts the debug TCP listener (default port 9876, overridable via `BYRO_DEBUG_PORT`) behind only that always-on feature — this is a stock `cargo run` / `cargo build --release` binary, not a dev-only profile. `crates/debug-server/src/listener.rs:173` binds `("127.0.0.1", port)` with zero authentication of any kind — confirmed via a full grep of `crates/debug-server/src` and `crates/debug-protocol/src` for `auth`/`token`/`password`/`Authorization`: the only hits are an unrelated doc comment ("token is looked up in the registry first") and expression-parser tokens, not an auth mechanism. Any local process can connect. Three independent request paths then let that connection touch the filesystem:

1. `DebugRequest::Screenshot { path }` → `crates/debug-server/src/system.rs:90` calls `std::fs::write(path, &png_bytes)` with the client-chosen path verbatim — no root confinement, no extension check.
2. The console command `tex.dump <bsa-path> <texture-path> [out.png]` (reachable via `Eval`, since `eval_request` dispatches any `CommandRegistry` name) — `byroredux/src/commands/assets.rs:254` opens `archive_path` verbatim via `crate::asset_provider::Archive::open(archive_path)`, then `:297` writes the decoded PNG to a client-chosen `out_path` via `std::fs::write(&out_path, &png)` — independent of Screenshot.
3. `DebugRequest::LoadNif { path, .. }` → `byroredux/src/debug_load.rs`'s `resolve_nif_bytes` (line 152) calls `std::fs::read(path)` on the client-supplied path directly, before ever consulting the `--bsa` archive list — confirmed by reading the function body, not just its doc comment ("try a loose absolute path first").

All three paths were read and confirmed independently at HEAD `c3f298a24`.

## Evidence
```rust
// crates/debug-server/src/system.rs:90-91
Some(path) => match std::fs::write(path, &png_bytes) {
    Ok(()) => DebugResponse::ScreenshotSaved { path: path.clone() },
```
```rust
// byroredux/src/commands/assets.rs:254-256, 297-298
let archive = match crate::asset_provider::Archive::open(archive_path) { ... };
...
if let Err(e) = std::fs::write(&out_path, &png) {
```
```rust
// byroredux/src/debug_load.rs:151-153
fn resolve_nif_bytes(path: &str) -> Option<Vec<u8>> {
    if let Ok(bytes) = std::fs::read(path) {
        return Some(bytes);
```
```rust
// byroredux/Cargo.toml:8-9
[features]
default = ["debug-server"]
```

## Impact
A local, otherwise-unprivileged process on the same machine (any other application running as the same user, or anything that can reach a fixed, well-known default TCP port) can overwrite arbitrary files reachable by the engine process, and read arbitrary files back out through `LoadNif`'s parse-and-report path — with no opt-in beyond "the engine is running". This escapes the ECS/world sandbox entirely onto the host filesystem, in every build including `--release`.

## Related
- #3449 (closed) addressed connection/thread exhaustion caps (an uncapped OS thread per connection), not file read/write — confirmed by reading its body; different defect class.
- No existing open or closed issue covers arbitrary file write/read via `Screenshot`, `tex.dump`, or `LoadNif` (checked `/tmp/audit/issues.json`, 4,639 issues, and a fresh open-issue keyword search).

## Suggested Fix
Require an explicit opt-in (env var or CLI flag) to start the debug server in a release build, keeping default-on only for dev builds via a `dev` feature; and/or confine `Screenshot`/`tex.dump` output paths to a fixed screenshots/dumps directory (reject absolute paths and `..`), and confine `tex.dump`'s archive-open and `LoadNif`'s path to the configured game-data root(s).

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D1-2026-09-22-01)

## Completeness Checks
- [ ] **SIBLING**: All three independent paths (`Screenshot`, `tex.dump`, `LoadNif`) fixed together, not just one — a partial fix leaves the other two exploitable
- [ ] **TESTS**: A regression test asserts the debug server refuses to start without the opt-in in a release-profile build, and that path confinement rejects `..`/absolute paths on `Screenshot`/`tex.dump`
