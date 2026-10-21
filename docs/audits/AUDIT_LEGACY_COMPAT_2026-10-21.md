**HEAD**: 4a3e89d56 · **Baseline**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-12.md` (HEAD `5d31fd637`) · **Audited**: Dim 1 (the REFR convention flip `2a8b8e505` and the new Euler inverse `d4bfe6abc`, plus an added-line scan of all 784 commits), Dim 2 (the new ObScript `PlayGroup` sequence consumer `f3380f6c8` and the spawn-clip merge it builds on) · **Unchanged since baseline (skimmed)**: Dim 3 (Pattern A grep clean; the in-range game branches are single decisions or data tables; one size-discriminated decode spot-checked)

# Legacy Compatibility Audit — 2026-10-21

## Method

This run is delta-scoped against the 2026-10-12 baseline. 784 commits are in range (`5d31fd637..4a3e89d56`).

- **Legacy source.** The Gamebryo 2.3 drive is not mounted. I relied on the `efd.lib`/`efd/Matrix3.h` citations recorded in `2a8b8e505`, `/mnt/data/src/reference/gamebryo-v26` / `-v32`, and `docs/legacy/`.
- **Raw-data probe.** I wrote one throwaway, read-only crate at `/mnt/data/tmp/suite-1021-legacy-compat/` (repo `Cargo.lock` copied in, its own `target/`). It has four binaries over the vanilla `Oblivion - Meshes.bsa`, the FNV and FO3 `Fallout - Meshes.bsa`, and the `byroredux-nif` import API:
  - `lc-seq-probe`: sink and standalone-channel coverage of named sequences.
  - `envelope`: timing envelope and non-transform loss in the merged spawn clip.
  - `dump`: per-clip channel sets.
  - `flags` / `vis`: authored `APP_CULLED` flags against the sequence's t=0 visibility.
  
  The outputs are in `/tmp/audit/legacy-compat/probe_*.txt`.
- **Tests run** (1.96.0 toolchain, repo `target/`):
  - `cargo test -p byroredux-core --lib -- math::coord components::camera`: 47 passed.
  - `cargo test -p byroredux-nif --lib -- strip rotation import::coord`: 67 passed.
  - `cargo test -p byroredux-nif --lib -- anim`: 97 passed.
  - `cargo test -p byroredux --bin byroredux -- euler frustum rotation_zup trigger_volume ragdoll_pose`: 42 passed, 2 ignored.
- **Dedup sources:**
  - `/tmp/audit/issues.json` (119 open).
  - `gh` closed searches (`BMDT`, `PlayGroup`, `mesh.info euler`, sequence merge).
  - `docs/audits/` greps.
  - `/tmp/audit/SUITE_FINDINGS.md`, read twice, the second time just before finalizing. Nothing overlaps.

No source, skill or issue was modified, and no engine was launched. Scratch notes are in `/tmp/audit/legacy-compat/dim_{1,2,3}.md`.

## Executive Summary

| Severity | NEW | Existing / cross-referenced |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 2 | 0 |
| LOW | 3 | 0 |

**Headline.** Both MEDIUM findings are in the object-animation path that Gamebryo's `NiControllerManager` drives.

1. **The cell loader's spawn clip loses the first sequence's timing envelope and every non-transform channel whenever the NIF also has standalone controllers.** When a mesh carries both an `NiControllerSequence` and standalone property controllers, the clip it spawns with keeps the standalone clip's cycle type and drops every visibility, alpha, UV and flipbook channel the sequence authors.
   - FNV's NCR, Nevada and FOA flags wave once and freeze, because their `Idle` loop becomes `Clamp`.
   - Oblivion's gates (`oblivionarchgate01`, `obliviongate_simple`, `obliviongate_forming`) play `SpecialIdle` once and lose up to 64 visibility tracks.
   - This dates from `fb74b409b` (2026-08-30) but was never filed.
2. **The new ObScript `PlayGroup` swaps the player to a bare sequence clip.** It never attaches that clip's sinks, so its alpha, visibility and UV values are discarded. It also stops the standalone effects (gate tendril flipbooks, magma emitters) that the spawn clip was playing.

**Coordinates are correct after the convention flip.** `2a8b8e505` restores Gamebryo's `FromEulerAnglesXYZ` product as the ship mode, and `d4bfe6abc` adds its exact inverse.
- I re-derived the inverse's matrix terms by hand (`R[0][1] = −sin b`, `R[1][1] = cos a cos b`, `R[2][1] = sin a cos b`, `R[0][0] = cos b cos c`, `R[0][2] = cos b sin c`), including the gimbal branch.
- Every REFR-family caller still goes through `euler_zup_to_quat_yup_refr`: `refr.rs`, `references/mod.rs`, `placement_lod.rs`, `loading_screen.rs`, `transition.rs` and `light_anim.rs`.
- The ObScript angle commands use the canonical helper and its inverse.
- What remains from the flip is residue: two new hand-rolled axis swaps, a debug readout that never showed the REFR triple, and prose that still calls ZYX the ship.

**Prior findings.** `LC-2026-10-12-D3-01` (Oblivion `ARMO` `BMDT`) is CLOSED as `#5696`. The fix is in place: `items.rs:537-549` reads the `u16` slots, and the fixture uses the real `03 00 80 00` bytes. `#4128` (NiControllerManager/KFM state machine) is still open. `PlayGroup` is its first real consumer for objects; the findings below are the fidelity gaps in that consumer, not the actor-KFM gap `#4128` tracks.

