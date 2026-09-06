# #3946 — SCR-D6-2026-09-06-05: `MAX_PROVIDER_FRAGMENT_BARRIERS = 64` guards a recursion that cannot cycle, and its early return discards the 64th tail's already-queued non-provider deferred mutations

- **Finding ID**: SCR-D6-2026-09-06-05
- **Labels**: low,scripting,quests,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3946

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.
_Merged finding — also reported as `SCR-D5-2026-09-06-05` by Dimension 5._


- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems (dispatch) — also flagged by Dim 5 · **Untrusted-Input**: No · **Location**: `crates/scripting/src/fragment.rs:254, 594-600` · **Status**: NEW
- **Description**: every provider tail is a strict suffix (`effects[index + 1..]`), so `apply_at_depth`'s recursion terminates structurally; the cap fires only on ≥65 barriers (plausible for StorageUtil-heavy init fragments). At the cap the method returns *before* flushing `scene_actor_bindings_dirty`, `activations`, `reference_enable_changes`, and `cinematic_presentation` — but the `deferred` reaching that depth was filled by a tail whose `stages`/`objectives` mutations were already committed under the guard. Partial application with one WARN; `MAX_CASCADE` makes the opposite (correct) choice by checking before applying.
- **Suggested Fix**: check `depth + 1 >= MAX…` before running the tail's `apply_effects` (skip the tail whole), or flush non-provider deferreds before any early return; or move the bound to lowering.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
