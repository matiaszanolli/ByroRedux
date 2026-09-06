# #3942 — SCR-D1-2026-09-06-02: `rejects_truncation` pins one truncation offset; the "take is the single bounds gate" invariant is re-verified by hand every cycle instead of by an exhaustive-prefix test

- **Finding ID**: SCR-D1-2026-09-06-02
- **Labels**: low,scripting,test-gap,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3942

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: PEX Reader & Opcode Decode · **Untrusted-Input**: Yes · **Location**: `crates/pex/src/lib.rs:773-779` · **Status**: NEW (test-coverage gap; code is correct)
- **Description**: the four wire-valid builders already in the test module (`build_sample`, `_skyrim_be`, `_starfield_with_guards`, `build_extender_dependent_skyrim_be`) truncated at every prefix `0..len` with `assert!(parse(..).is_err())` would mechanically pin the gate across all three dialects and the debug-info / skip / var-arg paths the current single-offset test never reaches.
- **Suggested Fix**: one `#[test]` looping the cut over the four builders (≈ 4 × 300 iterations).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
