# #3948 — SCR-D5-2026-09-06-07: the #1816 panic net covers `decompile_script` only — `analyze_pex_compatibility` and `lower_provider_program` run on the same untrusted-derived data outside `catch_unwind` on every entry variant

- **Finding ID**: SCR-D5-2026-09-06-07
- **Labels**: low,scripting,safety,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3948

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness (seam into the unaudited SDK layer) · **Untrusted-Input**: Yes · **Location**: `crates/scripting/src/translate/mod.rs:152-153, 158`; `crates/scripting/src/fragment.rs:1926-1927, 2178-2179` · **Status**: NEW (hardening; no panic demonstrated)
- **Description**: ~7k LOC of unaudited code (incl. the `unreachable!` arms in `papyrus_provider/execute.rs:848-858`) now sits on the cell loader's untrusted path with no net; a panic there aborts cell load exactly as #1816 did.
- **Suggested Fix**: widen `decompile_catching_panics` to the whole decompile → preflight → provider-lower → recognize sequence; route "can it panic?" to the dedicated SDK pass.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
