# #3943 — SCR-D3-2026-09-06-02: the smoke harness's `expected_top_level_item_count` still mirrors the pre-#3786 case-SENSITIVE auto-state rule, so the #3017 shape check now disagrees with `decompile_script` on the very input #3786 fixed

- **Finding ID**: SCR-D3-2026-09-06-02
- **Labels**: low,scripting,test-gap,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3943

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Decompiler Control-Flow / Boolean / Lower · **Untrusted-Input**: Yes (false harness report, not a crash) · **Location**: `crates/pex/examples/pex_corpus_smoke.rs:95` vs `crates/pex/src/decompile/lower.rs:424` · **Status**: NEW
- **Description**: `88e7dbfc` changed the auto-state match to `eq_ignore_ascii_case`; the harness predicate is still `==`. A mismatched-casing auto state would report a spurious `decompiled_shape_mismatch` and send triage at the decompiler. Latent — none observed in the vanilla corpus.
- **Suggested Fix**: expose `pub fn is_auto_state(object, state) -> bool` from `lower.rs` and call it from both.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
