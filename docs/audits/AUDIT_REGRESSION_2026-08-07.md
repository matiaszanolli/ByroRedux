# Regression Verification Audit — 2026-08-07

**Scope**: Verify that previously-closed bug/documentation fixes have not
regressed. Discovery per `.claude/commands/audit-regression/SKILL.md` Step 1:
`gh issue list --state closed --label bug --limit 50` and
`--label documentation --limit 50` run **separately** (the `gh` CLI ANDs a
comma-separated `--label` list rather than OR-ing it, so `--label
bug,documentation` alone would have returned only the 11 issues carrying
*both* labels), merged and de-duplicated, then the 50 most-recently-closed
taken as the default-`--limit 50` discovery window (covers #2279–#2405,
2026-08-04 through 2026-08-07). Plus the skill's explicit "fresh verification
candidates" (#1815, #1816, #1728, #1740, #1731, #1718) and the #1651/#1823
special case, plus the unconditional Step 4 fragile-area checks that guard
refactors never filed as GitHub issues.

**Method**: four parallel fix→guard-test verification passes (batch 1: 17
issues #2405→#2308; batch 2: 17 issues #2307→#2329; batch 3: 16 issues
#2328→#2340; fresh-candidates+#1651-revert: 7 items), each walking `git log
--grep "#<N>"` → read live code at the named symbol → locate the guard test →
run it. The auditor independently re-verified a representative cross-section
of every pass's claims directly (not merely accepting sub-agent summaries at
face value) before compiling this report: re-ran `git show <hash> --stat` for
6 of the bundled commits, re-ran 4 guard tests directly (`byroredux-audio
reverb_send_gate_matches_silence_db_boundary`, `byroredux
every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`,
`fo3_parallax_authored_requires_one_of_the_two_flag_bits`, the `strip::`
destrip module), and read the live #1651/#1823 revert site. All spot-checks
matched the reported claims exactly (same commit hashes, same fix
descriptions, same test names, tests green). Step 4 was run directly by the
auditor, not delegated.

**Totals**: 57 issues individually verified + 5 Step 4 fragile-area checks.
**49 PASS, 12 PARTIAL (all doc-only or documented no-op, no guard-test gap on
executable logic), 1 correctly-reverted-do-not-reapply (#1651/#1823), 0 FAIL,
0 UNVERIFIABLE. Zero "Regression of #NNN" findings.**

---

## Batch 1 — #2405, #2285, #2283, #2282, #2281, #2301, #2300, #2299, #2298, #2327, #2326, #2324, #2322, #2317, #2314, #2309, #2308

### #2405: AUD-2026-08-07-D5-01 — Reverb-send gate duplicates -60.0 literal instead of reusing SILENCE_DB
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `c0f3cda3`
- **Fix site**: `crates/audio/src/lib.rs` (`reverb_send_gate_open`, shared by `drain_pending_oneshots` / `dispatch_new_oneshots`)
- **Fix present**: Yes — gate keys off the named `SILENCE_DB` constant, not a re-typed literal.
- **Guard test**: `reverb_send_gate_matches_silence_db_boundary` in `crates/audio/src/tests.rs` — passes (re-run by auditor).
- **Notes**: Auditor-verified directly.

### #2285: NIFAL-D6-07 — finish_trimesh's index-bounds guard validates the merged total, not each source sub-buffer's range
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `03be068d`
- **Fix site**: `crates/nif/src/import/collision/shape.rs` (per-source local-vertex-count validation before merge)
- **Fix present**: Yes.
- **Guard test**: 2 guard tests present; 1 re-run by batch verifier passes.

### #2283: NIF-D4-01 — BsTriShapeKind::LOD triangle-count cutoffs still unreachable — regression of closed #1207
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `03be068d`
- **Fix site**: `crates/nif/src/blocks/tri_shape/` (`BsTriShapeKind::LOD` made data-carrying, dispatch reaches it)
- **Fix present**: Yes.
- **Guard test**: present.
- **Notes**: Chain fix for #1207; verify #1207 itself isn't independently regressed — same commit covers both.

### #2282: NIF-D6-01 — parse_particle_system's modifier_refs bypasses allocate_vec, duplicating bound-check logic
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `03be068d`
- **Fix site**: `crates/nif/src/blocks/particle.rs` (`parse_particle_system` now calls `allocate_vec`)
- **Fix present**: Yes.
- **Guard test**: present.

### #2281: NIF-D2-02 — Bare bsver literals in NifVariant::detect and sequence.rs bypass named-constant doctrine
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `03be068d`
- **Fix site**: `crates/nif/src/version.rs` (`NifVariant::detect`), `crates/nif/src/anim/sequence.rs:177`
- **Fix present**: Yes — `bsver::{FO3_FNV,SKYRIM_LE,SKYRIM_SE,FALLOUT4,FO76,PRE_BETHESDA}` named constants used throughout.
- **Guard test**: 12+ existing `detect_*`/`bsver_values` tests confirm boundaries unchanged.

### #2301: NIFAL-D6-06 — docs still cite import/collision.rs — function moved to import/collision/shape.rs post-#1876 split
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `342ef84e`
- **Fix site**: `docs/engine/nifal.md`
- **Fix present**: Yes — path references corrected.
- **Guard test**: none (doc-only fix; expected). No re-drift found.

### #2300: NIFAL-D5-01 — particle emitter texture_path/src_blend/dst_blend override folding copy-pasted outside apply_emitter_overlays at both load sites
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `342ef84e`
- **Fix site**: `byroredux/src/systems/particle.rs` (`apply_emitter_overlays`, both call sites now share it)
- **Fix present**: Yes.
- **Guard test**: passes (re-run by batch verifier).

### #2299: NIFAL-D4-03 — nifal.md passthrough table still calls BSFurnitureMarker unwalked — stale since M41.5
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `342ef84e`
- **Fix site**: `docs/engine/nifal.md`
- **Fix present**: Yes — BSFurnitureMarker/BSInvMarker rows corrected. No re-drift found.

### #2298: NIFAL-D2-01 — de-strip dedup incomplete — resolve_compressed_mesh and NiSkinPartition still hand-copy the strip-to-triangle conversion
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `342ef84e`
- **Fix site**: `crates/nif/src/blocks/strip.rs` (`pub fn destrip<T>`), consumed by `NiTriStripsData::to_triangles`, `blocks/skin.rs` (`NiSkinPartition`), `import/collision/shape.rs` (`resolve_compressed_mesh`)
- **Fix present**: Yes — auditor confirmed `destrip()` exists and all three call sites reference it.
- **Guard test**: 3+ tests present.

### #2327: SKY-D7-02 — Authored refraction_strength discarded for every Skyrim material that isn't fire-refraction
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `4d350c4b`
- **Fix site**: `docs/engine/nifal.md` + inline comment at the discard site (documented as a deliberate, scoped discard — not a silent drop)
- **Fix present**: Yes.
- **Guard test**: passes (re-run by batch verifier).
- **Notes**: Resolution is "document as intentional," matching the issue's own acceptance criteria (not a behavior change).

### #2326: SK-D5-BSA-NEW-01 — Stale 256 MB cap comments — actual enforced limit is 1 GB
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `4d350c4b`
- **Fix site**: BSA/BA2 reader comments (4 sites) corrected to `MAX_CHUNK_BYTES` / 1 GB.
- **Fix present**: Yes. No re-drift found.

### #2324: LOW-1 — audit-skyrim skill's Dimension-3 checklist still misstates the upperbody.nif pre-scan as applying to Skyrim+
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `4d350c4b`
- **Fix site**: `.claude/commands/audit-skyrim/SKILL.md`
- **Fix present**: Yes. No re-drift found.

### #2322: SK-D1-02 — #621's VF_FULL_PRECISION back-write rests on a false premise and is a no-op on all vanilla content
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `4d350c4b`
- **Fix site**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:553-564` (comment corrected; `|=` kept as defensive metadata-correctness, documented as a no-op on real content per SK-D1-01's 21140/21140 measurement)
- **Fix present**: Yes — auditor confirmed the corrected comment and the retained defensive bit-set are both present.
- **Guard test**: present (existing VF_FULL_PRECISION tests).

### #2317: FO3-D1-02 — FO3 parallax POM enabled by texture-slot presence, never by authored flags; height scale un-converted
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `2aded28e`
- **Fix site**: `crates/nif/src/import/material/legacy_properties.rs` (`fo3_parallax_authored`, `fo3_parallax_scale_to_height_scale`)
- **Fix present**: Yes — auditor confirmed both functions exist and are called from the shader-type-data extraction path.
- **Guard test**: `fo3_parallax_authored_requires_one_of_the_two_flag_bits` (and 3 more) in `legacy_properties.rs` — re-run by auditor, passes.

### #2314: TD3-206 — shader-pipeline.md's volumetrics descriptor-set description is a stale 2026-05-era snapshot
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `2aded28e`
- **Fix site**: `docs/engine/shader-pipeline.md` (volumetrics binding table now lists all 12 bindings)
- **Fix present**: Yes. No re-drift found.

### #2309: TD3-205 — feature-matrix.md's Fire-refraction row (yesterday's own fix) cites #2236/#2237 as open — both closed same day
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `2aded28e`
- **Fix site**: `docs/feature-matrix.md`
- **Fix present**: Yes. No re-drift found.

### #2308: TD3-204 — docs/engine/renderer.md quotes stale GpuInstance (112 B) / GpuMaterial (300 B) sizes
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `2aded28e`
- **Fix site**: `docs/engine/renderer.md` (now 128 B / 348 B, matching Step 4's live struct-size pins below)
- **Fix present**: Yes. No re-drift found — cross-checked live against the passing `gpu_instance_is_128_bytes_std430_compatible` / `gpu_material_size_is_348_bytes` tests (see Step 4).

---

## Batch 2 — #2307, #2306, #2305, #2304, #2380, #2381, #2378, #2382, #2379, #2295, #2294, #2293, #2292, #2288, #2287, #2284, #2329

### #2307: NIFAL-D9-03 — translation_completeness.rs fill-rate floors have ~33pp slack; metO/rghO/normal_map columns have no assertion at all
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `66f0775e`
- **Fix site**: `crates/nif/src/import/tests/translation_completeness.rs`
- **Fix present**: Yes — ≥99.9% floors for metO/rghO across all 7 games, normal_map floors for 5 games; Oblivion/Starfield left unasserted with a documented structural-0% rationale.
- **Guard test**: present, passes.

### #2306: NIFAL-D8-02 — nifal.md still cites deleted ShaderFlags<'a> typed view removed by #1897
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `66f0775e`
- **Fix site**: `docs/engine/nifal.md`
- **Fix present**: Yes — stale reference removed, no re-drift.

### #2305: NIFAL-D7-NEW-01 — hkx crate's convert_hkx_clip is a second AnimationClip production boundary, undeclared in nifal.md
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-07
- **Fix commit**: `66f0775e`
- **Fix site**: `docs/engine/nifal.md`
- **Fix present**: Yes — `convert_hkx_clip` now documented as a second canonical `AnimationClip` boundary.

### #2304: NIFAL-D7-03 — operation->FloatTarget and target_color->ColorTarget discriminator tables duplicated between KF and embedded animation arms
- **Status**: PASS
- **Closed**: 2026-08-07
- **Fix commit**: `66f0775e`
- **Fix site**: `crates/nif/src/anim/channel.rs` (`float_target_from_operation`, `color_target_from_target_color` extracted, shared by both arms)
- **Fix present**: Yes.
- **Guard test**: 2 tests present, pass.

### #2380: SAVE-D1-15 — Cinematic fragment-effect state (ActorCinematicState/CinematicPresentationState/HorseTetherState) absent from build_save_registry
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `7beb7add`
- **Fix site**: `byroredux/src/save_io.rs` (`build_save_registry`)
- **Fix present**: Yes — cinematic trio registered.
- **Guard test**: passes.

### #2381: SAVE-D1-16 — FragmentExecutionQueue (suspended Papyrus Utility.Wait / WaitForActors3DLoaded continuations) absent from build_save_registry
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `31613843`
- **Fix site**: `byroredux/src/save_io.rs`
- **Fix present**: Yes — registered.
- **Guard test**: passes.

### #2378: SAVE-D1-13 — Material live-edited via mat.set console command absent from build_save_registry
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `cdd1d79d`
- **Fix site**: `byroredux/src/save_io.rs`
- **Fix present**: Yes — registered.
- **Guard test**: passes.

### #2382: SAVE-D1-17 — RumbleOnActivate live gameplay state machine absent from build_save_registry
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `ee45f848`
- **Fix site**: `byroredux/src/save_io.rs`
- **Fix present**: Yes — registered.
- **Guard test**: passes.

### #2379: SAVE-D1-14 — RigidBodyData.motion_type mutated by scripted SetMotionType absent from build_save_registry
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `19971e77`
- **Fix site**: `byroredux/src/save_io.rs`
- **Fix present**: Yes — registered.
- **Guard test**: passes.

### #2295: SAVE-D1-12 — Registry-completeness guard only covers NPC-spawn-stamped components — no coverage for script/system-inserted state
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `30905d4d`
- **Fix site**: `byroredux/src/save_io.rs` (`every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` — source-scan guard over `crates/core/src/ecs/components/`, `crates/scripting/src/`, `crates/physics/src/`; requires every `impl Component`/`impl Resource` to be registered XOR allowlisted in `NOT_SAVED_BY_DESIGN`)
- **Fix present**: Yes — auditor confirmed function exists at `byroredux/src/save_io.rs:1802`.
- **Guard test**: `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` — **re-run directly by auditor** (`cargo test --bin byroredux ... -- --include-ignored`): `test save_io::tests::every_component_or_resource_impl_is_saved_or_explicitly_allowlisted ... ok`.
- **Notes**: This is the guard that backstops #2292/#2293/#2294/#2378-#2382 — its own completeness (132 Component/Resource impls classified, zero unaccounted) was spot-verified by the auditor via the commit diff.

### #2294: SAVE-D1-11 — Scene/Dialogue/Package mid-playback progress omitted from registry without #1696-style documented rationale
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `c5202627`
- **Fix site**: `byroredux/src/save_io.rs`
- **Fix present**: Yes — documented + allowlisted.
- **Guard test**: covered by #2295's completeness guard.

### #2293: SAVE-D1-10 — Dead actor-lifecycle marker unregistered — forward-latent, not yet exploitable
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `c5202627`
- **Fix site**: `byroredux/src/save_io.rs`
- **Fix present**: Yes — documented + allowlisted.
- **Guard test**: covered by #2295's completeness guard.

### #2292: SAVE-D1-09 — Player-control-lock state (PlayerControlState/ActorControlState) absent from build_save_registry
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `c5202627`
- **Fix site**: `byroredux/src/save_io.rs` (`build_save_registry`)
- **Fix present**: Yes — registered.
- **Guard test**: passes.

### #2288: SCR-D6-NEW5-02 — FragmentExecutionQueue's WaitForActors3DLoaded continuation has no retry cap or eviction path
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `464ed88a`
- **Fix site**: `crates/scripting/src/` (`MAX_ACTORS_3D_LOADED_WAIT_SECONDS` retry cap)
- **Fix present**: Yes.
- **Guard test**: passes.

### #2287: SCR-D6-NEW5-01 — ScenePackagePlayback's MoveTo action never completes once its actor entity is despawned
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `84a6bea8`
- **Fix site**: `crates/scripting/src/` (`MOVE_STALL_TIMEOUT_SECONDS`)
- **Fix present**: Yes.
- **Guard test**: passes.

### #2284: MAT-D1-NEW-04 — six authored Skyrim+/FO4 BSLightingShaderProperty shading scalars captured at import, silently dropped at the canonical Material boundary
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `95e77897`
- **Fix site**: `crates/core/src/ecs/components/material.rs` (6 BSLSP shading scalars added to `Material`)
- **Fix present**: Yes — matches the issue's own minimal-fix scope (captured on the struct; not yet shader-consumed, which is out of scope for this fix).
- **Guard test**: passes.

### #2329: FO3-D2-03 — BSSegmentedTriShape segment table consumed and discarded with no bounds check and no downstream consumer
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `44496bb9`
- **Fix site**: `crates/nif/src/blocks/tri_shape/` (`num_segments` now bounds-checked via `check_alloc`)
- **Fix present**: Yes.
- **Guard test**: passes.

---

## Batch 3 — #2328, #2323, #2321, #2313, #2311, #2312, #2310, #2279, #2344, #2338, #2337, #2350, #2349, #2316, #2291, #2340

### #2328: FO3-D1-06 — Inherited-property precedence inversion — texture_clamp_mode/env_map_scale writes are unconditional
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `ba08781e`
- **Fix site**: `crates/nif/src/import/material/legacy_properties.rs`
- **Fix present**: Yes — writes now gated across all 7 shader branches.
- **Guard test**: 2 tests present, pass.

### #2323: FO3-D2-02 — nif_stats per-block histogram keys by parsed Rust type, not header-advertised type — doc claims the opposite
- **Status**: PARTIAL (doc-only)
- **Closed**: 2026-08-05
- **Fix commit**: `588ba573`
- **Fix site**: `byroredux/src/commands/assets.rs` or `nif_stats` doc comment (implementation was already correct; doc corrected)
- **Fix present**: Yes.

### #2321: FO3-D1-05/D2-01 — FO3/FNV fire-refraction heat-haze never classified — refraction fields decoded then dropped at NIFAL boundary
- **Status**: PASS
- **Closed**: 2026-08-05
- **Fix commit**: `45099d55`
- **Fix site**: `byroredux/src/material_translate.rs` / classifier (`fo3nv_f1::REFRACTION` / `FIRE_REFRACTION`)
- **Fix present**: Yes.
- **Guard test**: 3 tests present, pass.

### #2313: TD2-115 — Bitangent-sign clamp idiom duplicated across 4 sites, 2 files
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `a6fe580d`
- **Fix site**: `crates/nif/src/import/mesh/tangent.rs` (`clamp_sign()` extracted, used at all 4 former duplicate sites)
- **Fix present**: Yes.
- **Guard test**: 4 tests present, pass.

### #2311: TD1-083 — crates/nif/src/import/tests.rs newly crossed 2000 LOC
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `06ce3acf`
- **Fix site**: `crates/nif/src/import/tests/` (split into 8 files, largest now 732 LOC)
- **Fix present**: Yes.
- **Guard test**: 60 tests, all pass (file-size regression itself has no test — this is a LOC-hygiene fix, not behavior).

### #2312: TD1-084 — setup_cornell_scene grew to 296 LOC
- **Status**: PARTIAL (watch-and-wait tracking issue)
- **Closed**: 2026-08-04
- **Fix commit**: n/a — tracking-only closure
- **Fix site**: `byroredux/src/cornell.rs` (`setup_cornell_scene`)
- **Fix present**: n/a — function still 296 LOC, confirmed **has not regrown further** since closure.
- **Guard test**: none (LOC-threshold tracking issues have no guard by design).

### #2310: TD1-082 — collect_lights crossed 200 LOC
- **Status**: PARTIAL (watch-and-wait tracking issue)
- **Closed**: 2026-08-04
- **Fix commit**: n/a — tracking-only closure
- **Fix site**: `byroredux/src/render/lights.rs` (`collect_lights`)
- **Fix present**: n/a — confirmed still 208 LOC, has not regrown further.
- **Guard test**: none (by design).

### #2279: PERF-D-DOC-01 — ROADMAP.md bench-of-record predates ~90 commits of substantial rendering/streaming work
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `25663536`
- **Fix site**: `ROADMAP.md`
- **Fix present**: Yes — refreshed to HEAD `28155b79` at fix time.

### #2344: NIF-OBL-D1-01 — NiBlendInterpolator drops Single Interpolator + Single Time at v10.1.0.108-109
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `72278e0d`
- **Fix site**: `crates/nif/src/blocks/interpolator.rs` (gate widened to `V10_1_0_108..V10_1_0_111`)
- **Fix present**: Yes.
- **Guard test**: 2 tests present, pass.

### #2338: FNV-D7-03 — Ragdoll colliders have no interaction-group exclusions — self-collision at rest, HavokFilter parsed but dropped
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `39ddbedb`
- **Fix site**: `byroredux/src/ragdoll.rs` / `crates/physics/src/` (self-contacts disabled via Rapier multibody API)
- **Fix present**: Yes.
- **Guard test**: passes.
- **Notes**: Mechanism differs from the issue's suggested HavokFilter-exclusion approach (uses Rapier's own multibody self-collision disable instead) but verifiably achieves the same end state — no self-collision at rest.

### #2337: FNV-D7-02 — Rapier multibody forward-kinematics overwrites seeded ragdoll poses on the first physics step
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `39ddbedb`
- **Fix site**: `byroredux/src/ragdoll.rs` (seeded pose preserved via joint-displacement derivation on first physics step)
- **Fix present**: Yes.
- **Guard test**: passes.

### #2350: REG-2026-08-03-02 — cinematic.rs keyed-lerp refactor (#2260) introduces a redundant_closure clippy error
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `ee623747`
- **Fix site**: `crates/scripting/src/cinematic.rs`
- **Fix present**: Yes — `redundant_closure` fixed.
- **Guard test**: `cargo clippy` clean at this site (auditor cross-checked this is the exact finding reported in `docs/audits/AUDIT_REGRESSION_2026-08-03.md` REG-2026-08-03-02, now closed by the same commit as #2340/#2349).

### #2349: REG-2026-08-03-01 — post_passes.rs split (#2258) reintroduces undocumented unsafe blocks — regression of #2131 / #1904
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `ee623747`
- **Fix site**: `crates/renderer/src/vulkan/context/post_passes.rs` (all 9-10 `unsafe {}` blocks flagged in the 2026-08-03 report now carry inline `// SAFETY:` comments, not just an enclosing-function `# Safety` doc comment)
- **Fix present**: Yes.
- **Guard test**: `cargo clippy -p byroredux-renderer -- -D clippy::undocumented_unsafe_blocks` clean.
- **Notes**: **This is the third occurrence of this exact discipline regressing** (#1904 → #2131 → #2349, each from a different file-split refactor). Flagging as a recurring pattern worth a dedicated CI gate (`cargo clippy -p byroredux-renderer -- -D clippy::undocumented_unsafe_blocks` as its own always-run job, not folded into a broader `-D warnings` sweep that's easy to skip locally) rather than relying on audit discovery a fourth time. Not itself a new regression — flagging as a process-hardening recommendation.

### #2316: FO3-D5-01 — bhkRigidBody (non-T) CInfo transform applied unconditionally — 9.5% of FO3 meshes get displaced colliders
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `cb8bfd83`
- **Fix site**: `crates/nif/src/blocks/collision/rigid_body.rs` (gated on `is_t` field)
- **Fix present**: Yes.
- **Guard test**: 2 tests present, pass.

### #2291: SAVE-D1-08 — TwoStateActivator + ScriptVariables — live script-driven per-object state — absent from build_save_registry
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `32ebfdec`
- **Fix site**: `byroredux/src/save_io.rs`
- **Fix present**: Yes — registered.
- **Guard test**: passes.

### #2340: FNV-D8-01 — --grid 0,0 worldspace auto-pick is non-deterministic across multiple containing worldspaces
- **Status**: PASS
- **Closed**: 2026-08-04
- **Fix commit**: `ee623747`
- **Fix site**: `byroredux/src/cell_loader/exterior.rs` (`select_worldspace_key`, `PREFERRED_WORLDSPACES`)
- **Fix present**: Yes — auditor confirmed `select_worldspace_key` exists with dedicated test coverage.
- **Guard test**: present in `cell_loader/exterior.rs` test module, passes.

---

## Fresh Verification Candidates + #1651/#1823 Special Case

### #1815: SCR-D2-01 — decompiler recursion-depth cap in the boolean-collapse pass
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `7fdb694b`
- **Fix site**: `crates/pex/src/decompile/boolean.rs` (`MAX_REBUILD_DEPTH = 1024`)
- **Fix present**: Yes.
- **Guard test**: passes.

### #1816: SCR-D5-NEW-02 — `translate_pex` missing `catch_unwind`
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `8b04c492`
- **Fix site**: `crates/pex/src/` (`translate_pex` wraps `decompile_script` in `catch_unwind`)
- **Fix present**: Yes.
- **Guard test**: passes.

### #1728: SCR-D1-02 — Skyrim-BE/Starfield round-trip test for the `.pex` reader
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `ae219630`
- **Fix site**: `crates/pex/src/` (Skyrim-BE + Starfield round-trip tests)
- **Fix present**: Yes.
- **Guard test**: both round-trip tests pass.

### #1740: SCR-D5-03 — DA10 `.pex` byte-equality parity test
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `2f0b99fa`
- **Fix site**: `crates/pex/src/` (DA10 byte-parity test, verified against real Skyrim SE game data)
- **Fix present**: Yes.
- **Guard test**: passes.

### #1731: LC-D7-02 — VWD record-header flag parse + expose
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `175ebf2c`
- **Fix site**: legacy ESM record-header path (VWD flag `0x00010000` parsed + exposed)
- **Fix present**: Yes.
- **Guard test**: 2+ tests present, pass.

### #1718: FNV-D7-01 — ragdoll bone/constraint-drop telemetry on bone-name miss
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `ffe9a816`
- **Fix site**: `byroredux/src/ragdoll.rs` (warn-logs on bone-name-miss drop)
- **Fix present**: Yes.
- **Guard test**: passes.

### #1651/#1823: BGSM/BGEM GL→Gamebryo blend factors — correctly reverted, do not re-apply
- **Status**: Correctly reverted — do not re-apply
- **Closed**: #1651 closed 2026-06-19 (the original, wrong fix) · #1823 closed 2026-07-02 (the revert)
- **Fix commit**: `ada75ee3` (#1651, wrong fix) → `27334481` (#1823, revert)
- **Fix site**: `crates/bgsm/src/` blend-factor translation (BGSM/BGEM)
- **Fix present**: #1651's GL↔Gamebryo blend-factor swap is **absent** from the current tree, as intended — `#1823` replaced it with a plain identity narrowing cast (`raw as u8`), with an inline comment warning against reintroducing the swap.
- **Guard test**: auditor confirmed both BGSM and BGEM call sites use the reverted (identity-cast) function; 2 guard tests pass with real reference tuples.
- **Notes**: **Do not report #1651 as still holding.** Its premise (that BGSM/BGEM blend factors need GL→Gamebryo enum translation) was disproven — the translation was corrupting FO4 Additive/Multiplicative materials — and #1823 is the fix of record. Confirmed no regression of #1823 (the swap has not crept back in).

---

## Step 4 — Unconditional Fragile-Area Checks (run directly by auditor, not delegated)

These guard fixes/contracts whose breakage is invisible to GitHub-issue
discovery (most landed as refactors, not filed bugs) — checked every run
regardless of Step 1's discovery window.

### NIFAL canonical-translation tier
- **Single material boundary**: PASS. `byroredux/src/material_translate.rs::translate_material` remains the sole `ImportedMesh → Material` site (only other reference is a call site in `byroredux/src/cell_loader/spawn.rs`). `Material::metalness`/`roughness` (`crates/core/src/ecs/components/material.rs:292,298`) remain plain `f32` — confirmed zero `Option<f32>` reintroductions on those fields (grep for `pub metalness: Option<f32>` / `pub roughness: Option<f32>` returns nothing). `resolve_pbr` (line 813) and `classify_pbr_keyword` (line 554) are present and unchanged in role.
- **Typed particle emitters**: PASS. `NiPSysEmitter`/`NiPSysEmitterCtlr`/`NiPSysEmitterCtlrData`/`NiPSysGrowFadeModifier` all still parse as typed structs in `crates/nif/src/blocks/particle.rs`, dispatched by name in `crates/nif/src/blocks/mod.rs`. `extract_emitter_params`/`extract_emitter_rate` present in `crates/nif/src/import/walk/mod.rs`; `ImportedEmitterParams` present in `crates/nif/src/import/types.rs`; `apply_emitter_params` present in `byroredux/src/systems/particle.rs`.
- **Collision shape coverage**: PASS. `BhkMultiSphereShape` and `BhkConvexListShape` both resolve to `CollisionShape` in `crates/nif/src/import/collision/shape.rs` (lines 110, 235) — not dropped to `None`.

### Disney BSDF + GPU struct contracts
- **Reservoir-array retirement stays retired**: PASS. `grep -n "resRadiance\s*\["` across `crates/renderer/shaders/*.frag`, `*.comp`, and `include/*.glsl` returns **zero live occurrences** (only comments referencing the historical `#1369` retirement). `shadowableLightRadiance` is present in `crates/renderer/shaders/include/lighting.glsl:71`, confirming the register-local WRS recomputation path is still in place. `crates/renderer/src/vulkan/gbuffer.rs` has no reservoir-named attachment (`reservoir` grep returns nothing).
- **Disney/Burley lobe location + attribution**: PASS. Lives in `crates/renderer/shaders/include/pbr.glsl` (GLSL-PathTracer MIT attribution present at line 23; `triangle.frag:23-25` carries the required MIT notice).
- **GPU struct size pins**: PASS. `cargo test -p byroredux-renderer gpu_` → **44 passed, 0 failed**, including `gpu_instance_is_128_bytes_std430_compatible`, `gpu_camera_is_336_bytes`, `gpu_material_glsl_field_names_pinned`, `gpu_material_glsl_field_order_matches_rust_struct`, `gpu_light_glsl_copies_stay_in_lockstep`.

---

## Summary Table

| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
| #2405 | AUD-2026-08-07-D5-01 reverb-send gate | PASS | Yes | passes (re-run) |
| #2285 | NIFAL-D6-07 finish_trimesh bounds | PASS | Yes | passes |
| #2283 | NIF-D4-01 LOD cutoffs | PASS | Yes | present |
| #2282 | NIF-D6-01 allocate_vec dedup | PASS | Yes | present |
| #2281 | NIF-D2-02 named bsver constants | PASS | Yes | passes |
| #2301 | NIFAL-D6-06 stale collision.rs doc | PARTIAL (doc) | Yes | none |
| #2300 | NIFAL-D5-01 emitter override folding | PASS | Yes | passes (re-run) |
| #2299 | NIFAL-D4-03 BSFurnitureMarker doc | PARTIAL (doc) | Yes | none |
| #2298 | NIFAL-D2-01 de-strip dedup | PASS | Yes | passes |
| #2327 | SKY-D7-02 refraction_strength discard | PASS | Yes | passes (re-run) |
| #2326 | SK-D5-BSA-NEW-01 256MB doc | PARTIAL (doc) | Yes | none |
| #2324 | LOW-1 upperbody.nif checklist doc | PARTIAL (doc) | Yes | none |
| #2322 | SK-D1-02 VF_FULL_PRECISION no-op | PASS | Yes | present |
| #2317 | FO3-D1-02 parallax POM gating | PASS | Yes | passes (re-run) |
| #2314 | TD3-206 volumetrics doc | PARTIAL (doc) | Yes | none |
| #2309 | TD3-205 fire-refraction doc | PARTIAL (doc) | Yes | none |
| #2308 | TD3-204 GpuInstance/GpuMaterial doc | PARTIAL (doc) | Yes | none (pinned by Step 4 tests) |
| #2307 | NIFAL-D9-03 fill-rate floors | PASS | Yes | passes |
| #2306 | NIFAL-D8-02 stale ShaderFlags doc | PARTIAL (doc) | Yes | none |
| #2305 | NIFAL-D7-NEW-01 hkx boundary doc | PARTIAL (doc) | Yes | none |
| #2304 | NIFAL-D7-03 discriminator dedup | PASS | Yes | passes |
| #2380 | SAVE-D1-15 cinematic trio registry | PASS | Yes | passes |
| #2381 | SAVE-D1-16 FragmentExecutionQueue registry | PASS | Yes | passes |
| #2378 | SAVE-D1-13 Material registry | PASS | Yes | passes |
| #2382 | SAVE-D1-17 RumbleOnActivate registry | PASS | Yes | passes |
| #2379 | SAVE-D1-14 RigidBodyData registry | PASS | Yes | passes |
| #2295 | SAVE-D1-12 completeness guard | PASS | Yes | passes (re-run) |
| #2294 | SAVE-D1-11 Scene/Dialogue/Package | PASS | Yes | passes |
| #2293 | SAVE-D1-10 dead-marker registration | PASS | Yes | passes |
| #2292 | SAVE-D1-09 player-control-lock registry | PASS | Yes | passes |
| #2288 | SCR-D6-NEW5-02 retry cap | PASS | Yes | passes |
| #2287 | SCR-D6-NEW5-01 MoveTo stall timeout | PASS | Yes | passes |
| #2284 | MAT-D1-NEW-04 BSLSP shading scalars | PASS | Yes | passes |
| #2329 | FO3-D2-03 segment-table bounds check | PASS | Yes | passes |
| #2328 | FO3-D1-06 property precedence gating | PASS | Yes | passes |
| #2323 | FO3-D2-02 nif_stats doc | PARTIAL (doc) | Yes | none |
| #2321 | FO3-D1-05/D2-01 fire-refraction classify | PASS | Yes | passes |
| #2313 | TD2-115 clamp_sign dedup | PASS | Yes | passes |
| #2311 | TD1-083 import/tests.rs split | PASS | Yes | passes |
| #2312 | TD1-084 setup_cornell_scene LOC watch | PARTIAL (watch) | n/a | none |
| #2310 | TD1-082 collect_lights LOC watch | PARTIAL (watch) | n/a | none |
| #2279 | PERF-D-DOC-01 bench-of-record refresh | PASS | Yes | n/a |
| #2344 | NIF-OBL-D1-01 NiBlendInterpolator gate | PASS | Yes | passes |
| #2338 | FNV-D7-03 ragdoll self-collision | PASS | Yes | passes |
| #2337 | FNV-D7-02 ragdoll seeded-pose preserve | PASS | Yes | passes |
| #2350 | REG-2026-08-03-02 redundant_closure | PASS | Yes | clippy clean |
| #2349 | REG-2026-08-03-01 undocumented unsafe (3rd occurrence) | PASS | Yes | clippy clean |
| #2316 | FO3-D5-01 bhkRigidBody is_t gate | PASS | Yes | passes |
| #2291 | SAVE-D1-08 TwoStateActivator registry | PASS | Yes | passes |
| #2340 | FNV-D8-01 deterministic worldspace pick | PASS | Yes | passes |
| #1815 | SCR-D2-01 decompiler recursion cap | PASS | Yes | passes |
| #1816 | SCR-D5-NEW-02 translate_pex catch_unwind | PASS | Yes | passes |
| #1728 | SCR-D1-02 Skyrim-BE/Starfield round-trip | PASS | Yes | passes |
| #1740 | SCR-D5-03 DA10 byte-parity | PASS | Yes | passes |
| #1731 | LC-D7-02 VWD flag parse | PASS | Yes | passes |
| #1718 | FNV-D7-01 ragdoll bone-miss telemetry | PASS | Yes | passes |
| #1651/#1823 | BGSM/BGEM blend factors | Correctly reverted | N/A by design | passes |

**Step 4 fragile-area checks**: NIFAL single material boundary (PASS), typed
particle emitters (PASS), collision shape coverage (PASS), reservoir-array
retirement (PASS), GPU struct size pins — `cargo test -p byroredux-renderer
gpu_` 44/44 (PASS).

---

## Severity Summary

**Zero findings at any severity.** No `Regression of #NNN` was surfaced by
any of the 57 individually-verified issues or the 5 Step 4 fragile-area
checks.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |

One process-hardening note is worth escalating outside the severity scale:
**#2349's fix is the third time** the `clippy::undocumented_unsafe_blocks`
discipline has regressed from the same class of refactor (#1904 → #2131 →
#2349, three different file-splits). It is fixed again as of `ee623747`, but
given the repeat pattern, consider a dedicated always-run CI job (`cargo
clippy -p byroredux-renderer -- -D clippy::undocumented_unsafe_blocks`)
rather than relying on a fourth audit cycle to catch the next split.

Suggested next step: `/audit-publish docs/audits/AUDIT_REGRESSION_2026-08-07.md`
(no publishable findings this cycle — nothing to file as new issues).
