# #3935 — SCR-D5-2026-09-06-02: `apply_effects` now recurses on `branch ++ tail` for every `Effect::Conditional`, so dispatch recursion depth is linear in the number of *sequential* Conditionals (O(N²) live clones) — `MAX_CONDITIONAL_DEPTH` bounds nesting only

- **Finding ID**: SCR-D5-2026-09-06-02
- **Labels**: high,scripting,quests,safety,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3935

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: HIGH (domain table: unbounded recursion / allocation reachable from untrusted `.pex`; bounded only by the `u16` instruction count)
- **Dimension**: Recognizer-Chain Soundness (the checklist's Conditional-dispatch bullet, item (d)); dispatch-time code owned by Dim 6 — reported once here
- **Untrusted-Input**: **Yes**
- **Location**: `crates/scripting/src/fragment.rs:1549-1562`; introduced by `962c9375` (2026-09-01)
- **Status**: NEW (regression of the premise the 2026-08-30 pass used to drop this candidate — "bounded transitively by whatever `lower_statements` produced")
- **Description**: Before `962c9375` the arm did `apply_effects(branch)` + `continue`, so recursion depth equalled nesting depth (capped at 256 by #3279). Now it builds `ordered_tail = branch ++ effects[index+1..]` (`Vec::with_capacity` + two `extend_from_slice`), recurses, and `break`s — each *sequential* Conditional adds one frame *and* one `Vec` that stays live until unwind: Σ(N−k) ≈ N²/2 `Effect` clones and N frames. `lower_statements` bounds statement *nesting*, not *count*; the `.pex` reader allows 65 535 instructions per function, so N is tens of thousands — hundreds of millions of live `Effect` clones and tens of thousands of frames when the quest reaches the bound stage. The `.psc` frontend has no sequential bound at all.
- **Evidence**: orchestrator re-read `fragment.rs:1510-1568` — `ordered_tail.extend_from_slice(branch); ordered_tail.extend_from_slice(&effects[index + 1..]); advances.extend(apply_effects(&ordered_tail, ..)); break;`.
- **Impact**: hostile/pathological mod content aborts (or OOMs) the engine at *dispatch* time — later and less diagnosable than a load-time failure; vanilla unaffected; benign long fragments pay O(N²) per dispatch.
- **Disproof attempted**: no iterative worklist exists; `break` confirms one frame per Conditional; `lower_statements` is a flat loop over statements; the 08-30 drop relied on the pre-`962c9375` shape.
- **Related**: #3279 (CLOSED); 2026-08-30 report "Stale candidates dropped" #3
- **Suggested Fix**: iterative `apply_effects` over an explicit stack of `(slice, index)` cursors; materialise a tail only at a `ProviderCall`/suspension. Add a ~10k sequential-Conditional AST test.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
