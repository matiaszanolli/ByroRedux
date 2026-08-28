# #3420 — FNV-2026-08-27-D6-01: FNV head sub-part meshes (mouth / teeth / tongue) are parsed, indexed and never spawned

**Labels**: medium, bug, character, esm-plugin, game:fnv, legacy-compat

**Filed**: 2026-08-27 · from `docs/audits/AUDIT_FNV_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-27.md` — finding `FNV-2026-08-27-D6-01` (HEAD `969d81c8`)

- **Severity**: MEDIUM
- **Dimension**: 6 — Animation, Skinning & NPC assembly
- **Location**: `byroredux/src/npc_spawn/resumable.rs:411-424` (the only `head_parts` consumer) · `crates/plugin/src/esm/records/actor/mod.rs:533-541` (four dead constants) · `:452-456` (a doc claim the code does not meet)

## Description

`RaceRecord::head_parts` exists so the spawner can select head sub-meshes by semantic role, and the `head_part` module publishes `HEAD`, `MOUTH`, `TEETH_LOWER`, `TEETH_UPPER`, `TONGUE`, `LEFT_EYE`, `RIGHT_EYE`. Only `LEFT_EYE` and `RIGHT_EYE` are ever read; `MOUTH`, `TEETH_LOWER`, `TEETH_UPPER` and `TONGUE` have no reference anywhere outside their own definition.

## Evidence

```
$ grep -rn "head_part::" byroredux/src/ crates/ | grep -v 'pub const'
byroredux/src/npc_spawn/resumable.rs:416:  ... head_part::LEFT_EYE
byroredux/src/npc_spawn/resumable.rs:417:  ... head_part::RIGHT_EYE
```

```
$ grep -rn "\.head_parts" byroredux/src/ crates/plugin/src/ | grep -v _tests
byroredux/src/npc_spawn/resumable.rs:400   # index.head_parts — the HDPT map, unrelated
byroredux/src/npc_spawn/resumable.rs:414   # race.head_parts — eyes only
crates/plugin/src/esm/records/actor/mod.rs:1406   # the push site
```

Every FNV race authors the four meshes; `CaucasianOldAged` carries `MouthHuman.NIF`, `TeethLowerHuman.NIF`, `TeethUpperHuman.NIF`, `TongueHuman.NIF` in both gender sections. The field's own doc (`actor/mod.rs:452-456`) frames the pairing as the fix for *"Pre-fix every NPC rendered with just the head NIF — no eyes, mouth, teeth, tongue, ears"*, a claim only the eyes half of which is realised.

## Impact

FNV head meshes model the lips but not the oral cavity — the mouth interior, both teeth rows and the tongue are separate NIFs. Every FNV NPC therefore has an empty hole behind the lips, visible whenever the jaw opens (all dialogue, all idle talk animations) and at grazing angles when closed. The data is parsed and one `flat_map` away from the spawn list.

## Related

FNV-2026-08-27-D4-03 must land first — until the RACE body section stops leaking into `head_parts`, `head_part::MOUTH` also matches `RightHand.nif`. Sibling: FNV-2026-08-27-D5-01 (the same field's doc rot).

## Suggested Fix

Extend the `eye_paths` filter to the full head-sub-part set (mouth / teeth ×2 / tongue), keeping the same gender-tag rule, and spawn them alongside the eyes in `spawn_runtime_head`. If the omission is a deliberate deferral, say so at the constants and correct the `head_parts` doc, which currently reads as though it were done.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the hair / brow / eye-texture selectors beside it, and the Oblivion spawn arm)
- [ ] **TESTS**: A regression test pins this specific fix
