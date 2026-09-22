# TOOL-D1-2026-09-22-03: byro-dbg has no socket read timeout and no #[test] in the whole crate

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4754

## Description
The server always answers within ~5s independent of the frame loop (`crates/debug-server/src/listener.rs:362`'s `rx.recv_timeout(Duration::from_secs(5))`), so an ECS-side stall alone does not hang the client. The narrower but real trigger is the engine *process* itself being fully wedged (OS-level freeze, debugger attach, severe thrashing) — in that case `wire::decode`'s `read_exact` blocks with no client-side timeout, hanging the REPL/TUI indefinitely. Confirmed: `grep -rn set_read_timeout tools/byro-dbg/src/` returns no matches anywhere in the crate (the only two `set_read_timeout` call sites in the whole workspace are server-side, `crates/debug-server/src/listener.rs:309,541`). Separately, the entire crate has zero `#[test]` functions — confirmed via `grep -rn '#\[test\]' tools/byro-dbg/src/` (0 matches) and `cargo test -p byro-dbg` (0 tests). `parse_shorthand`, `display::print_response`, and the TUI's `handle_response`/`variant_name` are all unit-testable with no coverage.

## Evidence
```
$ grep -rn set_read_timeout tools/byro-dbg/src/
(no matches)
$ grep -rn '#\[test\]' tools/byro-dbg/src/ | wc -l
0
```
```rust
// crates/debug-server/src/listener.rs:361-362 (server-side only)
match rx.recv_timeout(Duration::from_secs(5)) {
```

## Impact
Narrow trigger (full process freeze, not a slow query), so LOW; paired with a total absence of test coverage on a hand-parsed dispatcher (`parse_shorthand`).

## Related
None found.

## Suggested Fix
Add `stream.set_read_timeout(Some(Duration::from_secs(10)))` before the request loop in both `main.rs` and `tui.rs`; add unit tests for `parse_shorthand` at minimum.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D1-2026-09-22-03)

## Completeness Checks
- [ ] **TESTS**: Unit tests added for `parse_shorthand` (at minimum); a timeout is set and exercised