---

## Dimension 1 — Coordinate + placement fidelity

**Listed-path commits in range** (2):
- **`2a8b8e505`** sets `REFR_ROTATION_MODE_SHIP = 0`: CW angles, product `Rx(−rx)·Rz(ry)·Ry(−rz)` in Y-up, which is `Rx·Ry·Rz` in Z-up. The 2026-05-26 ZYX product moves to the A/B-only mode 1.
  - The new guard `euler_matches_gamebryo_from_euler_angles_xyz` applies independent `efd::Matrix3` CW matrices over five multi-axis triples, including the `GSProspectorSaloonInterior` window-wall REFR `00108729`.
  - `zyx_product_disagrees_with_gamebryo_on_multi_axis` and `modes_zero_and_one_agree_only_on_z_only_rotations` keep the guard able to tell the modes apart.
  - The out-of-range fallback in `euler_zup_to_quat_yup_mode`'s `_ =>` arm now lands on mode 0. `rotation_mode_arg` is still unclamped.
- **`d4bfe6abc`** adds `quat_yup_to_euler_zup`, pinned by `the_euler_extraction_inverts_the_refr_convention`. ObScript `GetAngle`, `SetAngle`, `Rotate` and `GetStartingAngle` (`obscript_effects.rs:1009-1027`, `:1408-1421`) round-trip through it and the canonical helper. They act only on a root, non-player entity, so local equals global.
- **`XRGD_ROTATION_MODE = 0`** (`ragdoll_pose.rs:415`) is hardcoded on purpose. Its convention was derived independently from corpse data, not from REFRs, so it must not follow `REFR_ROTATION_MODE_SHIP` if the ship mode is ever flipped again. That makes it the correct shape, not a hardcoded-mode finding. Since `2a8b8e505` it coincides with the ship mode; see D1-03 for the prose this left stale.

**Added-line scan** of the 784-commit diff for `(x, z, -y)`, `4096.0`, `from_mat3`, `EXTERIOR_CELL_UNITS`, `zup_to_yup`/`yup_to_zup` and `normalize_quat`:
- New `4096.0` literals are tests or distances: `groundcover.rs:1642`, `assemble_camera_and_lights.rs:980`, and the volumetric `far` fixtures.
- New world→grid floors all use the constant with the correct per-space sign. Z-up sites floor `[0]`/`[1]` (`exterior.rs:1336`); Y-up sites floor `x` / `−z` (`streaming::world_pos_to_grid`).
- `normalize_quat` has no new caller.
- `destrip` is still single-implementation.
- New `Quat::from_mat3` sites are confined to `crates/nif/src/import/collision/ragdoll.rs:517,605`. The second one composes NIF rotations with no SVD repair, which is already NIF-D5-2026-10-21-02 (`node_world` duplicates `compose_transforms`); I am not re-filing it.

### LC-2026-10-21-D1-01: Two new hand-rolled `(x, z, −y)` swaps bypass `zup_to_yup_pos`
- **Severity**: LOW
- **Dimension**: 1 — One implementation
- **Location**: `byroredux/src/cell_loader/tree_lod.rs:263-264` (`a9384f9c0`, 2026-10-14); `byroredux/src/systems/cinematic.rs:524-526` (`696da4f47`, 2026-10-13, `ApplyHavokImpulse`)
- **Status**: NEW
- **Description**: The Z-up → Y-up swap has one home, `byroredux_core::math::coord::zup_to_yup_pos`. That is the #1044 consolidation, and it is why the #333-class fixes reach every site. Two commits in range write the swap inline instead:
  - `let base = Vec3::new(x, z, -y);` places the tree-LOD billboard.
  - `Vec3::new(x, z, -y) * factor` builds a scripted Havok impulse.
