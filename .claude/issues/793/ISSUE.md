# M41-HANDS: NPC spawn loads upperbody.nif but not lefthand.nif / righthand.nif (kf-era hand mesh gap)

Labels: enhancement medium legacy-compat 
State: OPEN

**Severity**: MEDIUM (visible cosmetic gap on every kf-era NPC; gameplay-blocking once weapon-held animations land)
**Source**: Live diagnosis at `GSDocMitchellHouse` after #772 close (2026-05-03). Doc Mitchell renders solid (vanish bug fixed) but both arms cut off at the wrist.
**Game affected**: FNV verified; FO3 likely same shape (same `humanoid_body_path` arm of `crates/...npc_spawn.rs`); Oblivion needs verification.

## Summary

`npc_spawn::humanoid_body_path` ([byroredux/src/npc_spawn.rs:94-105](byroredux/src/npc_spawn.rs#L94-L105)) returns a single canonical body path:

```rust
(GameKind::Oblivion | GameKind::Fallout3NV, _) => Some(r"meshes\characters\_male\upperbody.nif")
```

But FNV's `Fallout - Meshes.bsa` ships hands as **separate NIFs** (verified via `cargo run -p byroredux-nif --example listbsa`):

```
meshes\characters\_male\upperbody.nif       ← currently loaded
meshes\characters\_male\lefthand.nif        ← missing
meshes\characters\_male\righthand.nif       ← missing
meshes\characters\_male\femaleupperbody.nif ← missing (gender path)
meshes\characters\_male\childupperbody.nif  ← missing (child variant)
```

The hand bones (`bip01 r hand`, `bip01 l hand`) resolve correctly in the skeleton subtree (#772 diagnostic confirmed bind-close transforms at entity 104 / 83), so the skeleton side is sound. The geometry side simply isn't loaded.

## Game impact

Every kf-era NPC renders without hands — Doc Mitchell, Sunny Smiles, every Goodsprings resident, every Megaton dweller. Currently masked because vanish bug (#772) was hiding all bodies; now that NPCs render, the missing-hand gap is the next visible defect.

## Suggested Fix

Extend `humanoid_body_path` from returning a single path to returning a **slice of paths**, then update the body-load loop at `npc_spawn.rs:428-465` (the `(_body_count, body_root, _body_map) = load_nif_bytes_with_skeleton(...)` call) to iterate. Each NIF parents to `placement_root` and shares the existing `skel_map` — same shape as the current upperbody load, just repeated.

Per-game path lists (vanilla content, mods can add more later):

| Game | Gender | Paths |
|---|---|---|
| FO3 / FNV | Male | `_male\upperbody.nif`, `_male\lefthand.nif`, `_male\righthand.nif` |
| FO3 / FNV | Female | `_male\femaleupperbody.nif`, `_male\lefthand.nif`, `_male\righthand.nif` |
| FO3 / FNV | Child | `_male\childupperbody.nif`, `_male\childfemaleupperbody.nif` (gender-split), hands TBD |
| Oblivion | both | needs verification — Oblivion uses a different mesh layout |

Foot meshes likely have the same shape. The `listbsa` output also shows `_male\headhuman.nif`-equivalent paths but those are the head (already handled by the RACE/MODL path).

## Locations

- [byroredux/src/npc_spawn.rs:82-105](byroredux/src/npc_spawn.rs#L82-L105) — `humanoid_body_path` (path resolver)
- [byroredux/src/npc_spawn.rs:390-465](byroredux/src/npc_spawn.rs#L390-L465) — body-load site (iterate this)
- [byroredux/src/scene.rs](byroredux/src/scene.rs) — `load_nif_bytes_with_skeleton` (no changes; just called more times)

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Foot meshes — same gap shape if they exist as separate NIFs (`feet.nif` / `lfoot.nif` / `rfoot.nif`). Verify via `listbsa` and extend the path list if so.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Integration test on Goodsprings / Doc Mitchell — assert `bip01 r hand` and `bip01 l hand` entities have rendered descendants (i.e. the hand mesh actually attaches and renders, not just bones present).