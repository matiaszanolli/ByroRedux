# ByroRedux Tech-Debt Audit — 2026-05-14

**Run**: `/audit-tech-debt --depth deep` (10 dimensions, all run)
**Prior audit**: [`AUDIT_TECH_DEBT_2026-05-13.md`](AUDIT_TECH_DEBT_2026-05-13.md) — ~132 findings
**Repo HEAD**: `main` @ `5ab6a8b` (43 commits past the prior audit, including Session 36's 9-commit monolith-split sweep)

## Executive Summary

| Severity | Count | Comment |
|----------|------:|---------|
| **HIGH** | 0    | Prior audit's lone HIGH (`MAX_FRAMES_IN_FLIGHT` duplicate, TD4-002) closed via `#1037` / `de274c9`. No new HIGH this run. |
| **MEDIUM** | 36 | 12 carry-overs (the parse-but-don't-consume family is sticky); 24 net-new — dominated by **Session 36 audit-skill rot** (15 findings, doc-update commit didn't touch `.claude/commands/audit-*.md` files) plus **shader↔Rust drift class** (6 net-new under `#1038`). |
| **LOW**   | ~110 | Across all 10 dimensions; bulk concentrated in dead-code mute reviews (Dim 2) and bare-NifVersion literals (Dim 4). |
| **INFO**  | 8    | Status-only — closed milestones whose docs still claim "in progress", trait-impl-forced `_var` params (Dim 8), `M55 volumetrics` evolved past "clear-only skeleton". |
| **Total** | ~154 | Down ~12% from baseline despite Session 36 path-rot opening 15+ new doc findings. |

### Dominant themes (2026-05-14 cycle)

1. **Session 36 split + audit-skill rot.** Today's monolith split (`acceleration.rs` → `acceleration/`, `scene_buffer.rs` → `scene_buffer/`, `anim.rs` → `anim/`, `import/mesh.rs` → `import/mesh/`, `blocks/collision.rs` → `blocks/collision/`, plus two test-file splits) invalidated every `file.rs` path reference in `.claude/commands/audit-*.md` / `_audit-common.md`. The doc-refresh commit `5ab6a8b` updated CLAUDE.md / HISTORY / ROADMAP / engine docs but skipped the audit skills — every next audit run grepping for those paths will hard-fail. **15 MEDIUM findings under Dim 7 + 3 under Dim 10**, all trivially fixable in one batched PR.

2. **Shader ↔ Rust constant drift continues (#1038 hub).** Of the 5 prior MEDIUMs (TD4-003..006), 3 now have drift-detection tests (`cluster_cull`, `skin_compute`, `caustic`). 6 new MEDIUMs surfaced (`BLOOM_INTENSITY`, `VOLUME_FAR`, DBG_* viz flags, water motion enum, TAA/SSAO/SVGF/caustic workgroup sizes, cluster `NEAR`/`FAR_FLOOR`/`FAR_FALLBACK`, `GLASS_RAY_BUDGET`, `MAT_FLAG_VERTEX_COLOR_EMISSIVE`). The architectural fix (one-time `build.rs` codegen target #1038) closes the whole family in a single move.

3. **`#1047` parse-but-don't-consume hub closed administratively; family is still live.** 7 findings (SpeedTree placeholder, StencilState, BSSky/Water, IMGS/ACTI/TERM, plus 4 net-new: OblivionHdrLighting, TREE.SNAM/CNAM, FO4 face_morphs, BPTD body parts) parse but no renderer/runtime consumer wires up. The hub closed because the parser-side gating-milestone comments are present; the consumer-side milestones (M55, M28, M41.x) are tracked separately.

4. **Test hygiene is concentrated.** 90 actual `#[test] #[ignore]` annotations. 16 unique CLOSED issues are referenced as the `#[ignore]`-reason — 2 CRITICAL (#405 SCOL placements, #533 NAM0 weathers) and 4 HIGH (#754, #819, #934, #965). **25 of 90 hardcode the Steam install path** and can't be brought back online even with `BYROREDUX_*_DATA` set — routing them through `data_dir(env_var, default)` (already proven in `parse_real_esm.rs:30`) is a single batched PR.

5. **File complexity sweep crushed 7 of 9 monoliths.** Session 36 closed `acceleration.rs` (4 383 → max 1 055), `dispatch_tests.rs` (3 667 → 891), `cell/tests.rs` (3 329 → 761), `collision.rs` (2 184 → 346), `anim.rs` (2 101 → 537), `import/mesh.rs` (2 212 → 546), `scene_buffer.rs` (2 334 → 578). **Only `vulkan/context/draw.rs` (2 571) and `vulkan/context/mod.rs` (2 363) remain > 2 000 LOC** — both renderer-context-trio, both deferred per the speculative-Vulkan-fixes feedback memory.

## Baseline Snapshot — 2026-05-14

| Metric | 2026-05-13 | 2026-05-14 | Δ |
|--------|-----------:|-----------:|---:|
| TODO / FIXME / HACK / XXX | 4 | **4** | 0 |
| `#[allow(dead_code)]` | 42 | **41** | -1 |
| `unimplemented!()` / `todo!()` | 1 | **1** | 0 (the lone hit is a doc comment) |
| `panic!("not implemented/yet/TODO")` | 0 | **0** | 0 |
| `#[ignore]` tests (real annotations) | ~73 | **90** | +17 (Session 35 audit-bundle finishers added new `#[ignore]`d real-data regressions) |
| `#[allow(...)]` total | — | **82** | new metric |
| `_var`-prefixed unused params (signatures) | 33 | **33** | 0 |
| files > 2 000 LOC | 9 | **2** | **-7** (Session 36) |
| files > 1 500 LOC | — | **15** | new metric |
| Total `.rs` files (incl. tests) | — | **441** | new metric |

Recorded so the next audit can diff. Note the `#[ignore]` count went up — Session 35's audit-bundle close-out added more real-data regression gates (closed-issue backing tests), most of which are `#[ignore]`d because they need on-disk game data.

## Top 10 Quick Wins (trivial or small effort, immediate payoff)

| # | Finding | Effort | Payoff |
|---|---------|--------|--------|
| 1 | **TD7-025..039** — sweep `.claude/commands/audit-*.md` and `_audit-common.md` for `acceleration.rs` / `scene_buffer.rs` / `anim.rs` / `import/mesh.rs` / `blocks/collision.rs` references (15 paths). | trivial (one batched sed + manual verify) | Every audit run stops hard-failing on stale grep targets. |
| 2 | **TD6-101..107** — route 25 `#[ignore]`d real-data tests through `data_dir("BYROREDUX_*_DATA", …)` helper (already proven at `parse_real_esm.rs:30`). | small (~1 h) | Unblocks 2 CRITICAL and 4 HIGH regression gates for `BYROREDUX_*_DATA`-set runs. |
| 3 | **TD4-101** — add drift-detection test pair for `BLOOM_INTENSITY` + `VOLUME_FAR` (mirror the proven `composite_frag_caustic_fixed_scale_matches_rust_const` template). | trivial (~10 LOC × 2 = 20 LOC) | Closes 2 of 6 outstanding shader-drift MEDIUMs without waiting for #1038 codegen. |
| 4 | **TD2-101..109** — drop 8 declared-but-unused `[dependencies]` entries across 8 crates (`thiserror` × 5, `log` × 4, `winit`, `byroredux-platform`, `swf`, `image`, `byroredux-core`, `ruffle_render`). | trivial (~8 toml edits) | Smaller cargo audit / supply-chain surface; clearer "this crate is graphics-only" signal. |
| 5 | **TD1-001 + TD1-002 / TD5-005** — fix the two `// TODO: thread StagingPool (#242)` markers. App owns a `StagingPool`; pass `&mut self.staging_pool` into `setup_scene` + `rebuild_geometry_ssbo`. | small (~30 LOC) | Closes a 31-day-old live half-implementation; eliminates per-frame transient staging allocations on the geometry-rebuild path. |
| 6 | **TD8-009..010** — delete 9 actionable `_var` params on free functions (3× `humanoid_*_gender`, `watr_to_params(_game)`, `TextureRegistry::recreate_on_resize(_new_swapchain_image_count)`, `debug_server::start(_world)`). | trivial (3 sig edits + delete 9 args) | CLAUDE.md global rule compliance ("delete completely, no `_var` breadcrumbs"). |
| 7 | **TD7-039** — `audit-renderer.md:241,252` says GpuInstance lives in 3 shaders; actual count is 5 (`triangle.vert/frag`, `ui.vert`, `water.vert`, `caustic_splat.comp`). Update to 5 and re-verify `feedback_shader_struct_sync.md`. | trivial (2 number bumps + memory note refresh) | Future shader-struct edits won't miss two of the lockstep targets. |
| 8 | **TD6-114..115** — `cargo fix --tests` on `bs_tri_shape_shader_flag_tests.rs:18` + `material_path_capture_tests.rs:12` (unused `MeshResolver` imports from Session 36 split commit `014adc8`). | trivial (one `cargo fix` invocation) | Removes 2 of the 4 outstanding renderer test warnings. |
| 9 | **TD8-011..012** — review 13 blanket `#[cfg(test)] #[allow(unused_imports)]` blocks in `cell_loader.rs` + `scene.rs` from the Session 35 split (`1c0b98d`). | small (~30 min review + 1 commit) | Each was added defensively during the split; most of the suppressed imports are no longer referenced by any test child module. |
| 10 | **TD7-041** — close-out 6 ROADMAP references to `#687 / #688 / #697 / #698` as "open tracking issues" — all 4 are CLOSED on GitHub. Repoint to "tracked under [git log]" or remove. | trivial (6 inline edits) | ROADMAP no longer claims active tracking on closed issues. |

## Top 5 Medium Investments

| # | Finding | Effort | Why it pays |
|---|---------|--------|-------------|
| 1 | **#1038 build.rs codegen for shader constants** (TD4-101..107). One-time scaffold: emit `shaders/constants.glsl` from a single Rust source-of-truth module. | medium (1 day) | Closes the entire shader-Rust drift family — 6 outstanding MEDIUMs disappear plus a permanent class of bugs is prevented. The drift-detection-test approach (proven 3× now) scales linearly; codegen breaks the linear cost curve. |
| 2 | **TD3-101 — ESM dispatcher 96 near-identical arms** in `records/mod.rs`. 37 are macro-equivalent calls to `parse_minimal_esm_record`. Build a 2-macro (`minimal_arm!` + `typed_arm!`) dispatch-table layer. | medium (~1 day) | Tier-3 work adds QUST/DIAL/INFO/PERK/MGEF/SPEL/ENCH/AVIF/PACK/NAVM — each currently costs 2 dispatcher edits + 1 typed-map field. With the macro it's 1 row per record. **This is the R2 risk-reducer from ROADMAP; Tier 3 cannot start without it.** |
| 3 | **TD9-101 / TD9-102 — split `vulkan/context/draw.rs` (2 571 LOC) and `vulkan/context/mod.rs` (2 363 LOC)** along phase comments already in source. | medium (1–2 days, gated on Vulkan smoke check between each substantive step) | Last 2 monoliths > 2 000 LOC; defer-then-block pattern means every future renderer feature touches one of these. Phase comments (`// === Frame sync ===`, `// === Skin compute ===`, etc.) already mark the split boundaries. |
| 4 | **TD3-104 — `impl NiObject for X { … }` at 174 sites** (up from 117 last audit). `impl_ni_object!` macro proposal in `nif/src/blocks/traits.rs`. | medium (~1 day) | `#680` had to retrofit `as_av_object` across every subclass — the next upcast-extension will need the same N-site touch. Macro converts to single-row edits. |
| 5 | **TD3-102 — EDID/FULL/MODL/SCRI/VMAD walk re-rolled across 27 record parsers**; `CommonItemFields::from_subs` exists but only 10 parsers consume it. | medium (~1 day) | Divergence is proven: #816 (SCOL FULL fix), #369 (VMAD), #624 (LocalizedPluginGuard) each landed against one parser. The next divergence bug is queued. |

## Findings by Severity → Dimension

### MEDIUM (36)

#### Dimension 1 — Stale Markers (2)
- **TD1-001 / TD1-002** — `// TODO: thread StagingPool (#242)` at `byroredux/src/scene.rs:477` + `byroredux/src/main.rs:1151`. Issue closed 2026-04-13 same commit (`f97df27`) that planted the marker; consumer-side never landed. Both reachable from shipped CLI flags + per-frame geometry-rebuild path. Promoted from LOW per the closed-driver-issue rule. Dup of prior TD1-001/002 (still unresolved). Cross-ref Dim 5 TD5-005.

#### Dimension 3 — Logic Duplication (5)
- **TD3-101** — ESM dispatcher 96 arms in `records/mod.rs:719-1613`. See Top-5 Medium Investments #2.
- **TD3-102** — EDID/FULL/MODL/SCRI/VMAD walks re-rolled × 27. See Top-5 #5.
- **TD3-103** — `vk::MemoryBarrier::default()` + `cmd_pipeline_barrier` at 13 sites (6 in `context/draw.rs`). Half of prior TD3-008 closed via `#1046` (WriteDescriptorSet builder); MemoryBarrier half still open.
- **TD3-104** — `impl NiObject for X` 174 sites. See Top-5 #4.
- **TD3-105** — 33+ inline `[stream.read_f32_le()?, …]` array reads despite `NifStream::read_ni_point3` existing. Propose generic `read_f32_array_n<const N>` in `crates/nif/src/stream.rs`.

#### Dimension 4 — Magic Numbers / Shader-Rust Drift (6)
- **TD4-101** — `BLOOM_INTENSITY` / `VOLUME_FAR` no drift test. See Quick Wins #3.
- **TD4-102** — DBG_* viz flag bits (`DBG_BYPASS_NORMAL_MAP = 0x10` etc.) shader-only at `triangle.frag:718-759`. Referenced by literal in `feedback_chrome_means_missing_textures` memory note.
- **TD4-103** — Water motion enum (`WATER_CALM/RIVER/RAPIDS/WATERFALL`) shader-only at `water.frag:38-41`.
- **TD4-104** — TAA / SSAO / SVGF / caustic compute shaders use `local_size = 8` with no Rust mirror + drift test (bloom + volumetrics already covered).
- **TD4-105** — Cluster `NEAR` / `FAR_FLOOR` / `FAR_FALLBACK` / `THREADS_PER_CLUSTER` shader-only (the `TILES_X/Y/SLICES_Z` half is tested).
- **TD4-106** — `GLASS_RAY_BUDGET = 8192u` has no Rust mirror; pairs with Rust-owned `RAY_BUDGET_STRIDE`. Promoted from LOW.
- (TD4-107 — `VERTEX_UV_OFFSET_FLOATS = 9` + `MAT_FLAG_VERTEX_COLOR_EMISSIVE = 0x1u` — counted as LOW; future-MEDIUM if drift bites.)

#### Dimension 5 — Stubs / Parse-but-don't-Consume (7)
- **TD5-001** — SpeedTree `--tree` CLI returns placeholder billboard. Reachable from `docs/smoke-tests/m-trees.sh`. Gated by SpeedTree Phase 2 (no ROADMAP row).
- **TD5-002** — `StencilState` parsed (7 sub-fields), pipeline hardcodes `stencil_test_enable(false)`. Gated by `#337` but consumer-side ungated.
- **TD5-003** — `BSSkyShaderProperty` / `BSWaterShaderProperty` flags captured, zero renderer consumers.
- **TD5-010** — `OblivionHdrLighting` (14 f32 HNAM HDR fields) parsed, zero consumers, no gating docstring. **NEW** — promote.
- **TD5-011** — TREE.SNAM (leaf indices) / TREE.CNAM (canopy params) parsed, zero consumers, ungated. **NEW**.
- **TD5-013** — FO4 `NpcRecord.face_morphs` parsed, zero consumers — sibling `runtime_facegen` IS consumed in `npc_spawn.rs:619` (M41.0 Phase 3b). Asymmetric. **NEW**.
- **TD5-016** — BPTD body-parts parsed (FO3/FNV/Skyrim dismemberment routing), zero consumers, ungated. **NEW**.

#### Dimension 6 — Test Hygiene (5)
- **TD6-101** — #405 (CRITICAL closed) SCOL placements regression gate at `cell/tests/integration.rs:341` is `#[ignore]`d behind hardcoded `/mnt/data/SteamLibrary/…` path. See Quick Wins #2.
- **TD6-102** — #533 (CRITICAL closed) NAM0 weather regression — 3 backing tests, all `#[ignore]`d, all hardcoded.
- **TD6-103** — #754 (HIGH closed) Starfield BSWeakReferenceNode dispatch — backing test `#[ignore]`d + hardcoded.
- **TD6-104** — #819 (HIGH closed) FO4 parse-rate gate — `#[ignore]`d (intentional, but the prose claim in audit-skill says it's "always on").
- **TD6-105** — #965 (HIGH closed) WRLD worldspace dispatch — `#[ignore]`d + hardcoded.

#### Dimension 7 — Stale Documentation (15)
All 15 are audit-skill rot from Session 36 paths:
- **TD7-025..033** — `_audit-common.md:30-43` "Project Layout" entries point at deleted `acceleration.rs`, `scene_buffer.rs`, `anim.rs`, `import/mesh.rs`, `blocks/collision.rs`, plus the two test-file splits. **9 trivial sed fixes** in one batched commit.
- **TD7-034..038** — `audit-renderer.md`, `audit-performance.md`, `audit-concurrency.md`, `audit-nif.md`, `audit-safety.md` each cite at least one Session-36-split file by `.rs` name in a "must not regress" anchor line. Update to symbol-based references per `#1040`'s pattern.
- **TD7-039** — `audit-renderer.md:241,252` says GpuInstance lives in 3 shaders; actual is 5. See Quick Wins #7.
- **TD7-041** — ROADMAP claims `#687 / #688 / #697 / #698` are "open tracking issues"; all 4 CLOSED. See Quick Wins #10.
- **TD7-042** — `HISTORY.md:170` still says `MAX_MATERIALS = 1024`; actual is `4096` (`scene_buffer/constants.rs:103`). HISTORY is append-only but this is a typo in the still-relevant Session-32 entry.
- **TD7-044** — `scene_buffer/gpu_types.rs:158-161` has a thinking-aloud "wait — six trailing vec4s" mid-edit artifact in a `///` docstring (math is right, narrative needs cleanup).

#### Dimension 9 — File / Function Complexity (5)
- **TD9-101** — `vulkan/context/draw.rs` at **2 571 LOC** with `draw_frame` as a single **2 329-LOC** function nesting depth 12. Phase comments mark proposed split: `frame_sync` / `instance_map` / `skin_dispatch` / `scene_upload` / `main_pass` / `post_pass` / `present`. **Deferred per `feedback_speculative_vulkan_fixes` — RenderDoc baseline needed.**
- **TD9-102** — `vulkan/context/mod.rs` at **2 363 LOC**, `new()` is 1 247 LOC across 31 numbered phases. Split along the numbered phases.
- **TD9-103** — `byroredux/src/render.rs::build_render_data` at **1 274 LOC**. Extract per-draw-class subroutines.
- **TD9-104** — `crates/nif/src/import/material/walker.rs::extract_material_info_from_refs` at **799 LOC**. Newly visible since Session 36 mesh/material split surfaced it. Extract per-property-kind handlers.
- **TD9-105** — `crates/plugin/src/esm/records/mod.rs::parse_esm_with_load_order` at **761 LOC** with the 102-arm FourCC dispatch (TD3-101). Resolved by the macro/table proposal there.

#### Dimension 10 — Audit Rot (3)
- **TD10-011 / TD10-012** — Audit-skill GpuInstance shader-mirror count is wrong (says 3, actual 5). Repointing the closeout commit gave the wrong count. Cross-ref TD7-039.
- **TD10-013** — `audit-renderer.md:282` + `audit-safety.md:76` catalog DBG_* bits at `triangle.frag:628-686` (actual line range is **718-780**, ~90-line drift) and still list `DBG_FORCE_NORMAL_MAP = 0x20`. #1035 (closed today, 2026-05-14T18:03) renamed the bit to `DBG_RESERVED_20`. Closeout-rot specific to today's #1035 commit chain.

### LOW (~110)

Bulk findings by dimension; see per-dimension reports under `/tmp/audit/tech-debt/` (deleted post-merge — re-run to regenerate):

- **Dim 1 (0)** — clean below MEDIUM.
- **Dim 2 (18 net-new)** — 8 unused workspace deps (TD2-101..109), 9 cargo-check warnings including 2 unused `MeshResolver` imports from Session 36's `mesh/` split (TD6-114..115 sibling), 1 truly dead `pub(crate) fn current_frame_id` in `texture_registry.rs`. Plus 16 baseline TD2 findings still live (the prior audit covered them). Promote-candidates: `unload_cell` and `OneCellLoadInfo.cell_root` are now both consumed but their docstrings say "write-only".
- **Dim 3 (10 tail items)** — Z-up→Y-up coord-flip 4th leak in `byroredux/src/systems/particle.rs:15` (TD3-107). 7× consecutive `for buf in &mut self.X { buf.destroy(); } self.X.clear();` in `scene_buffer/descriptors.rs::destroy` (TD3-106). TLAS WriteDescriptorSet at 3 sites (TD3-109). Exterior grid `gx * 4096.0` at 2 sites in `cell_loader/exterior.rs` disagreeing on Z-flip sign (TD3-110). Plus 6 more tail items per `/tmp/audit/tech-debt/dim_3.md`.
- **Dim 4 (11)** — 89 bare `NifVersion(0x...)` literals (TD4-108, down from 113), 33 bare `bsver()` compares in `blocks/` (TD4-109), 107 ESM `data.len() == N` subrecord checks (TD4-110).
- **Dim 5 (12)** — Soft stubs + gated parse-but-don't-consume; cross-refs `#1047`.
- **Dim 6 (15)** — Smoke-only asserts (5), bare `unwrap()` in test bodies, `#[ignore]`d audio + facegen + bgsm tests with no `BYROREDUX_*_DATA` honor.
- **Dim 7 (10)** — Source-doc-comment drift on byte sizes, struct field counts.
- **Dim 8 (8)** — 9 free-function `_var` params (TD8-009..010, TD8-013, TD8-015), 13 blanket `#[cfg(test)] #[allow(unused_imports)]` blocks (TD8-011..012).
- **Dim 9 (8)** — Files in 1500–2000 LOC band: `tri_shape.rs` (1910), `import/walk.rs` (1867), `interpolator.rs` (1671), `shader.rs` (1641), `records/mod.rs` (1613); each gets a 1-line split proposal.
- **Dim 10 (8)** — Bare-line-number drift in `audit-renderer.md` against `triangle.frag` / `triangle.vert` (multiple ~80-line drifts), `mesh.rs:87` cited against a file that was split today by Session 36, stray empty `.claude/issues/could/` directory, ISSUE.md lacks Status field (20 of 30 sampled local dirs have GH=CLOSED, zero locals have any state hint).

### INFO (8)

- **TD3-001..004** — Closed via `#1043` / Session 35 (KeyParse trait, allocate_vec sweep, descriptor pool builder, texture upload + barrier helpers).
- **TD4-002 / TD4-007 / TD4-011 / TD4-012 / TD4-014 / TD4-016 / TD4-018 / TD4-019** — Closed via Session 36 + `#1037` + `#1042`.
- **TD5-006** — M55 volumetrics demoted (was MEDIUM "clear-only skeleton"; now a documented `VOLUMETRIC_OUTPUT_CONSUMED: bool` lockstep keep-alive).
- **TD7-001..008** — Closed via `#1039` / commit `65e5fd5`.
- **TD8-014** — 23 of 33 `_var`-prefixed params are trait/blanket-impl-forced (`ConsoleCommand::execute`, `System::run` via blanket `impl<F: FnMut> System for F`) — kept as INFO; the impl signature is non-negotiable.

## Deferred

These findings are real but gated on milestones still in progress. Surface them here so they're not re-discovered every audit.

| Finding | Gating milestone / issue |
|---------|--------------------------|
| TD5-001 SpeedTree placeholder | SpeedTree Phase 2 (no ROADMAP row yet — propose Tier-9 addition) |
| TD5-002 StencilState | `#337` — renderer follow-up for stencil-masked decals/portals |
| TD5-003 BSSky/Water shader flags | Skyrim sky polish (post-#993 partial) |
| TD5-008 IMGS/ACTI/TERM | Tier 4 (interactivity) |
| TD5-016 BPTD body parts | M41 Phase 4 (dismemberment routing) |
| TD9-101 / TD9-102 `draw.rs` + `context/mod.rs` split | Dedicated `/refactor-session` with RenderDoc baselines between steps |

## Dimension Reports (raw)

Per-dimension reports were written to `/tmp/audit/tech-debt/dim_{1..10}.md` during this run. They contain the full finding list (including bulk LOWs not enumerated here). After Phase 4 cleanup they are deleted — re-run `/audit-tech-debt` to regenerate.

## Recommended next action

Publish this report:

```
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-05-14.md
```

Expect ~15 batched issues to file under the `tech-debt` label, with the **Top 10 Quick Wins** (~3–5 hours of work total) being the highest-leverage cluster — they close 5 carry-over MEDIUMs from 2026-05-13 plus all 15 Session-36 audit-skill rot findings in one weekend afternoon.
