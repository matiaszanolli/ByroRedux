# #3602: OBL-D7-02: 792 embedded NiControllerSequence animations import at 100% but never reach the ECS on cell load — every animated Oblivion static spawns frozen

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 7 (Exterior Blocker Chain)
**Severity**: HIGH
**Location**: `byroredux/src/streaming.rs` and `byroredux/src/cell_loader/references/import.rs` (both call `import_embedded_animations`); `crates/nif/src/anim/entry.rs` (`import_embedded_animations` vs `import_kf`)

## Description

423 Oblivion mesh files carry 792 embedded `NiControllerSequence` animations. The importer
handles them perfectly — but **both** cell-loader NIF import sites call only
`import_embedded_animations`, which does not look at `NiControllerSequence` at all. Every
animated static spawns frozen.

## Evidence

Measured over `Oblivion - Meshes.bsa` (8,032 files):

```
files carrying NiControllerSequence : 423   (792 sequences, 423 NiControllerManager)
byroredux_nif::anim::import_kf(&scene) on those scenes:
    yields clips in 423 / 423 files  ->  792 clips   (100%)
    yields nothing in   0 files
```

Call sites, verified 2026-08-30:

```
byroredux/src/streaming.rs:1180                   -> import_embedded_animations(&scene)
byroredux/src/cell_loader/references/import.rs:99 -> import_embedded_animations(&scene)

byroredux/src/scene.rs:1015        -> import_kf   (external .kf path)
byroredux/src/npc_spawn.rs:507     -> import_kf   (NPC spawn)
byroredux/src/systems/animation.rs -> import_kf   (test)
```

`import_embedded_animations` (`crates/nif/src/anim/entry.rs`) handles only the standalone
single-interp controllers — `NiFlipController`, `NiMaterialColorController`,
`NiTextureTransformController`, `NiSingleInterpController`, `NiLight*Controller`,
`BsShaderController` — and yields a clip in just **72** of 8,032 files. `import_kf` is
reachable only from the external `.kf` path, NPC spawn, and the `--kf` CLI flag: never from a
placed REFR.

Name resolution is **not** the problem, and was measured to rule it out: across all 8,032
meshes the embedded clips produce **637 transform channels, 637 of which resolve to a real
`NiNode` name — 0 unresolved.**

## Impact

Oblivion's animated statics — Oblivion gates, machinery, banners, the
`obgate*`/`oblivionarchgate*` family — spawn frozen. The animation data parses, imports and
resolves; it is simply never handed to the `AnimationClipRegistry` for a placed REFR. This
sits on the exterior blocker chain (Oblivion gates are exterior landmarks).

**Likely shared with FO3/FNV/Skyrim**, which embed sequences too; measured here on Oblivion
because that is this audit's corpus.

## Suggested Fix

Route embedded `NiControllerSequence` clips through the cell-loader import: at both sites,
take `import_kf`'s clips in addition to `import_embedded_animations`' single-interp clip, and
register them against the placed REFR's entity.

## Related

`import_kf` / `import_embedded_animations` (`crates/nif/src/anim/entry.rs`, #1673).

## Completeness Checks
- [ ] **SIBLING**: both call sites must change together — `streaming.rs` (exterior stream tiles) and `cell_loader/references/import.rs` (cell load); a fix at one leaves the other frozen
- [ ] **LOCK_ORDER**: registering clips touches `AnimationClipRegistry` from the load path — preserve TypeId-sorted acquisition against the storages already held
- [ ] **TESTS**: a regression test pins a placed REFR of an `obgate*` mesh receiving a non-empty `AnimationPlayer` clip set
