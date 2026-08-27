# FNV-2026-08-26-D6-01

**Issue**: #3316
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: HIGH
**Dimension**: 6 — Animation, Skinning & Particles
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/src/anim/transform.rs:46`

**Premise verified**: `extract_transform_channel_at` reaches the
`NiTransformInterpolator` arm and immediately does

```rust
if let Some(interp) = scene.get_as::<NiTransformInterpolator>(interp_idx) {
    let data_idx = interp.data_ref.index()?;      // ← null data_ref ⇒ whole channel = None
    let data = scene.get_as::<NiTransformData>(data_idx)?;
```

`interp.transform` (the block's own `NiQuatTransform`) is never read. The
sibling arms *do* implement the fallback — `NiLookAtInterpolator` routes to
`constant_transform_channel` (`transform.rs:80`, #604), the B-spline arm has
its own static-pose path (`bspline.rs:328`), and `extract_bool_channel_at`
(`channel.rs:368-374`) explicitly falls back to `interp.value`. So the pose
fallback exists in three places in this file and is missing from the single
most common one. `import_sequence` (`sequence.rs:47`) only inserts a channel
when the extractor returns `Some`, and `clip_has_data` (`entry.rs:85`) drops
the whole clip when no channel survives.

**Spec** (not a guess):
- nif.xml `<niobject name="NiTransformInterpolator">` (line 3248) carries
  `Transform` *and* `Data`; the sibling `NiPoint3Interpolator` (line 3256)
  documents the identical field as **"Pose value if lacking NiPosData"** with
  `default="#INV_VEC3#"` = `-3.402823466e+38` (nif.xml line 85) — i.e. FLT_MAX
  is the "no pose" sentinel, any other value is an authored pose.
- Gamebryo v3.2 reference doc
  `gamebryo-v32/Documentation/Reference/NiAnimation/NiTransformInterpolator.htm`:
  *"NiTransformInterpolator supports the animation of position, rotation,
  and/or scale using an NiQuatTransform. **This NiQuatTransform can be an
  unchanging pose** or interpolated from animation key channels."* v2.6's
  header exposes `SetPoseValue/SetPoseTranslate/SetPoseRotate/SetPoseScale`
  and `GetChannelPosed()` (`gamebryo-v26/NiAnimation/NiTransformInterpolator.h:56-80`).
- The repo's own `FLT_MAX_SENTINEL` doc-comment (`channel.rs:413-425`) already
  states the intended model: real value → key, FLT_MAX → nothing.

**Evidence** (vanilla `Fallout - Meshes.bsa`, 14 881 NIFs / 4 316 KFs):

```
$ ... --example _tmp_fnv_d6_nulldata_pose -- "Fallout - Meshes.bsa"
NiTransformInterpolator with NULL data_ref = 50836
  all-axes FLT_MAX (nothing to recover) = 11
  at least one REAL authored axis        = 50825      ← 99.98%
  real translation=31610 real rotation=49064 real scale=5830

$ ... --example _tmp_d6_cb_drop -- "Fallout - Meshes.bsa"
controlled blocks -> NiTransformInterpolator: 123729
  dropped (null data_ref): 49182  (39.7%)
files with >=1 dropped: 3868 / 4316
files where ALL transform channels dropped: 199

$ ... --example _tmp_fnv_d6_kf_health -- "Fallout - Meshes.bsa"
kf files=4316 parsed=4316 zero_clip_files=64
```

All 64 zero-clip files DO carry a `NiControllerSequence` with controlled
blocks (`_tmp_fnv_d6_zeroclip` → `{"has 1 seq, total cbs=>0": 64}`); they
produce nothing solely because every channel is posed. Spot checks:

```
meshes\characters\_male\2hrdeath.kf:      clips=0  tot_cb=2 null=2 real_pose=2  (Bip01 Pelvis, Bip01 NonAccum)
meshes\characters\_1stperson\mtjumploop.kf: clips=0 tot_cb=2 null=2 real_pose=2 (Camera1st, Bip01 NonAccum)
meshes\creatures\robobrain\1hpaimup.kf:   clips=0  tot_cb=1 null=1 real_pose=1  (Bip01)
meshes\characters\_male\pa2hmholster.kf:  clips=0  tot_cb=2 null=2 real_pose=2  (Weapon, Bip01 NonAccum)
```

**Impact**: this hits the only two FNV clips the engine actually plays today.

```
meshes\characters\_male\locomotion\mtidle.kf          clips=1 chans=[56] null_data=4/8  all posed-real
   dropped-but-posed bones: Bip01 L Toe0, Bip01 Neck, Bip01 L Clavicle, Bip01 R Clavicle
meshes\characters\_male\idleanims\chairskirt_leftenter.kf clips=1 chans=[27] null_data=30/32 all posed-real
   dropped-but-posed bones: Bip01 L Thumb1, L Finger1/11/12/2/21/22/3, ...
```

- `mtidle.kf` is `humanoid_default_idle_kf_path` — every KF-era NPC in every
  FNV cell. Its neck and both clavicles hold at the *mesh bind pose* instead
  of the idle's authored pose, so every standing NPC's head/shoulder set is
  subtly wrong relative to vanilla.
- `chairskirt_leftenter.kf` is `sandbox_sit_enter_kf_path`, the clip
  `sandbox_seat_system` parks a seated actor on. 30 of its 32
  `NiTransformInterpolator` channels are finger/thumb poses — a seated actor
  gets flat bind-pose hands instead of the authored posed hands.
- Forward-looking: 64 `.kf` files (incl. `2hrdeath.kf`, holster/aim clips,
  first-person jump loop) are *literally not importable* today, and the
  general 39.7% loss will land on every future combat/locomotion clip.

**Fix sketch**: in the `NiTransformInterpolator` arm, replace the `?` on
`data_ref` with a fall-through to the existing
`constant_transform_channel(&interp.transform)` (which already applies the
FLT_MAX-per-axis gate). Second-order refinement matching Gamebryo's
`GetChannelPosed`: when `NiTransformData` *is* present but a specific TRS
channel has zero keys, fill that channel from the pose instead of leaving it
empty. Add a regression test on `NiTransformInterpolator` — the two existing
#772 tests (`anim/tests/transform.rs:266,314`) only construct
`NiLookAtInterpolator`, which is why this never tripped.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
