# #3944 — SCR-D4-2026-09-06-01: the #2668 bisect is correct under duplicate `pp_off` entries, but nothing pins that — the one case where `partition_point` vs `binary_search` differ is untested and the stated invariant ("strictly increasing") is wrong

- **Finding ID**: SCR-D4-2026-09-06-01
- **Labels**: low,scripting,test-gap,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3944

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes (diagnostic offsets only) · **Location**: `crates/papyrus/src/lexer.rs:52-57, 59-74, 154-183` · **Status**: NEW (#2668 CLOSED; follow-up on its fix)
- **Description**: two back-to-back continuations (`"a\\\n\\\nb"`) yield `entries = [(1, 2), (1, 4)]` — non-decreasing, not strictly increasing as the commit body, `ISSUE.md`, and docstring claim. `partition_point(pp_off <= p)` + `idx − 1` picks the *last* duplicate (largest cumulative `removed`), matching the old scan — but `binary_search_by_key`'s index under duplicates is unspecified, and the regression test's fixture (`pp_off ∈ {2,4,6}`) has no duplicates. Dim 4's differential harness: 349 525 inputs, 34 134 maps with adjacent duplicates, 0 mismatches — correct today, unpinned.
- **Suggested Fix**: add a `\\\n\\\n` case, a leading-continuation (`pp_off = 0`) case, a mixed CRLF/lone-CR map, and `to_original(out.len()) == source.len()`; reword the docstring to "non-decreasing … do not replace with `binary_search`".

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
