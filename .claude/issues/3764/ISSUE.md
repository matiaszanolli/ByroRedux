# #3764 — SAFE-2026-08-30-D8-01: embedded-clip registration bypasses the #790 path memo on the NPC spawn path

**Severity**: MEDIUM · **Location**: `byroredux/src/scene/nif_loader.rs` — `load_nif_bytes_with_skeleton`'s embedded-clip branch
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (SAFE-D8-01)

`load_nif_bytes_with_skeleton`'s embedded-clip branch called the un-keyed
`registry.add(clip)` instead of the #790 dedup mechanism
(`AnimationClipRegistry::get_or_insert_by_path`) the sibling KF loader
(`npc_spawn.rs`) already uses correctly. This function runs once per NPC
skeleton, once per body part, once per head, and once per equipped item —
so any NPC-worn NIF carrying an embedded controller stack registered a
fresh, un-freeable `AnimationClip` copy on every NPC spawn and every cell
reload, none of it ever released (the two `release()` call sites only
retire `NifImportRegistry`-owned handles, a different registry).
Separately, the same branch `world.spawn()`ed the `AnimationPlayer` entity
with no `Parent` link into the NIF's own subtree — a second, entity-level
leak, since cell unload's despawn walk reaches victims via `Children`
traversal from tracked roots.

## Fix implemented

Both parts of the issue's own suggested fix:

1. `registry.add(clip)` → `registry.get_or_insert_by_path(label.to_string(),
   || clip)`, keyed on `label` (the archive mesh path, already threaded
   into the function at every `resumable.rs` call site). Matches the exact
   shape the issue's own suggested fix specifies — `clip` is still built
   eagerly (not deferred into the closure), so `get_or_insert_by_path`
   discards the redundant build on a hit rather than skipping it, but the
   REGISTRY-side leak (a second handle for identical content) is closed,
   which is the actual defect.
2. `add_child(world, root, player_entity)` when `root` is `Some` — parents
   the `AnimationPlayer` entity into the NIF's own subtree, confirmed
   reachable by the cell-unload despawn walk (`cinematic_retained_entities`
   in `cell_loader/unload.rs` walks victims via `Children`, which
   `add_child` populates on the parent).

**SIBLING** (issue's own checklist item): grepped every production
`registry.add(`/`.add(clip)` call site outside the KF loader (6 found,
beyond the fixed one, plus several test-only sites in
`crates/core/src/animation/mod.rs`/`crates/save/src/validate.rs` excluded
as non-production):

- `cell_loader/partial.rs:93` and `cell_loader/references/synth_child.rs:602`
  — the cell-loader REFR-based NIF import path the issue itself contrasts
  as already-correct: both are gated behind a higher-level
  `NifImportRegistry`/`pending_clip_handles` cache-miss check that returns
  early on a repeat, so the registration genuinely runs once per unique
  model path — confirmed by reading the early-out logic, not assumed from
  the comment.
- `byroredux/src/scene.rs:1016` — the `--kf` CLI debug-tool path (loads one
  loose KF file specified on the command line, once per process
  invocation), not a per-spawn or per-cell-reload hot path. Out of the
  issue's stated scope.
- `byroredux/src/systems/animation.rs:1671,1993,2116` — all three inside
  `#[cfg(test)] mod tests` (confirmed against the module boundary at
  line 1520), synthetic test fixtures, not production code.

No further fix needed beyond the one site the issue named.

**LOCK_ORDER** (issue's own checklist item): preserved — `StringPool` is
still dropped (`drop(pool)`) before `AnimationClipRegistry` is taken, the
conversion happens entirely before the registry borrow starts.

**TESTS** (issue's own checklist item): `embedded_clip_registration_
dedupes_by_label_across_repeated_spawns` pins the exact call shape the fix
now uses (`get_or_insert_by_path` with the same `label` key across two
simulated spawns) — same handle returned, registry length stays at 1.
Building a true end-to-end fixture (synthetic NIF bytes with an embedded
`NiControllerSequence` chain, driven through
`load_nif_bytes_with_skeleton` itself) would be substantially larger than
the fix; the underlying dedup mechanism this now routes through is already
exhaustively covered by `get_or_insert_by_path_dedupes_repeated_calls` and
its siblings in `crates/core/src/animation/registry.rs`.

Full workspace: `cargo test --no-fail-fast` 7068 passing, 0 failing (+1 new
test).
