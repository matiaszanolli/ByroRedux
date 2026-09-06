# #3945 — SCR-D4-2026-09-06-02: the "aligned" depth caps in `lift.rs` and `effects.rs` are hand-copies of `pub(crate)` papyrus constants — the alignment their docstrings promise is unenforced

- **Finding ID**: SCR-D4-2026-09-06-02
- **Labels**: low,scripting,tech-debt,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3945

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Pratt Parser (consumers in Dims 2/5) · **Untrusted-Input**: Yes (consistency, not safety) · **Location**: `crates/papyrus/src/parser/expr.rs:19`, `stmt.rs:38` (`pub(crate)`); copies at `crates/pex/src/decompile/lift.rs:363` (`usize`) and `crates/scripting/src/translate/effects.rs:373` (`u32`) · **Status**: NEW
- **Description**: three independent literal `256`s, differing types, no `use` and no `const _: () = assert!(..)`; both consumer crates already depend on `byroredux-papyrus`. Dim 4 also measured that the papyrus caps are *additive* across the statement and expression axes (255 nested `If` + 127 paren pairs) — fits 1 MiB at the workspace's `opt-level = 1`, so not a safety gap, but "share one stack-safety budget" (`stmt.rs:35-37`, `lift.rs:348-353`) is loose wording.
- **Suggested Fix**: make the papyrus constants `pub` and reference them, or add a `const` assert beside each copy.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
