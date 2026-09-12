# D5-06: TPLT resolution hoist never happened — 6bcd1666 patched four call sites individually instead of consolidating into spawn_placement_root

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4133
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11b.md` (HEAD `b3db49fa`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4133 --json state`.

---

Reported by `/audit-character` (second, independent re-verification pass) —
see `docs/audits/AUDIT_CHARACTER_2026-09-11b.md` (HEAD `b3db49fa`).

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: all templated families (FNV/FO3/FO4/Skyrim — anywhere `TPLT`/`template_flags` is read)
- **Location**: `byroredux/src/npc_spawn/resumable.rs:1418-1444` (`spawn_placement_root`, unchanged shape — still hands the raw shell to its four stamps); `byroredux/src/npc_spawn.rs:81,156,196`; `byroredux/src/npc_spawn/ai_package.rs:519-534`; `byroredux/src/npc_spawn/resumable.rs:432-438,619-625,1177-1183`; `byroredux/src/cell_loader/references/mod.rs:629-651`

## Description

`6bcd1666` correctly fixed the three MEDIUM TPLT-bypass defects filed by the
prior `AUDIT_CHARACTER_2026-09-11.md` (creature-attack damage, race meshes,
factions/AI-packages all reading the unresolved NPC shell instead of the
TPLT-resolved record), and shipped a passing regression test for each. But it
did so by adding four independent `resolve_inherited_*` call sites rather
than consolidating any of the resolution already happening elsewhere in the
same spawn path — exactly the mechanical hoist the prior report's own
"structural remedy" paragraph asked for:

> "The structural remedy is the same for all three: hoist one
> `resolve_inherited_stats` / `resolve_inherited_traits` pair into
> `spawn_placement_root` and hand every consumer the resolved record, so a
> sixth site cannot repeat it."

That hoist did not happen. `spawn_placement_root` still calls
`stamp_faction_ranks`, `stamp_actor_values` (→ `derive_npc_actor_values`),
`stamp_creature_attack`, and `stamp_character_components` with the raw
`&NpcRecord` and lets each stamp resolve independently. Counting every
independent author of the same three-line pattern
(`effective_actor_level` → `resolve_inherited_*` → read a field) in the
current spawn path: the four stamps above, plus `apply_ai_package_behavior`,
`prepare_runtime_state`, `prepare_creature_state`, `prepare_prebaked_state`,
and `load_references_budgeted` — nine sites total.

## Evidence

```
byroredux/src/npc_spawn.rs:83   resolve_inherited_factions(npc, shell_level, index)
byroredux/src/npc_spawn.rs:158  resolve_inherited_stats(npc, shell_level, index)
byroredux/src/npc_spawn.rs:209  resolve_inherited_stats(npc, shell_level, index)
byroredux/src/npc_spawn.rs:210  resolve_inherited_traits(npc, shell_level, index)
byroredux/src/npc_spawn/resumable.rs:438   resolve_inherited_traits(npc, effective_actor_level(npc), index)
byroredux/src/npc_spawn/resumable.rs:626   resolve_inherited_traits(npc, effective_actor_level(npc), index)
byroredux/src/npc_spawn/resumable.rs:1182  resolve_inherited_traits(npc, effective_actor_level(npc), index)
byroredux/src/cell_loader/references/mod.rs:642  resolve_inherited_traits(npc, ..., index)
```

`spawn_placement_root` (`byroredux/src/npc_spawn/resumable.rs:1418-1444`)
hands each of its four stamps the raw `&NpcRecord` + `index`, unchanged since
before `6bcd1666`.

## Impact

1. **Correctness risk, latent.** Every currently-reachable consumer is
   individually correct today (confirmed by four green regression tests from
   `6bcd1666`), but the type system gives a future contributor no signal to
   route a new population-boundary read through `resolve_inherited_*` — it
   can read `npc.<field>` directly and compile cleanly, exactly as
   `stamp_creature_attack` did when first introduced. This defect class has
   now recurred seven times (#2956, #3381, #3382, #3480, #4091, #4092,
   #4093).
2. **Redundant work.** One NPC spawn via the runtime/FaceGen path now walks
   the TPLT chain independently up to six times per spawn (bounded by
   `MAX_TPLT_DEPTH`, so not unbounded, but wasteful), and any future
   divergence between two of the six copies would reproduce the "one entity,
   two chains" symptom #3480 exists to document.

## Suggested Fix

In `spawn_placement_root`, resolve once:

```rust
let stats = resolve_inherited_stats(npc, level, index);
let traits = resolve_inherited_traits(npc, level, index);
let factions = resolve_inherited_factions(npc, level, index);
```

and change every stamp's signature to take the already-resolved records
instead of `npc` plus `index`. Apply the same pattern to the three
`build_npc_equip_state` call sites and to `apply_ai_package_behavior`'s
caller. This is a mechanical refactor with no intended behavior change
(every current call site already resolves correctly) — it converts "which of
the two record shapes do I have" from a habit into a compile-time question
for the next contributor.

## Related

Recurrence of the same defect class as #2956, #3381, #3382, #3480, #4091,
#4092, #4093. Process critique of how `6bcd1666` landed — not a re-opening of
any of those, which remain correctly fixed.

## Completeness Checks
- [ ] **SIBLING**: All nine current independent `resolve_inherited_*` call
      sites (`npc_spawn.rs`, `npc_spawn/resumable.rs` ×3,
      `npc_spawn/ai_package.rs`, `cell_loader/references/mod.rs`,
      `prepare_runtime_state`, `prepare_creature_state`,
      `prepare_prebaked_state`) are converted to consume a resolved record
      instead of resolving independently
- [ ] **TESTS**: A regression test pins that `spawn_placement_root` resolves
      exactly once per spawn (e.g. via a call-count instrumentation or
      equivalent) so a future consumer reintroducing a tenth independent
      resolve call is caught
