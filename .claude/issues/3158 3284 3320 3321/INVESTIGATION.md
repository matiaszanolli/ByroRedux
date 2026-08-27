# 3158 / 3284 / 3320 / 3321 — investigation notes

## #3158 — `HasPerk` reads a component nothing writes (FIXED)

The issue deliberately declined to assert the Skyrim wire format, so that was
the first thing to settle. `crates/plugin/examples/probe_npc_perks.rs` (new)
censuses `PRKZ`/`PRKR` across shipped masters:

| master | `NPC_` | with `PRKR` | entries | `PRKR` size |
|---|---:|---:|---:|---:|
| `Skyrim.esm` | 5118 | 1620 | 7993 | 8 B |
| `Fallout4.esm` | 3015 | 1361 | 2771 | 5 B |
| `FalloutNV.esm` | 3816 | 0 | 0 | — |
| `Fallout3.esm` | 1647 | 0 | 0 | — |
| `Oblivion.esm` | 2482 | 0 | 0 | — |

So the answer to "(b) widen the gate or add a debug log" is **widen**, and
only as far as Skyrim. FO3/FNV/Oblivion ship no NPC perks at all, which makes
their `HasPerk` zero *data-correct* for NPCs — the issue's framing of the
symptom as "Skyrim, FO3 and FNV" is right about the observable but the cause
differs per family.

Skyrim's `PRKR` is **8 bytes to FO4's 5**, but both start with the PERK
FormID `u32` and carry the rank at byte 4 (Skyrim pads three unused bytes), so
the existing `len() >= 5` read already decodes both. Only the gate was wrong.
New predicate `GameKind::uses_npc_perk_entries()`; the `PRKR` arm moved out of
`parse_npc_actor_values` (which stays FO4+, since Skyrim has no `PRPS` and a
differently-shaped `DNAM`) into its own `parse_npc_perks`.

### SIBLING box

`ActorValues`, `CharacterLevel` and `Background` have the identical
single-writer shape (`spawn_npc_entity` only) and are likewise absent on the
player. They are **deliberately not stubbed**:

* an empty `ActorValues` would flip `melee_damage_charal_bonus`
  (`byroredux/src/combat.rs`:356) off its `else { return 0.0 }` arm and onto a
  derived-value computation over a zero SPECIAL set — a live behaviour change
  to melee damage, dressed up as a component stub;
* `Background` has no honest value until the player has a real race/class.

