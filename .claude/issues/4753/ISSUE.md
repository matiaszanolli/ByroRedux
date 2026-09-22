# TOOL-D1-2026-09-22-02: WalkEntity.max_depth and generic dump responses are unbounded on the encode side, while the decode side enforces a 16MB cap

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4753

## Description
`crates/debug-protocol/src/wire.rs`'s `encode` (lines 12-19) writes `let len = json.len() as u32;` with no check against the crate's own `MAX_MESSAGE_SIZE` (16 MB) constant — that constant is only consulted on the *decode* side (`decode`, same file). `WalkEntity`'s `max_depth: u32` parameter has no upper bound and `eval_walk_entity` (`crates/debug-server/src/evaluator.rs:332-395`) keeps no visited-entity set — confirmed by reading the full function body: the loop condition is only `if depth > max_depth { continue; }`, with no cap on `max_depth` itself. A large hierarchy walked at a high `max_depth` (or `ListEntities` on a densely-populated world) can produce a response over the 16 MB cap the client enforces on read. When that happens, the client's `decode` rejects the frame *before* reading the payload and returns an error; both `byro-dbg` entry points treat any decode error as fatal — confirmed in `tools/byro-dbg/src/main.rs:91-97` (`Err(e) => { eprintln!(...); break; }`) and the TUI net-thread's equivalent `break`.

## Evidence
```rust
// crates/debug-protocol/src/wire.rs:12-19
pub fn encode<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
    let json = serde_json::to_vec(msg)...;
    let len = json.len() as u32;   // no MAX_MESSAGE_SIZE check
```
```rust
// crates/debug-server/src/evaluator.rs:355-358
while let Some((entity, depth)) = stack.pop() {
    if depth > max_depth {
        continue;
    }
```
```rust
// tools/byro-dbg/src/main.rs:91-97
match wire::decode::<DebugResponse>(&mut stream) {
    Ok(response) => display::print_response(&response),
    Err(e) => {
        eprintln!("Receive error: {}", e);
        break;
    }
}
```

## Impact
A single `walk <root> 999999` against a real cell (or any command against a large world) can kill the whole `byro-dbg` session with an opaque "message too large" error; the TUI variant degrades silently (net thread dies, dashboard freezes, nothing tells the operator why). Developer-tool defect only, fully recoverable by reconnecting.

## Related
None found — no existing issue covers wire-protocol size symmetry or `max_depth` bounding.

## Suggested Fix
Cap `max_depth` server-side (e.g. 64) and add a visited `HashSet<u32>` to `eval_walk_entity`; have the server synthesize a `DebugResponse::error(...)` instead of sending an over-cap frame when the encoded payload would exceed `MAX_MESSAGE_SIZE`.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D1-2026-09-22-02)

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a response payload over `MAX_MESSAGE_SIZE` and asserts the server returns a `DebugResponse::error(...)` instead of an oversized frame, plus a `max_depth` cap test