- **Evidence**: An identifier-level grep for the swap outside `coord.rs` and tests finds only these two production sites. Every other hit is a comment or test assertion, and `precombined.rs:1805,1819` (2026-08-10) sits inside a bound-validation helper.
- **Impact**: Both are correct today. The risk is drift: a future change to the canonical conversion (unit scale, sanitising, a basis change) would miss them, which is the failure mode the skill was written to prevent.
- **Related**: #1044, #1112. The hand-rolled `(cos, 0, −sin)` plan directions beside the tree swap carry a yaw sign whose source is not cited; that question belongs to `/audit-exterior`.
- **Suggested Fix**: Route both through `zup_to_yup_pos`. The tree site is `Vec3::from(zup_to_yup_pos(tree.position))`; the impulse site is `Vec3::from(zup_to_yup_pos(request.vector)) * factor`.

### LC-2026-10-21-D1-02: `mesh.info` prints a glam ZYX decomposition of the Y-up quaternion as if it were the REFR `DATA` triple
- **Severity**: LOW
- **Dimension**: 1 — REFR Euler (diagnostic surface)
- **Location**: `byroredux/src/commands/assets.rs:659-676`; `docs/engine/debug-cli.md:553-555` (which also names the wrong file, `commands/mod.rs`)
- **Status**: NEW (the code dates from `9114da751`, 2026-05-26; no issue or audit mentions it)
- **Description**: The comment says the readout "matches the FNVEdit / CK convention the user sees in the ESM (REFR DATA stores RX RY RZ as ZYX-composed Euler)". The code is `t.rotation.to_euler(EulerRot::ZYX)` on the engine's Y-up quaternion, printed as `rx/ry/rz`. These are angles about Y-up axes in glam's `Rz·Ry·Rx` order, not the Z-up REFR triple, under either the old or the new ship mode.
- **Evidence**: A REFR with only a heading `rz = θ` spawns as `Ry(−θ)` in Y-up (true in both modes). `to_euler(ZYX)` returns `(z=0, y=−θ, x=0)`, so `mesh.info` prints `rx=0 ry=−θ rz=0`: the heading shows on the wrong axis with the wrong sign. `quat_yup_to_euler_zup` (added in `d4bfe6abc`) is now the exact inverse of the shipping conversion.
- **Impact**: This is a diagnostic only, but it is step 3 of `docs/engine/geometry-defect-triage-workflow.md` (`mesh.info` on the picked ref). An operator comparing its rotation with FNVEdit's REFR `DATA` sees a mismatch on every rotated REFR, including correctly placed ones. That pushes them toward the false "rotation bug" conclusions the `--rotation-mode` dispute produced twice.
- **Related**: `2a8b8e505`, `d4bfe6abc`.
- **Suggested Fix**: Print `quat_yup_to_euler_zup(t.rotation)` in degrees and label it "REFR DATA convention". Correct `debug-cli.md:553-555` and its file path.

### LC-2026-10-21-D1-03: Prose and test docs still name ZYX / mode 1 as the shipping REFR convention
- **Severity**: LOW
- **Dimension**: 1 — REFR Euler (doc rot after `2a8b8e505`)
- **Location**:
  - `byroredux/src/cell_loader/transition.rs:410` and `:706`: "the shipping default (`--rotation-mode 1` ≡ the plain helper's formula)".
  - `byroredux/src/cell_loader/references/trigger_volume_tests.rs:80`: "same shipping mode (1 = CW+ZYX)".
  - `byroredux/src/ragdoll_pose_tests.rs:46-53`: "this convention is not the REFR `ZYX` product". The test compares XRGD against mode 1, which is no longer the REFR convention; XRGD now *equals* the REFR ship.
  - `docs/engine/geometry-defect-triage-workflow.md:146-150`: "The 2026-05-26 ZYX-OpenMW fix … was the canonical mode flip for FNV — pre-fix content rendered with mode 0 would show this class". This is inverted.
  - `crates/plugin/tests/parse_real_esm.rs:3698`: "post the 2026-05-26 fix".
  - `crates/plugin/examples/cell_rot_sweep.rs:1-6`: "post-mode-1 … shipped ZYX-vs-XYZ flip".