Populating those is CHARAL work (#3004 / #2986), and the finding is recorded
in the code comment at the insert site rather than papered over.

### Test shape

The pin is deliberately end-to-end (`skyrim_parsed_npc_perk_reaches_the_hasperk_condition`)
rather than three unit tests, because the bug lived at the seam and each link
looked correct alone: parse drops the perk silently → `stamp_character_components`
skips the component when the list is empty → `HasPerk` returns `0.0` for an
absent component, which is the *correct* Bethesda default. A test on any one
link stays green through the bug.

## #3284 — FNV `WaterKind` vocabulary (ALREADY FIXED — stale finding)

Both halves of the issue's ask were already satisfied at HEAD by `f4ed4f34`
(2026-08-20):

* the design decision is documented in `water_kind_from_name`'s doc comment,
  which lists `spill`, `fountain` and `potomac` as *considered and rejected*
  with the reasoning (`Potomac` is the `WRLD` `NAM2` default for 10
  worldspaces; promoting it would add current to every un-overridden body in
  all of them);
* `rejected_tokens_stay_calm` pins `Potomac`, `PotomacNRShallow`,
  `ToxicSpillPuddle`, `TenPenWaterFountain` and others as `Calm` — exactly the
  issue's sole completeness check.

The issue was filed 2026-08-24 from `AUDIT_FNV_2026-08-24.md`, four days after
the fix; the audit checked the token list but not the doc block or the test
directly beneath it. No code change.

## #3320 — interior water texture resolved after the only flush (FIXED)

Premise verified end to end. `resolve_texture` reserves a bindless slot and
points its descriptor at the fallback checkerboard until a batched flush;
there are two `flush_pending_uploads` call sites in the engine and no
per-frame flush. `spawn_water_plane` resolves the WATR `NNAM` normal map, and
the interior loader called it *after* `load_references`, whose tail carries an
interior load's only flush.

Took the issue's "better" option rather than its primary sketch: the water
spawn **moved above** `load_references` so both routes share one flush
boundary, instead of adding a second flush. That matches the exterior route
(`ExteriorCellApplyJob::begin` already spawns terrain + water before `advance`
loads references) and removes the ordering foot-gun rather than patching
around it. `load_references` runs on an unlimited `FrameTimeBudget` and always
reaches `complete`'s forced flush, so the single boundary is unconditional.

**SIBLING**: the other `resolve_texture` call sites outside
`cell_loader/references/` are `placement_lod.rs`, `object_lod.rs` and the
exterior `water.rs` arm — all on the exterior streaming path, which has its
own flush (`streaming_helpers.rs`:339). The interior route was the only one
with a resolve past its flush.

Two guards, because neither alone is enough:

* `debug_assert_eq!(pending_dds_upload_count(), 0)` at the end of
  `load_cell_with_masters` — the real invariant, but nothing in `cargo test`
  loads a cell (needs Vulkan + game data), so it never runs in CI;
* `interior_water_spawns_before_the_reference_load_flush` — reads the
  function's own source and requires the spawn to still precede the reference
  load. Coarse by design; it exists because the alternative is no CI check.

`flush_pending_uploads` `mem::take`s its pending list before doing any work,
so the count is 0 after a flush regardless of success — the assertion can only
fire on a *new* resolve below the boundary, which is what it is for.

## #3321 — FNV ships unconsumed distant-object LOD (FIXED)

Corpus re-derived first, as the orchestrator note required
(`crates/bsa/examples/probe_lod_corpus.rs`, new):

```text
FNV   landscape\lod entries 2663    _far.nif 0    distantlod\ 0
        terrain wastelandnv 1360   level4 1024 / 8 256 / 16 64 / 32 16
        blocks  wastelandnv  295   level4                (+7 worldspaces, 355 total)
FO3   landscape\lod entries 2232    _far.nif 2    distantlod\ 0
        blocks  across 15 worldspaces, level4 and level8
```

The unconfirmed 295 is exact. Two further corrections the issue did not have:

* the "2 `_far.nif`" `exal.md` attributed to FNV is **FO3's** — FNV ships zero;
* `washmontop` / `dcworld03` / `dcworld09`, which `exal.md` called "landmark
  sub-folders" folded into the terrain tree, are ordinary worldspace folders,
  each with its own `blocks\` sibling.

Implemented as an `ObjectLodScheme` arm rather than a new module: the two
schemes are the same shape (per-quad combined mesh + one shared worldspace
atlas) and differ only in naming and container, so they share `object_lod.rs`'s
residency, work budget and eviction. FO3/FNV ride the legacy band ladder their
terrain siblings use (`LodBandLadder::for_object_game`), which also covers
FO3's level-8-only worldspaces; Oblivion stays the only `None`, since its
`DistantLOD\*.lod` placement lists are genuinely per-object and stay in
`placement_lod`.

`object_lod_scheme_table` pins that any game declaring a scheme also has a
ladder — a game with one and not the other is silently dead, which is the
exact shape of the bug reported.

**Live verification** (`--game fnv --wrld WastelandNV --grid 0,0 --radius 3`,
`cc666a48` + this change):

```
Object-LOD bands @cell (0,0): +280 quads loaded, -0 unloaded
  (1080 tracked, levels 4..=32, 64 cells deep)
bench: entities=13827 meshes=1229 draws=4838/316b/13c
tex.missing: No missing textures
tex.loaded:  ...blocks\wastelandnv.buildings.dds resident
```

280 quads where the pre-fix engine loaded 0. The 800 remaining tracked entries
are empty sentinels for levels 8/16/32, which `wastelandnv` does not ship —
the same shape Skyrim already has for its absent level-32 `.bto`.

Gotcha for anyone repeating this: `--game <key>` resolves
`assets/debug_profiles.toml` relative to **CWD**, so it must run from the repo
root. Run from the game's `Data/` dir it fails with
`profile not found ... Known keys: []` and loads a 6-entity empty scene.
