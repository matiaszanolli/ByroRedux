# TOOL-D5-2026-09-22-02: BYROREDUX_SKYRIM_DATA (not canonical BYROREDUX_SKYRIMSE_DATA) is still read by five Rust files and shell scripts

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4760

## Description
The canonical Skyrim SE data-dir env var is `BYROREDUX_SKYRIMSE_DATA` (`crates/plugin/src/esm/test_paths.rs:83`, `SKYRIM_SE_ENV`). `BYROREDUX_SKYRIM_DATA` (missing the `SE`) is still the only variable read at five Rust sites, confirmed via `grep -rln 'BYROREDUX_SKYRIM_DATA\b'`: `byroredux/src/asset_provider/animation.rs`, `byroredux/src/npc_spawn/ai_package.rs`, `crates/hkx/src/animation.rs`, `crates/scripting/examples/mq101_conformance.rs`, and `crates/ui/tests/hudmenu_protocol.rs` (5 files total, matching the report's count), plus a double-digit number of `docs/smoke-tests/*.sh` / `scripts/*.sh` shell scripts. Every site checked is opt-in test/tooling code — `#[ignore]`-gated `#[test]` functions or a standalone `cargo run --example` probe / shell smoke scripts — never the running engine or a player-facing code path.

## Evidence
```
$ grep -rln 'BYROREDUX_SKYRIM_DATA\b' --include='*.rs' .
byroredux/src/asset_provider/animation.rs
byroredux/src/npc_spawn/ai_package.rs
crates/scripting/examples/mq101_conformance.rs
crates/ui/tests/hudmenu_protocol.rs
crates/hkx/src/animation.rs
```
```rust
// crates/plugin/src/esm/test_paths.rs:83 (canonical)
pub const SKYRIM_SE_ENV: &str = "BYROREDUX_SKYRIMSE_DATA";
```

## Impact
Confined to opt-in developer workflows; never a player or production-engine defect. A developer who configures the documented canonical `BYROREDUX_SKYRIMSE_DATA` will find these specific opt-in tests/scripts silently fall back to a hardcoded path that only resolves on the maintainer's own dev machine.

## Related
#3741 (closed) addressed helper visibility, not this spelling split — not a regression of it.

## Suggested Fix
Rename all five `.rs` sites (ideally by importing `SKYRIM_SE_ENV` instead of a string literal, so the existing completeness guard covers them) and the shell scripts that reference the non-canonical name.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D5-2026-09-22-02)

## Completeness Checks
- [ ] **SIBLING**: All five `.rs` sites and the shell scripts renamed together, not just one, so the split doesn't partially persist
- [ ] **TESTS**: Import `SKYRIM_SE_ENV` instead of a string literal at each `.rs` site so the existing completeness guard covers them going forward