- **Status**: NEW
- **Description**: `2a8b8e505` updated `coord.rs`, `euler.rs`, `boot/`, `coordinate-system.md` and the CLI docs, but not these sites. The functional assertions are still true: the dispatcher test is a source scan, and the trigger-volume test uses a Z-only rotation, on which modes 0 and 1 agree. Only the prose is wrong.
- **Impact**: The triage workflow tells an operator that mode-0 output is the known-bad class. That is exactly backwards for the next "walls rotated" report, and it is the doc most likely to be read in that situation.
- **Related**: D1-02; memory note *gamebryo_cw_rotation*.
- **Suggested Fix**: Re-word the six sites to name mode 0 / XYZ as the ship. Rename the ragdoll-pose test (for example, `rotation_is_the_gamebryo_xyz_product_not_zyx`) and say in its doc that XRGD and REFR now share the convention.

---

## Dimension 2 — Legacy subsystem coverage (NiControllerManager / sequence playback)

**Gamebryo behaviour.** In Gamebryo, an `NiControllerManager` owns a mesh's `NiControllerSequence`s. Each sequence carries its own cycle type and frequency, plus interpolators for *every* controller kind it drives: transform, `NiVisController`, `NiAlphaController`, `NiTextureTransformController`, `NiFlipController`, and so on. Activating a sequence (an object script's `PlayGroup`) blends that whole sequence in. Standalone controllers outside the manager keep running independently.

**Redux behaviour.** The cell loader builds one clip per mesh with `import_embedded_animations_with_sequences` and attaches the `Animated*` sinks for that clip only (`spawn.rs:899-914`). `f3380f6c8` adds `NamedSequences` and ObScript `PlayGroup`/`IsAnimPlaying` (`crates/scripting/src/obscript_anim.rs`). The probe measured how both paths treat real content.

Other in-range Dim 2 commits are clean:
- `ce08ec0cf` (the importer now emits the core key types; `#4167` guards still compare them).
- `d58e7dbad` (`attach_clip_sinks`).
- `7822695e4` and `9b9b27d3a` are importer comments and the hair-mask bake, owned by NIFAL.

### LC-2026-10-21-D2-01: The spawn clip of a mesh with both standalone controllers and a sequence drops the sequence's cycle type and all of its non-transform channels
- **Severity**: MEDIUM. Authored animation data is parsed, then dropped at the merge, with visible results: flags and gates stop animating after one cycle, and emitters the sequence hides keep emitting.
- **Dimension**: 2 — Animation model (timing envelope + channel kinds)
- **Location**: `crates/nif/src/anim/entry.rs:832-856` (`import_embedded_animations_with_sequences`), and its premise in the doc comment at `:820-823`. Callers: `byroredux/src/cell_loader/references/import.rs:101-102`, `byroredux/src/streaming/pre_parse.rs:326`.
- **Status**: NEW. The code dates from `fb74b409b` (#3602, 2026-08-30). No open or closed issue covers it.
- **Description**: When a NIF yields a standalone-controller clip, the merge keeps that clip and copies in only the first sequence's `channels` (transform), `duration` (max of the two), `accum_root_name` and `text_keys`. Three things are lost:
  - The sequence's `float_channels`, `color_channels`, `bool_channels` and `texture_flip_channels`.
  - The sequence's `cycle_type`, `frequency` and `phase`. The standalone clip's envelope wins.
  
  The doc's premise, "sequences carry the `channels` (transform) map, standalone controllers carry the float/color/bool/texture-flip vectors", is false on vanilla content. The one test, `embedded_import_with_sequences_picks_up_a_nicontrollersequence`, only covers the branch where the standalone clip is `None`.
- **Evidence** (probe `envelope`, object meshes only, characters and creatures excluded):

  | Archive | NIFs with both | Envelope differs | Sequence non-transform channels dropped |
  |---|---:|---:|---:|
  | Oblivion | 11 | 4 | 11 |
  | FNV | 7 | 5 | 1 |
  | FO3 | 1 | 0 | 0 |

  - FNV `clutter\flags\nv_ncr_flag.nif`, `nv_nevadaflag.nif` and `nv_foa_flag.nif`: standalone `Clamp` against `Idle` `Loop` (9-10 transform channels). The flag wave plays once and stops.
  - Oblivion `oblivionarchgate01.nif`: standalone `Clamp` (dur 1) against `SpecialIdle` `Loop` (dur 5, 107 transform channels). The merged clip is `Clamp`, dur 5. All 64 `SpecialIdle` visibility channels are dropped. At t=0, `SpecialIdle` hides `dustEmitter` and `MagmaEmitter`, but both are visible by NIF flags, so they render and emit when the authored idle hides them.
  - `obliviongate_simple.nif` and `obliviongate_forming.nif` show the same pattern.
  - In the other direction, FNV `dlc04brainpod01.nif` and `braincontroldevice01.nif` (`Talking`, `Clamp`) are forced to `Loop`.
- **Impact**:
  - Placed vanilla flags across FNV exteriors stop waving after their first cycle.
  - Every Oblivion gate's idle (rotating rubble and tendril transforms) runs once and freezes, and shows effects the idle sequence keeps hidden.
  - The same merged clip is what the streaming pre-parse registers, so exterior streaming is affected too.
- **Related**: #3602 (the fix that introduced the merge), #3345 (phase), D2-02, #4128.
- **Suggested Fix**:
  - Stop merging two animations into one clip. Spawn two players (or two `AnimationStack` layers) on the placement root: the standalone-controller clip with its own envelope, and the first sequence's clip intact.
  - At minimum, extend the merge to the four non-transform vectors and keep the *sequence's* envelope for its channels.
  - Add a test with both sources present that asserts the sequence's `bool_channels` and `cycle_type` survive.

### LC-2026-10-21-D2-02: ObScript `PlayGroup` swaps to a bare sequence clip — its non-transform channels have no sinks, and the standalone effects stop
- **Severity**: MEDIUM. This is the newest consumer of the legacy sequence model, and it silently discards authored visibility, alpha and UV animation on the content it targets (Oblivion gate open/close, FNV terminals and holograms).
- **Dimension**: 2 — Animation model (`NiControllerManager` sequence activation)
- **Location**:
  - `crates/scripting/src/obscript_anim.rs:72-88` (`play_group`).
  - `byroredux/src/cell_loader/spawn.rs:899-922`: sinks are attached for the spawn clip only, then `NamedSequences` is inserted.
  - `byroredux/src/cell_loader/object_sequences.rs` (`named_sequences_for`).
  - Commit `f3380f6c8` (2026-10-18).
- **Status**: NEW
- **Description**: `play_group` looks up the named sequence's handle and writes it into the placement's existing `AnimationPlayer.clip_handle`, resetting time to 0. Two legacy behaviours do not survive the swap:
  1. **Missing sinks.** `animation_system` can only write into an `Animated*` component that already exists (`anim_convert.rs` #2221 contract). The sinks were attached at spawn for the spawn clip's channels only. A named sequence that animates a target or channel kind the spawn clip doesn't therefore resolves and samples its value, then throws it away.
  2. **Standalone effects stop.** The spawn clip is the standalone-controller clip merged with the first sequence (see D2-01). The named clip is the sequence alone. After the swap, every standalone property controller (flipbooks, UV scrolls, emitter visibility) stops on whatever value it last wrote, although in Gamebryo those controllers run independently of the manager.
- **Evidence** (probe `lc-seq-probe`, multi-sequence object NIFs: FNV 453, Oblivion 214):
  - Sequences with sinks missing: FNV 2 NIFs, Oblivion 7.
    - FNV `furniture\tlpod01.nif` `Idle` (`Monitor:8` UV offset and scale) and `dlc05\effects\dlc05alienhologramhead.nif` `Forward`/`Backward`/`Left`/`Right` (`newHead01yellowRim` alpha).
    - Oblivion `obliviongate_forming.nif` `Forward`/`Backward`/`Equip`/`Unequip` (`CollapsingEnergyPlane` visibility, `FlashPlane:0` alpha, `GateBladeLeft` visibility), `obliviongate_simple.nif` `Forward`/`SpecialIdle`, and `magiceffects\shockshield.nif`.
  - Sequences that drop standalone channels the spawn clip plays: FNV 1, Oblivion 5. Examples are the `Tendrils02..04:0` flipbooks on `obliviongate_forming`, `oblivionarchgate01` and `obliviongate_simple`, and the `MagmaEmitter02-04` visibility on `oblivionwargateani02`.
  - Script usage: in the ESM text, `playgroup forward 0|1` appears 427 times in `Oblivion.esm` and 236 times in `FalloutNV.esm`, and `Forward` is index 3, inside the Oblivion limit of 5 that `group_name` allows.
- **Impact**: Once an object script plays a group, these effects freeze for the rest of the session:
  - The flash and blade visibility an Oblivion gate's open animation authors never applies.
  - The gate's tendril flipbooks stop.
  - A terminal pod's monitor scroll and the alien hologram's rim fade stop.

  The `IsAnimPlaying` answer stays correct; only the visuals are wrong.
- **Related**: D2-01 (the same "one clip for two animations" model), #4128 (actor-side KFM state machine), #2221 (sink contract).
- **Suggested Fix**:
  - When `named_sequences_for` registers a model's sequences, call `attach_clip_sinks` for *each* sequence handle on that placement. Sinks are insert-if-missing, so this is idempotent.
  - Run sequences on their own player (or stack layer) so that `PlayGroup` replaces only the sequence and leaves the standalone-controller player running.

---

## Dimension 3 — Cross-game translation patterns (no findings)

- **Pattern A.** `grep -rnE 'bsver(\(\))? *[<>=]+ *[0-9]{2,3}\b' crates/nif/src`, with comment lines removed, returns only the three test strings in `blocks/base.rs:530,550,551`. Clean.
- **New game-keyed branches in `byroredux/src`**:
  - `cell_loader/load_order.rs:414` picks `FormIdScheme::Starfield` once. The wire format justifies it: only Starfield stores small and medium masters in their own spaces, while FO4 and Skyrim ESLs are positional on disk (`reader.rs:479-492`).
  - `apply_exterior_processing` (Oblivion child-worldspace LAND inheritance, #5505) and the `exterior_ready` mirror at `:678` test the same predicate twice, ten lines apart. That is benign and is EXAL's to own.
  - `npc_spawn.rs` adds `GameKind`-keyed path tables (the sanctioned table shape) and one Starfield face-rig decision in `npc_skeleton_path`.
- **Pattern C spot-check.** `67433ee3a` (#6038, Skyrim `WTHR` `NAM0`/`DALC` older form versions) chooses the layout by payload size (13/14/17 groups, 24/32-byte `DALC`). Each size has a test (`parse_skyrim_older_nam0_decodes_the_authored_groups`, `parse_skyrim_24_byte_dalc_keeps_the_six_axes`), and the 240-byte FNV stride is explicitly rejected.
- The other in-range record decodes (#6120 SF `ACBS`, #6122 RACE `ANAM`, #6039–#6042 `WTHR`) are `/audit-esm`'s. Their cross-game splits are already filed as ESM-2026-10-21-02/03.

---

## Skill drift (not counted; for `/audit-sync`)

- The `audit-legacy-compat` SKILL says "XCLL lighting calls the canonical helper directly". It does not. `cell_loader/load.rs:245` `xcll_direction_yup` builds a spherical azimuth/elevation direction with its own inline `(x, z, −y)` (since `77b540d0a`, 2026-08-14), and no Euler helper is involved. The sentence should describe the spherical helper and its guard (`euler_zup_to_quat_yup_tests.rs`).
- The skill's Dim 1 `First step:` grep (`'(x, z, -y)'`) misses the identifier-level `Vec3::new(x, z, -y)` form that D1-01 found. Adding `'new(x, z, -y)'` would cover it.

## Scratch-file reconciliation

I checked `/tmp/audit/legacy-compat/dim_1.md`, `dim_2.md` and `dim_3.md` against this report:
- `dim_1` → D1-01, D1-02, D1-03, the NIF-D5-02 cross-reference and the skill-drift note.
- `dim_2` → D2-01 and D2-02.
- `dim_3` → no findings.

Everything appears above and nothing was dropped.

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md
```

Labels:
- **LC-2026-10-21-D2-01**: `medium` `bug` `legacy-compat` `animation` `nif` `game:oblivion` `game:fnv`
- **LC-2026-10-21-D2-02**: `medium` `bug` `legacy-compat` `animation` `scripting`
- **LC-2026-10-21-D1-01**: `low` `tech-debt` `legacy-compat`
- **LC-2026-10-21-D1-02**: `low` `bug` `legacy-compat` (`byro-dbg` command surface → `tech-debt` per the label map)
- **LC-2026-10-21-D1-03**: `low` `doc-rot` `legacy-compat`
