# ByroRedux — History

Session narratives and audit-bundle closeouts, newest first. Append-only.
For current state see [ROADMAP.md](ROADMAP.md); for fine-grained archaeology
see `git log`.

New entries are drafted by `/session-close` at the end of each working
session. The canonical entry shape is:

```
## Session N — <one-line theme>  (YYYY-MM-DD, <commit range>)

<one-paragraph "why this session happened">

- **Bucket A** — bullet list of concrete shipped work, with issue refs
- **Bucket B** — …

<one-line "net effect" — test count delta, LOC delta, any bench delta>
```

Anything that's not a session narrative (per-issue archaeology,
closed-issue lists, resolved-known-issue logs) should not land here.
Commits hold that record.

---

## Session 33 — Markarth renders: Tier 8 ship (M55/M58/M-LIGHT v1) + SpeedTree Phase 1 + perf audit-bundle close + NIF `until=` doctrine flip + Anniversary Edition path-strip  (2026-05-08 → 2026-05-10, 33f48b5..e2409c0)

21-commit multi-day session that converged three structural threads and one live-test catch. The **Tier 8 visual fidelity** stretch lit up an entire indirect-lighting pipeline in one commit — M55 volumetrics (froxel inject + integrate, single-shadow-ray RT against TLAS), M58 bloom pyramid (5-mip down + 4-mip up, R11G11B10F), M-LIGHT v1 stochastic single-tap soft shadows (angular cone), and a golden-frame regression harness pinned to the cube demo. The **SpeedTree Phase 1** arc dissected the FNV/FO3/Oblivion `.spt` TLV format from scratch (single-file dissector → tag dictionary recovery → TLV walker hitting the ≥95% acceptance gate → importer placeholder fallback → cell-loader extension switch → `--tree` CLI surface + smoke test) so foliage at least renders a placeholder card instead of crashing the importer. The **performance + NIF audit-bundle** thread closed 5 perf findings (#928–#932) and ran a fresh NIF audit that surfaced the load-bearing **`until=X` semantic doctrine flip** (#935) — niftools/nifly are inclusive (`<=`), the post-#765/#769 sweep had chosen exclusive (`<`), and every shipping `until=` gate sat at versions older than 20.0.0.5 so the bug was silent on Bethesda content but bit pre-Bethesda Gamebryo / NetImmerse legacy. The closing arc was a **live Markarth render** that surfaced an Anniversary Edition compat bug — `tex.missing` reported juniper / reach branches / driftwood all authored with the full pipeline-internal prefix `skyrimhd\build\pc\data\textures\…` that the real Skyrim runtime strips at lookup time — and the path-normalize fix that ended with the user looking up at a real reach-tree silhouette against the Markarth sky.

- **Tier 8 visual fidelity ship (3 commits)** — `33f48b5` (M55 volumetrics + M58 bloom + M-LIGHT v1 + golden frame harness) lights up the full indirect-lighting / glow / soft-shadow pipeline in one batch. M55 volumetrics adds a 160×90×128 froxel grid with two compute passes (inject + integrate), a single shadow ray per froxel against the TLAS, and HG phase scattering; sized 2× 14 MiB / slot. M58 bloom adds a 5-mip down + 4-mip up pyramid in B10G11R11_UFLOAT with a 4-tap bilinear box filter; intensity tuned to 0.15 (initially 0.20 — too high on Prospector saloon). M-LIGHT v1 ships stochastic single-tap soft-shadow with `sunAngularRadius` bumped 0.0047 → 0.020. The golden-frame harness at `byroredux/tests/golden_frames.rs` captures the cube demo at frame 60 against a checked-in PNG; opts into `--ignored` so a missing Vulkan device doesn't fail CI. `f62d4bd` glass improvements — single-sided override, fresnel rim, diffuse mip-bias at the fresnel-fallback path (the saved feedback memory "chrome → missing textures" caught this one cleanly). `b536299` (#905) bloom + volumetric image-view rebind on composite resize — without it, the new pipelines were holding stale views after window resize and the composite shader sampled garbage.
- **SpeedTree Phase 1 unblock (9 commits)** — `92f4045` regression-locks the Phase 0 closures (#611 / #615 / #872) before any forward motion. `bba63cc` adds a dedicated TREE record parser to `crates/plugin` — pre-fix every `.spt`-referencing TREE silently fell into the generic record path and lost its texture/billboard data. `8b77cb7` recon harness + corpus baseline for the format. `23abd4b` single-file dissector + TLV format crack — `.spt` is a tag-length-value chunk format with a (verified) ≈40 known tags. `6f83b1c` analyzers — full tag dictionary recovered against the corpus. `5e2f54d` TLV walker hit the **≥95% acceptance gate** against the FNV/FO3/Oblivion `.spt` corpus. `10e716a` importer ships a placeholder billboard fallback so even un-decoded trees render as a card (better than a parse panic). `674aa91` cell-loader extension switch — `.spt` references now route to the SpeedTree importer instead of NIF. `af6ad36` `--tree` CLI flag for direct visualisation + smoke test.
- **Renderer audits dim 11 + dim 16 filed (1 commit)** — `732487a` ships two new audit reports under `docs/audits/` (TAA-deep + bloom-pyramid-deep, the inputs to the perf bundle below).
- **Performance audit-bundle close (5 commits, 5 issues)** — `a5a5b6a` (#928 PERF-GP-01) gates volumetrics dispatch behind `VOLUMETRIC_OUTPUT_CONSUMED: bool = false` so the integrate pass doesn't run unless composite actually reads its output; lockstep contract documented across `volumetrics.rs` ↔ `composite.frag`. `97cae8a` (#929 PERF-CPU-01) wraps the M41 dropout-detection scaffold in `cfg!(debug_assertions)` so release builds stop paying the per-frame allocation. `04316f7` (#930 PERF-GP-02) collapses `pipeline_two_sided` and drops `two_sided` from the blend cache key — pipeline duplicates were redundant once `VK_DYNAMIC_STATE_CULL_MODE` came online; `DrawBatch.two_sided` field added as the merge-key-relevant info that was previously implicit on `PipelineKey`. `efe4c34` (#932 PERF-CPU-02) promotes `footstep_system`'s scratch Vec to a `FootstepScratch` Resource (`Vec::with_capacity(32)`, `mem::take` + restore pattern preserving capacity across frames). `ffbd3f1` (#931 PERF-GP-03) drops 9 redundant pre-barriers in `BloomPipeline::dispatch` (19 → 10 barriers/frame, 47% cut) — each `BloomFrame` slot owns its own mip allocations so cross-frame WAR is gated by the per-frame fence; rejected the audit's "~3 barriers" target as unachievable without single-pass FidelityFX SPD (several-hundred-LOC shader rewrite). Two perf findings deferred: #933 volumetric integrate early-out (subordinate to #928), #934 par_sort vs serial benchmark (needs criterion harness).
- **NIF audit + publish (1 audit doc + 10 issues filed)** — `/audit-nif` ran across 6 dimensions, surfaced 14 NEW findings (2 HIGH, 6 MEDIUM, 6 LOW) + 4 VERIFIED regression guards. `docs/audits/AUDIT_NIF_2026-05-10.md` is the report. `/audit-publish` filed 10 issues: #935 (until= semantic), #936 (BSplineComp Float/Point3 dispatch), #937 (`Fallout3.bsver()` returns 21 not 34), #938 (feature-flag predicate hygiene), #939 (drift-detector telemetry gap), #940 (importer drops Tile/Sky/Water/TallGrass `file_name`), #941 (BSTreadTransfInterpolator), #942 (FO76 InstancedNode + SSE LargeRef), #943 (`version.rs` test/dev-FO3 hygiene), #944 (NiSourceTexture cond-gate regression test). NIF-D5-NEW-04 / NIF-D5-NEW-05 bundled into the existing #728 / #329 threads.
- **NIF `until=X` doctrine flip (1 commit, #935, ~14 sites across 11 files)** — `fa6b0bd` flips every `// see #765 sweep` / `// exclusive` call site from `version < NifVersion(0xN)` to `version <= NifVersion(0xN)`. niftools' own `verexpr` token table defines `#NI_BS_LTE_FO3#` with operator `#LTE#` and the description "All NI + BS *until* Fallout 3" — `until` is colloquial for inclusive `<=`; nifly mirrors this in C++ with `<= V10_0_1_X` consistently. Pre-fix, on v=10.0.1.3 exactly `NiSourceTexture::Use Internal` was skipped (1 byte under-read); on v=10.4.0.1 exactly TexDesc PS2 L/K was skipped (4 bytes under-read). Bethesda content unaffected (every gate is older than 20.0.0.5), but pre-Bethesda Gamebryo / NetImmerse legacy (Civ4 Colonial Fleet, IndustryGiant 2, Morrowind-era mods, v10.0.1.2 BSStreamHeader files) now reads boundary fields correctly. Doctrine documented at the top of `version.rs` so future contributors don't re-flip. Boundary-regression tests rewritten under the new doctrine in `properties.rs`, `interpolator.rs`, `texture.rs`. Bundles NIF-D1-NEW-01 (NiStencilProperty siblings) + NIF-D1-NEW-02 (`target_color` since-gate on NiLightColorController / NiMaterialColorController).
- **BLAS eviction static/total split (1 commit, #920)** — `52d5a7f` adds `static_blas_bytes` as the eviction-eligible subset of `total_blas_bytes`. Pre-fix `evict_unused_blas` and the mid-batch eviction predicate compared `total_blas_bytes` (static + skinned) against `blas_budget_bytes`, but the eviction loop only walks `blas_entries` (static slots) — so on post-M41 NPC-heavy scenes skinned BLAS could push `total_blas_bytes` permanently over budget and LRU-thrash static BLAS every frame. Static add / drop / evict sites bump both counters in lockstep; skinned add / drop sites bump only `total_blas_bytes`. Regression test pins the input contract: 70% static + skinned overflow → no eviction; 90% static → eviction fires.
- **Markarth Anniversary Edition compat (1 commit, no issue — live-test catch)** — `e2409c0` adds `strip_build_prefix` helper to `byroredux/src/asset_provider.rs` and applies it at the entry of `resolve_texture_with_clamp`. Some shipping Bethesda content — most notably the Skyrim AE "Skyrim HD" trees, plants, and landscape clutter — embeds texture paths with the full pipeline-internal prefix `skyrimhd\build\pc\data\textures\…` that the real Bethesda runtime strips against a `Data\` root. Discovered live via `byro-dbg` `tex.missing` on a Markarth grid (0,0) radius 2 render — pre-fix reported 10 unique missing textures × 157 instances (24× florajuniper, 22× tundradriftwoodbranches, 22× reachtreebranch, etc.); post-fix dropped to 1 unique (`<no path, no material>` × 65, which is REFRs with no path at all — not a resolution miss). Textures registered in the registry went 361 → 370. The visual delta: foliage stopped rendering as checker billboards and showed up as real reach-tree silhouettes against the Markarth sky. Strip is generic ("last `\data\` (case-insensitive) wins") so future CC packs with different build prefixes work too. 7 unit tests cover the headline case, case-insensitive token, both separator styles, last-boundary rule, no-`\data\` passthrough, and trailing-`\data\` edge.

Net: tests **1827 → 1879 (+52)**, Rust LOC (non-test) **~147 575 → ~153 802 (+~6 227)**, total LOC **~153 335 → ~160 086 (+~6 751)**, source files **290 → 307 (+17)**, workspace members **18 → 19** (new `byroredux-spt` crate for SpeedTree), open issue dirs **858 → 898 (+40)**. ~8 GitHub issues closed (#905, #920, #928, #929, #930, #931, #932, #935). Three new audit reports filed (`AUDIT_RENDERER_2026-05-07.md`, `AUDIT_RENDERER_2026-05-07_DIM15.md`, `AUDIT_RENDERER_2026-05-08_DIM11.md` were already from session 32; this session added the **2026-05-07 dim 11 + 16** pair, `AUDIT_PERFORMANCE_2026-05-10.md`, `AUDIT_NIF_2026-05-10.md`, plus the renderer dim-12 audit that surfaced #920). New CLI surface: `--tree` for direct `.spt` visualisation. Bench-of-record `6a6950a` now **455 commits stale** (was 433 at session-32 close, +22); rapid drift continues to be a Known Issue (R6a-stale-7). Visual demo of the session: a real reach-tree silhouette over Markarth at 45.6 wall_fps / 21.91 ms / 12 725 entities / 6 833 draws — first time foliage has rendered as foliage in a Skyrim cell.

---

## Session 32 — Audit-driven sweep: FNV-D5 + Renderer-D11 closeouts, M41-EQUIP Phase 2 close-out, smoke-test framework  (2026-05-08, cfc89af..0af2aa9)

13-commit single-day session, audit-driven. Three structural arcs converged: an **FNV-D5 dimension-5 audit** ran at HEAD `318fcaf`, surfaced 3 findings (#900 / #901 / #902), all closed within the same session — the load-bearing one (#900) was a `skin_compute` descriptor-pool exhaustion under the new M41-EQUIP entity volume that turned RT shadows off on overflowing NPCs and dumped 58 WARN / 300 frames of retry-spam. A **Renderer-D11 deep TAA audit** found 2 LOW shader-only defects (#903 NaN-propagation reliance on undefined GLSL `min`/`max`, #904 full-u16 mesh_id disocclusion compare) and shipped the fix to TAA + the SVGF temporal sibling in one batch. **M41-EQUIP Phase 2 close-out** finally got the LVLI dispatch the prebaked path needed (vanilla Skyrim+ outfits reference leveled lists, not direct ARMO refs — pre-fix Whiterun NPCs silently spawned with no gear) plus the new `--bench-hold` infrastructure, the `Inventory` / `EquipmentSlots` debug-server registration, and a runnable smoke-test harness with hard / soft pass-fail assertions. Plus four standing-queue closures: #337 NiStencilProperty capture, #720 BSEyeCenterExtraData FO4/FO76 dispatch, #873 BSGeometry per-element push-loops, #848 footstep_system stage-ordering, #891 NiTextureEffect Phase 1 import.

- **FNV-D5 audit closeouts (3 issues, audit doc `AUDIT_FNV_2026-05-08.md`)** — `4b1f56d` (#900) bumps `SKIN_MAX_SLOTS` 32 → 64 + adds `failed_skin_slots: HashSet<EntityId>` retry-suppression cleared on any LRU eviction. The pre-fix pool-sizing comment claimed `max_slots == 32 (matches MAX_TOTAL_BONES / MAX_BONES_PER_MESH)` — math was wrong (real ratio = 32768/128 = 256), comment rewritten honestly. `53f4f64` (#901 + #902) refreshes ROADMAP's `62 219` FNV ESM count to `73 054 structured + 5 625 long-tail` (post-#808/#809/#810 dispatch closeout) and rewrites R6a-stale-7's gating clause: M41-EQUIP B.2 landed, the deferral is falsified, refresh is overdue with HEAD-captured numbers (entities 2562 / FPS 143.7) inline.
- **Renderer-D11 deep TAA audit closeouts (2 issues, audit doc `AUDIT_RENDERER_2026-05-08_DIM11.md`)** — `48b106f` (#903 + #904) ships shader-only fixes to both `taa.comp` AND `svgf_temporal.comp`. NaN/Inf history guards stop relying on undefined GLSL `min`/`max` semantics for implicit NaN filtering (TAA pre-clamp, SVGF rejects-tap on detect). 15-bit mesh_id mask on disocclusion compares so bit-15 (`ALPHA_BLEND_NO_HISTORY`) toggles don't force a one-frame history reset on opacity transitions. SPV regenerated; no Vulkan-state changes (safe under `feedback_speculative_vulkan_fixes.md`).
- **M41-EQUIP Phase 2 close-out (4 commits)** — `be4663b` lights up `byroredux_plugin::equip::expand_leveled_form_id` — recursive resolver that flattens LVLI references (`OTFT.items` + `npc.inventory` CNTO entries) into base ARMO/WEAP form IDs gated on `actor_level`. Single-pick (highest-level eligible) is the Bethesda flag-bit-0-unset default; multi-pick lands all eligible (over-equips slightly, safer for render-audit workflows). 8 new tests cover passthrough, level gating, multi-pick, nested recursion, circular cap, unknown id. Both spawn paths route through it. `73adffb` adds `--bench-hold` CLI flag — keeps the engine open after the `bench:` summary so `byro-dbg` can attach against the loaded scene; closes the FNV-D5 audit's "couldn't capture `tex.missing` live" coverage gap. `9b957bb` ships `docs/smoke-tests/m41-equip.sh` + the README pattern doc + `Inventory` / `EquipmentSlots` / `ItemStack` / `InventoryIndex` / `ItemInstanceId` `inspect`-feature serde derives + debug-server registration for the equip components.
- **Smoke-test self-debugged from first run** — the user ran `m41-equip.sh` against FO4 MedTekResearch01 and the output surfaced three real bugs in the script itself: `find Inventory` was wrong syntax (correct is `entities <Component>`), `cleanup_done` was unbound under `set -u` because bash tears local variables down before `RETURN` traps fire, and the FO4 archive list omitted `Textures3-9.ba2` + `MeshesExtra.ba2` + `Materials.ba2` (pre-fix `tex.missing` reported 47 unique misses including 213× `officeboxpapers01_d.dds` and 46× `metallocker01.bgsm`). All three closed in `085321d`. `3422884` adds hard / soft pass-fail assertions parsing the bench `entities=N` / `draws=N` line + `byro-dbg`'s `(N entities)` summary lines + `tex.missing`'s `N unique missing textures` JSON header. Hard floors per cell (FO4 5000 entities / 4000 draws; Skyrim 1200 / 700) absorb half the observed values; soft thresholds warn but don't fail. The 10 809-entity FO4 load at 57.9 FPS / 17.27 ms is itself a healthy positive signal — M41-EQUIP + LVLI dispatch is producing real geometry.
- **Standing-queue closures (5 issues)** — `318fcaf` (#337 D4-NEW-01) `MaterialInfo.stencil_state: Option<StencilState>` captures all 7 non-`draw_mode` fields of `NiStencilProperty` so the silent drop at the importer boundary is closed; renderer-side stencil pipeline variants stay deferred behind two real dependencies (depth-format flip, per-MaterialKind variants) per the speculative-vulkan-fix policy. `9173920` (#720 NIF-D5-04) dispatches `BSEyeCenterExtraData` on FO4/FO76 — 625 vanilla instances no longer fall into NiUnknown; mirrors #710's `BSPositionData` sibling pattern exactly with happy-path + hostile-count regression tests. `860b122` (#873 NIF-PERF-09) collapses 5 `BSGeometryMeshData::parse` per-element push-loops to single `read_pod_vec` calls (`colors`, `normals_raw`, `tangents_raw`, `meshlets`, `cull_data`); `Meshlet` + `CullData` gain `#[repr(C)] + Default`, `read_pod_vec` widens to `pub(crate)`, new `read_u8_quad_array` wrapper for RGBA. `058fea6` (#848 AUD-D6-NEW-07) moves `footstep_system` from `Stage::Update` (parallel, pre-propagation) to `Stage::PostUpdate` (exclusive, post-propagation) — the pre-fix "~3 cm of motion" rationale underestimated fly-cam boost by ~100×. `0af2aa9` (#891 LC-D2-NEW-01) Phase 1 — new `ImportedTextureEffect` + `walk_node_texture_effects` + `import_nif_texture_effects` mirror the `ImportedLight` shape so vanilla Oblivion sun gobos / FO3 / FNV light cookies / magic-FX env maps no longer parse-and-silently-discard at the importer boundary; renderer Phase 2 stays deferred.

Net: tests **1811 → 1827 (+16)**, Rust LOC (non-test) **~146 399 → 147 575 (+~1 176)**, total LOC **~152 159 → 153 335 (+~1 176)**, source files **289 → 290 (+1)**, workspace members **18** (unchanged), open issue dirs **853 → 858 (+5)**. ~9 GitHub issues closed (#337, #720, #848, #873, #891, #900, #901, #902, #903, #904). Two new audit reports filed (`AUDIT_FNV_2026-05-08.md`, `AUDIT_RENDERER_2026-05-08_DIM11.md`). New CLI surface: `--bench-hold` + `--materials-ba2` (the latter pre-existed but was newly surfaced in CLAUDE.md). New observability surface: `docs/smoke-tests/` with the M41-equip script as the first runnable smoke. Bench-of-record `6a6950a` now **433 commits stale** (was 419 at session-31 close, +14); the R6a-stale-7 row's narrative was rewritten in #902 to drop the falsified-deferral framing.

---

## Session 31 — Cell-load perf bundle, M41-EQUIP scaffold, REFR rotation fix, audit-bundle closeout  (2026-05-06 → 2026-05-08, 086b25c..470f737)

55-commit session spanning two-and-a-half calendar days. Three structural arcs converged: the 2026-05-06 cell-load performance audit (dims 7 + 9) drove a coordinated batch that turned per-REFR / per-NPC / per-frame O(N²) and O(N) hot paths into deduped / batched / dirty-gated O(1)s — REFR placement dedup (#879), NPC spawn cache (#880), batched DDS uploads (#881), batched StringPool lock (#882), unload_cell single fan-out (#883), dirty-gated material SSBO (#878), and `tracing` spans across the whole chain (#886) so the next regression is observable instead of inferred. M41-EQUIP shipped a five-phase scaffold (#896 Phases A.0 → B.2) that introduced `Inventory` + `EquipmentSlots` components, the `ItemInstancePool` resource, and per-game ARMO → worn-mesh resolution for both kf-era and Skyrim+ paths — NPCs now spawn wearing their default outfit. A long-running REFR rendering bug pinned to wrong Euler→Y-up rotation composition order (`Rx · Ry · Rz`, not `Rz · Ry · Rx`, after the diagnostic CLI flag landed in `196dd67`). On top: REN-D15 audit closed out, the NIF parser perf cluster (#834, #872, #874-#876) rolled `Vec<String>` → `Vec<Arc<str>>` + `read_pod_vec` extension into 5 sites, and the M44 audio crate's "API ships, cell loader doesn't call it" reverb-send caveat from Session 30 finally got wired (#846).

- **Cell-load perf bundle (2026-05-06 audit dims 7 + 9, 7 commits)** — `d5f0862` (#879) refcount-dedups REFR placements that share a cached NIF; `2081338` (#880) routes NPC spawn through the process-lifetime scene-import cache so Megaton's 31 dwellers no longer re-parse skeleton + body + head from BSA bytes per spawn; `7c6c156` (#881) batches cell-load DDS uploads into one fence-wait; `fc06921` (#882) batches the StringPool write-lock per `spawn_placed_instances` call (was per-mesh, churning the lock); `a79dfb9` (#883) collapses `unload_cell`'s six sequential SparseSet scans over the victim list into a single fan-out walk; `683bc3b` (#878) dirty-gates the per-frame material SSBO upload via content hash so byte-identical frames skip the upload; `73a7a66` (#886) wires `tracing` spans across the cell-load critical path so the next regression is locally diagnosable. Two refactors fall out: `0c3b61c` extracts `DeferredDestroyQueue<T>` shared by mesh + BLAS; `ca3cbdb` extracts `ParsedNifCache<T>` shared by both NIF cache resources.
- **M41-EQUIP scaffold (#896, 9 commits across two phases)** — `0a0d652` Phase A.0 lands `Inventory` + `EquipmentSlots` components + `ItemInstancePool` resource without wiring any spawn/render path. `f1b3156` Phase A.1 walks NPC inventory, spawns ARMO meshes, populates ECS — concurrent body+armor render kept as deliberate spike; `21ae560` Phase A.2 pre-scans for body-slot armor and skips `upperbody.nif` when present, killing the z-fight + 2×-bone-palette overhead. `121c705` Phase B.0 + `24a7bd8` Phase B.1 + `4ec9bb6` Phase B.2 build the per-game `resolve_armor_mesh` helper (Skyrim+ ARMO → ARMA → worn-mesh chain) and wire it into both spawn paths. `775412f` aligns `ItemStack.base_form_id` with the codebase's u32 form-id convention; `b9a6bc6` credits xEdit / ElminsterAU for the ESM record-shape definitions.
- **REFR rotation pinned (2 commits)** — `196dd67` ships a `--rotation-mode 0..3` CLI flag to triage every Euler→Y-up composition order in isolation; `386aabb` confirms the bug as wrong composition: REFR Z-up Euler `(rx, ry, rz)` must be `Quat::from_euler(Rx) * Quat::from_euler(Ry) * Quat::from_euler(Rz)` after the Z-up→Y-up axis remap, not the reverse the importer was using. Closes a long-running placement bug visible across exterior cell loads.
- **REN-D15 audit closeout + renderer ambient polish (3 issue commits + 4 renderer)** — `84eb74e` (#897 REN-D15-01) derives the fog `night_factor` from the climate-driven TOD slot pair so palette and fog stop disagreeing on "day vs transitioning" for ~0.3-2h windows on non-default CLMTs (FO3 Capital Wasteland's `[5.333, 10, 17, 22]` was the canonical case); `f9683ab` (#898 REN-D15-02) fixes the `triangle.frag::INTERIOR_FILL_AMBIENT_FACTOR` docstring's perceptual claim; `f92a2b4` (#899 REN-D15-03) gives WTHR ANAM/BNAM cloud layers 2/3 distinct interim scroll multipliers (`0.85×`/`-1.15×` base U) so the four-layer composite no longer collapses onto two visually identical pairs when ANAM/BNAM is absent or matches DNAM/CNAM. Around them, four renderer-side ambient tweaks: `f684a91` Kaplanyan-Hoffman specular antialiasing; `cdc3b01` half-Lambert wrap on interior-fill directional; `98d644c` isotropic ambient injection for interior-fill (replaces the per-fragment Lambert term); `977682a` metallic ambient + geometric-normal AO for corrugated metal (Nellis museum). Plus `15c1eab` (#887) gates the BSTriShape Bitangent X / Unused W slot read on `VF_TANGENTS`.
- **NIF parser perf cluster (5 commits, 3 audits' worth of NIF-PERF rows)** — `a8495b7` (#834 NIF-PERF-05) promotes `NifHeader.block_types: Vec<String>` to `Vec<Arc<str>>` + new `block_type_name_arc(i)` accessor so the four NiUnknown recovery sites refcount-clone instead of paying `Arc::from(&str)` per dispatch failure; `c1b8bfc` (#872 NIF-PERF-08) adds `HasObjectNET::name_arc()` (default `None`, override on every NiObjectNETData-backed block) so `walk.rs`'s `resolve_affected_node_names` and `resolve_block_ref_names` refcount-bump existing `Arc<str>` storage instead of allocating fresh names; `1573660` (#874 NIF-PERF-10) adds `NifStream::read_u16_triple_array` — three sites collapse `read_u16_array(N*3) + chunks_exact(3).map(...).collect()` into one `read_pod_vec::<[u16; 3]>` (BsTriShape inline path drops a dead `allocate_vec` pre-allocation along with it); `b2a7451` (#876 NIF-PERF-12 + #875 NIF-PERF-11) bulk-reads `block_type_indices` / `block_sizes` via a new `read_pod_vec_from_cursor` helper in header.rs and changes `MorphTarget.vectors` from `Vec<[f32; 3]>` to `Vec<NiPoint3>` so the bulk-read result moves in place (axis-preserving collect was a pure no-op memcpy).
- **Parser correctness + observability (8 issues)** — `c2c62e2` (#571 SK-D1-02) warns when BSDynamicTriShape produces vertices but no triangles (silent-import path now audible on broken / stripped-down mod facegen NIFs); `3b2e489` (#816 FO4-D4-NEW-04) preserves SCOL `FULL` display name (124 / 2617 vanilla SCOLs ship one) — `ScolRecord.full_name` now matches `PkinRecord::full_name`'s lstring-or-zstring routing; `4b38f49` (#889 SK-D1-NN-03) renormalises half-float skin weights at decode via a new `renormalize_skin_weights` helper shared by the inline and SSE-buffer twins (was asymmetric vs the NiSkinData path's `densify_sparse_weights`); `470f737` (#888 SK-D1-NN-02) documents `decode_sse_packed_buffer`'s SSE-only contract + adds the `VF_FULL_PRECISION` constant for the future FO4-extension branch; `2143899` (#892 LC-D2-NEW-02) adds opt-in `ParseOptions::validate_links` flag + `NifScene::link_errors` field so dangling `BlockRef`s surface in dev / `nif_stats` builds without forcing the cost on the default path. Other parser additions: `e973d0d` (#815) parses PKIN `FLTR` workshop build-mode filter; `c43e740` (#893) adds the `StringPool::intern` / `get` stack-buffer fast path (256-byte `LOWERCASE_STACK_BUF`, allocation-free for 99% of asset paths); `6307b6d` (#895 LC-D6-NEW-03) documents `StringPool::resolve`'s lowercase-canonical-form divergence vs Gamebryo's case-preserving `NiFixedString`. Diagnostics: `3956167` (#598) ships a `ba2_ratio_anomaly` scanner; `58ab3cf` (#601) extends `nif_stats` with `--all` + `--min-count` for long-tail visibility; `843aed2` (#841) extends the skin inspector with bone-world + palette dump as a triage tool for the FNV body-skinning spike artifact.
- **Audio M44 follow-on + audit polish (8 issues)** — `6794b04` (#846) wires the reverb send to interior/exterior cell type (the API shipped in M44 Phase 6, no caller had flipped it — every cell sounded the same). `88443c6` (#843 + #844 + #847) batches three audit findings: `AudioWorld.multi_listener_warned` debounce flag for multi-AudioListener scenarios; `ActiveSound.stop_issued` flag eliminates `prune_stopped_sounds`'s tick-by-tick re-stop walk during fade-out windows; `set_reverb_send_db` doc names the kira `with_send` build-time-only constraint so callers know it's a "next-dispatch knob" not a live mixer fader. `9f90a72` (#852 + #851 + #859): `pending_oneshots` becomes `VecDeque` with O(1) `pop_front` at cap (was O(n) `Vec::remove(0)` shift); `drain_pending_oneshots` moves the manager-active gate before `mem::take` (defensive — the unreachable branch could otherwise drop the queue); `SoundCache` doc names the dormant-API status. `2143899` (#849) documents `sync_listener_pose`'s listener-handle reuse contract across `AudioListener` entity churn (kira `listener_capacity = 8` matters). The R1 follow-up `84ab376` (#781) skips `to_gpu_material()` on MaterialTable dedup hits — the cached `GpuMaterial` is byte-identical so the conversion was wasted work.
- **ECS / scheduler / debug-server doc hygiene + cleanup (6 issues)** — `35da45e` (#867 + #868 + #857): `Scheduler::run` rustdoc names panic-propagation behaviour in both `parallel-scheduler` enabled / disabled builds (rayon's no-cancel-on-panic vs sequential short-circuit); same docstring spells out the structural re-entry impossibility (`Scheduler` is intentionally not a `Resource`) so a future maintainer doesn't promote it and trip `BorrowMutError`; debug-server bind hostname carries a lockstep-coupling note at both call sites. `80a27db` (#885 CELL-PERF-09) replaces `stamp_cell_root`'s per-eid push loop with `entry.extend(first..last)`. `80d5fd6` (#866 FNV-D6-NEW-07) case-folds `AnimationClipRegistry` path keys internally (was caller-normalised contract with no enforcement; foot-gun for IDLE-record / Papyrus-routed callers).
- **Closed-issue refs + cleanup (3 commits)** — `aab7dd6` (#725) import-pipeline polish (vertex_map drop, parallax defaults, Lighting30 comment); `606aca0` (#840) replaces stale closed-issue refs in NIF aggregator log strings + tightens a regression assert; `616cbac` (#812) promotes BA2 zlib size-mismatch log to warn level.
- **Audits filed** — `0392781` (perf dim 7 + 9 audit), `d9805d0` (Legacy compat dim 6 + Skyrim dim 1 + 4); `docs/audits/AUDIT_RENDERER_2026-05-07.md` + `_DIM15.md` filed mid-session driving the REN-D15 closeouts.

Net: tests **1729 → 1811 (+82)**, Rust LOC (non-test) **~140 312 → 146 399 (+~6 087)**, total LOC **~146 072 → 152 159 (+~6 087)**, source files **282 → 289 (+7)**, workspace members **18** (unchanged), open issue dirs **826 → 853 (+27)**. ~30 GitHub issues closed; M41-EQUIP scaffold open (#896 phases A + B shipped, full equip-render integration still pending). M44 audio's "cell loader doesn't call `set_reverb_send_db`" caveat from Session 30 closed by #846. Bench-of-record `6a6950a` now **419 commits stale** (was 363 at session-30 close); refresh still deferred until M41 visible-actor workload — the cell-load perf bundle is on the streaming-in critical path, not the steady-state per-frame path the bench measures, so the existing bench remains representative for now.

---

## Session 30 — M44 audio end-to-end, cell-streaming hardening, concurrency audit closeouts  (2026-05-05 → 2026-05-06, 9ec71d2..f3c0f08)

40-commit session spanning ~24 hours. Headline arc was M44 audio shipping six phases in a single push — `byroredux-audio` is now the 18th workspace crate, with kira-backed spatial sub-tracks, BSA decode, looping per-emitter sounds, streaming music, and a global reverb send. Around it, three companion arcs converged: the cell-streaming worker grew real fault-tolerance (panic catch + cache reuse + evicted-clip release) closing the durability gap surfaced during M40; the 2026-05-04 ECS performance audit's remaining items closed out (root cache, scratch hoist, billboard cycle collapse, NIF Vec pre-size); and a fresh concurrency audit on dims 2-3 found mostly fixed-without-closure issues plus three new low-severity defensive gaps. No bench refresh — `6a6950a` now 363 commits stale, still gated on M41 visible-actor workload.

- **M44 audio (Phases 1–6, 5 commits)** — `1532392` Phase 1 scaffolds the `byroredux-audio` crate on `kira 0.10` with `AudioWorld` + ECS components (`AudioListener` / `AudioEmitter` / `OneShotSound`); `b93c76f` Phase 2 wires BSA → `StaticSoundData::from_cursor` decode through symphonia + a `SoundCache` resource; `45a9864` Phase 3 lands real spatial playback through kira's spatial sub-track model with lazy listener creation, per-emitter `SpatialTrackHandle`, prune-on-Stopped; `3987ecd` Phase 3.5 adds the `play_oneshot` queue API + `FootstepEmitter` + `footstep_system` (XZ-plane stride accumulator, vertical motion excluded), `--sounds-bsa <path>` decodes canonical FNV dirt-walk WAV; `e191d9f` Phases 4+5+6 ship looping ambient (`AudioEmitter.looping = true` applies kira's `loop_region`), streaming music (`load_streaming_sound_from_bytes/_from_file` + `play_music` / `stop_music` / `is_music_active`), and the global reverb send (per-cell `set_reverb_send_db` over a `SendTrackBuilder.with_effect(ReverbBuilder)`). `db669e3` (#842) bumps kira sub-track + send-track capacities above default. 12 default tests + 5 `#[ignore]`d real-data integrations covering BSA decode, full lifecycle, queue-driven lifecycle, looping survives natural duration, streaming play/stop on real OGG.
- **Cell-streaming hardening (M40 follow-on)** — `6622c51` (#854) wraps the streaming worker in `catch_unwind` so a NIF parser regression no longer permanently bricks exterior streaming (closes the C6-NEW-01 finding from `AUDIT_CONCURRENCY_2026-05-05.md`); `37447c9` (#862) early-skips already-cached NIFs in the worker so re-entered cells don't re-parse; `f813546` (#864) early-outs `finish_partial_import` when the cache already holds the model; `8862394` (#863) releases evicted clip handles back into `AnimationClipRegistry` so cell-streaming doesn't grow it unboundedly; `a34cb04` (#861) plumbs extended XCLL fields through `CellLightingRes`; `ffaf74a` (#860) drops dead `CellLoadResult.weather / .climate` fields; `f3dc1ee` (#801) wires SVGF / TAA recovery from cell-streaming events (paired alpha bump on disocclusion).
- **ECS performance audit closeouts (final four from 2026-05-04 perf audit)** — `0e39203` (#826 ECS-PERF-04) caches the root set in `world_bound_propagation_system` keyed on storage-len triple; `ddfcc81` (#827 ECS-PERF-05) merges duplicate `Name` queries in `animation_system` prelude; `45d99ac` (#829 ECS-PERF-07) collapses the read+write `GlobalTransform` cycle in `billboard_system`; `cec205c` (#835 NIF-PERF-04) pre-sizes `ImportedScene` collection Vecs from block count to avoid the per-block reallocation walk.
- **Concurrency audit (dims 2 + 3) + 5 closeout fixes** — Filed `docs/audits/AUDIT_CONCURRENCY_2026-05-06.md` covering Vulkan sync + resource lifecycle. Most issues already silently fixed (`#677` DEN-9 SVGF/TAA recreate barriers actually re-issued via `initialize_layouts`, `C2-01` / `C2-02` from 2026-04-12 long-since closed). Three new findings, all defensive-gap LOW: `#871` SkinSlot `output_buffer` leaked when `allocate_descriptor_sets` fails after buffer alloc succeeds — fixed in `947e5f7`. Existing-issue closeouts: `f616941` (#655 LIFE-M2) `SwapchainState::destroy` upgraded to `&mut self` + clears views vec + nulls swapchain; `8deac1e` (#675 DEN-5) SVGF sky/alpha-blend early-out writes `moments.b = 0` (never accumulate) instead of `1.0` (first-frame seed); `7ecf861` (#799 SUN-N3) sun glow halo respects `sun_intensity` ramp so it fades with the disc through dawn/dusk; `3846648` (#807 R1-N7) reserves material slot 0 for the neutral-lit `GpuMaterial::default()` so the three-way overload (default-init / first-interned / over-cap fallback) collapses to a single clean fallback meaning.
- **M41 / animation closeouts** — `9ec71d2` (#794 M41-IDLE + M41.0) pins the animation chain healthy with a 3-layer regression suite: parser diagnostic + 4 synthetic e2e + 1 real-data e2e in `crates/nif/tests/mtidle_motion_diagnostic.rs` and `byroredux/src/systems.rs::animation_system_e2e_tests`. Real FNV `mtidle.kf` → `animation_system` produces a 1.49 component-wise rotation delta on `Bip01 Spine` after 4 ticks. `2b2057a` files #841 (M41-PHASE-1BX) — body-skinning spike artifact pinned with bind-pose disagreement diagnostic + spike-cause documentation gap; `8d62ac0` adds `skin.list` / `skin.dump` debug commands as triage tools for #841. `6995a7c` (#845 AUD-D4-NEW-04) per-emitter unload fade for looping sounds.
- **Parser + renderer hardening** — `0ef36fa` (#570 SK-D3-03) widens `material_kind` to `u32` end-to-end across the variant ladder (was `u8` truncating Skyrim+ kinds 256+); `9784b43` (#376 F2-02) extracts the FNV CONT DATA flags byte from the 5-byte payload (Oblivion ships 4-byte, FNV grew a flags byte); `3852bc9` (#91 SAFE-11) validates the pipeline cache header before passing to the driver — refuses corrupt / cross-vendor caches with a clean error instead of GPU-undefined behaviour; `cda40a1` (#625 CPU side) surfaces `BSValueNode` + `BSOrderedNode` subclass fields (was previously demoting both to `NiNode`); `286e1f1` (#870) `const_assert MAX_FRAMES_IN_FLIGHT == 2` for the shared depth image so a future bump to 3 fails at compile time instead of running with under-allocated framebuffers; `bda4eed` (#690) adds disk-resident v103 BSA regression test against vanilla Oblivion archives; `45428cf` (#791) inverts the `CellRootIndex` map for O(victims) cell unload (was O(roots) per victim, the cell-stream-out hot path); `5c89b79` (#792) clarifies `stamp_cell_root` inner-loop docstring; `3ed0a4e` (#821 REN-D9-NEW-02) documents the window-portal raw-N bias asymmetry; `cd0265c` cross-references the #869 wireframe / flat-shading deferral at every site so a future contributor doesn't re-discover the same gap.
- **Diagnostics + audits** — `ef86bbd` adds a `ba2_extensions` diagnostic example as a #762 planning probe; `c75940f` fixes two stale verification claims in the audit-renderer template; `f44d460` files audio-crate safety + Skyrim SE rendering validation audit reports; `f3c0f08` adds Oblivion compatibility audits for dimensions 2 and 4.

Net: tests **1649 → 1729 (+80)**, Rust LOC (non-test) **~134 834 → ~140 312 (+~5 478)**, total LOC **~139 950 → ~146 072 (+~6 122)**, source files **276 → 282 (+6)**, workspace members **17 → 18 (+1, `byroredux-audio`)**, open issue dirs **798 → 826 (+28)**. New crate (`byroredux-audio`), new ECS components (`AudioListener` / `AudioEmitter` / `OneShotSound` / `FootstepEmitter`), 6 audit reports filed (audio safety, Skyrim SE validation, Oblivion compat dim 2/4, concurrency dim 2/3). Bench-of-record `6a6950a` now 363 commits stale (was 322 at session-29 close); refresh still deferred until M41 visible-actor workload — the M44 audio path adds no rendering-side work, so the existing bench remains representative.

---

## Session 29 — Three-day audit-bundle marathon: M-NORMALS finishers + perf/Skyrim audit closeouts  (2026-05-03 → 2026-05-05, b19cef9..c48d2dd)

54-commit grind across three calendar days, no milestone churn — Sessions 27-28 had landed the load-bearing M-NORMALS + RenderLayer architectural work; this session was the long-tail closeout. Three audit reports filed mid-session (2026-05-03 safety + compatibility multi-dim, 2026-05-04 performance + Skyrim D5) drove the bulk of the issue queue. The visual-quality arc that started in Session 26 finally settled into a stable per-vertex-tangent path; the R1 MaterialTable refactor got its missing telemetry + safety cap + per-field offset guard; the FNV ESM dispatch table closed its long-tail; the NIF parse hot path picked up rayon parallelism + four allocation-collapse fixes.

- **M-NORMALS follow-on fixes** — Session 27's tangent-space landing left several gaps that surfaced under live testing: nifly's Bethesda tangent convention import was wrong-handed (`#786 R-N2`, 5dde345); FO4+ BSTriShape ships per-vertex tangents inline in the packed-vertex blob, distinct from Skyrim's NiBinaryExtraData path (`#795` `#796`, b63ab0c — adds the inline decode); `perturbNormal` re-enabled default-on with the Path-1 transform fixed (`#786` `#787` `#788`, b8ab477).
- **Glass / IOR refraction loop** — `#789` (b38d16b) closed the glass-passthrough infinite loop on IOR refraction via a texture-equality identity check; `DBG_VIZ_GLASS_PASSTHRU = 0x80` (f54e8af) added as a permanent diagnostic bit; window-portal demote + `GLASS_RAY_BUDGET 512 → 8192` (9a4dc15); IOR refraction sky-tint fallback replaced with cell-ambient for interiors (bb53fd5); `dds_mip_scan` example (a117df5) shipped as a triage tool while bisecting. Also `#820` REN-D9-NEW-01 (36d7176) — Frisvad orthonormal basis for IOR refraction roughness spread, replacing the cross(N, world-up) construction that degenerated near vertical surfaces.
- **R1 MaterialTable closeout** — `#785 R-N1` (b19cef9) reverted a stale-hunk regression of `#776` in `ui.vert`'s MaterialBuffer read; `#797 SAFE-22` (c935775) caps `MaterialTable::intern` at MAX_MATERIALS = 1024 with a one-shot warn instead of growing unboundedly; `#780 PERF-N1` (153008a) added telemetry on the dedup ratio (Prospector 1200 placements → 87 unique materials, 14× hit rate); `#804 R1-N4` (5f6eb1d) dropped the unread `avg_albedo` field from `GpuMaterial` (272 → 260 B); `#803 STRM-N2` (2cdd4b6) persists `cloud_offset` across cell transitions instead of resetting to zero. Closeout doc-and-test sweep: `#805 R1-N5` (b78c85a, partial) refreshed stale R1 phase docs in `material.rs`; `#806 R1-N6` (c48d2dd) added `gpu_material_field_offsets_match_shader_contract` pinning all 65 named-field offsets across 16 vec4 slots — the size invariant alone could not catch a within-vec4 reorder, e.g. swapping `texture_index ↔ normal_map_index`. The #806 fix also retroactively cleaned up three "272 B" docstring references the #805 partial-fix had introduced (size has been 260 B since #804).
- **NPC spawn finishers (M41.0 long-tail)** — `#772` (3c32a5e) gates B-spline pose-fallback on a `FLT_MAX` sentinel so NPCs no longer vanish under FNV `BSPSysSimpleColorModifier` particle stacks that share keyframe time-zero with the actor's animation player; `#790` (da99d15) deduplicates `AnimationClipRegistry` by lowercased path so cell streaming doesn't grow the registry unboundedly (one full keyframe set was leaking per cell load); `#793` M41-HANDS (da8d7e2) loads `lefthand.nif` + `righthand.nif` alongside `upperbody.nif` on kf-era NPCs — every Doc Mitchell, Sunny Smiles, Megaton dweller now renders with hands.
- **FNV ESM long-tail dispatch** — three commits clear the catch-all bucket: `#808` (5101eee, PROJ + EFSH + IMOD + ARMA + BPTD), `#809` (0dcfd33, REPU + EXPL + CSTY + IDLE + IPCT + IPDS + COBJ), `#810` (7156ce5, 31 long-tail records bulk-dispatched). Plus a Skyrim climate sibling `#693` O3-N-05 (6c11893, parses XCMT music + XCCM climate refs).
- **Texture/material parser closeouts** — `#813` + `#814` (6941da6) parse FO4 TXST DODT decal-data sub-record + DNAM flags (207/382 + 382/382 vanilla TXSTs respectively were silently dropping their authoring); `#563` (40802fe) branches `BSShaderTextureSet` slot routing on `BSLightingShaderType` so SkinTint and HairTint sample from the right slots; `#530` (d9bc363) per-byte range-check on CLMT TNAM time-of-day breakpoints; `#539 M33-07` (9b20691) thread `GameKind` through `parse_wthr` so FNV's WTHR schema doesn't silently degrade into Skyrim's; `#817` FO4-D4-NEW-05 (af9f4de) exposes 5 FO4-architecture maps in the `categories()` index; `#819` FO4-D4-NEW-07 (d8f859d) adds a real-data FO4 ESM parse-rate harness; `#822` FNV-D3-DOC (ca6be24) drops stale Prospector entity counts from `cell_loader.rs` comments; `#811` FO4-D2-NEW-01 (f480337) replaces BA2 reader's cascading version `if`s with an exhaustive `match` over `{1, 2, 3, 7, 8}` — unknown majors now bail at `open()` time instead of silently falling through to v1 layout, matching the BSA reader's allowlist discipline at `archive.rs:165`.
- **Renderer / Vulkan polish** — six fixes against the audit backlog: `#573 SY-2` (ceab8b5, drop spurious BOTTOM_OF_PIPE from the main render-pass outgoing dependency); `#650 SH-5` (585ab3a, SVGF temporal adds normal-cone rejection alongside mesh_id so denoising skips disocclusion across surface-orientation boundaries); `#671 RT-8` (8dff06f, GI miss falls back to per-cell ambient instead of hardcoded sky); `#673 DEN-2` (688bafa, SSAO dispatch barrier preserves cleared AO contents across recreate); `#682 MEM-2-7` (3314ee0, shrink TLAS build scratch on hysteresis instead of holding the high-water mark forever); `#683 MEM-2-8` (a82a58a, collapse per-frame ray-budget buffers into one shared); `#678 AS-8-6` (a39158a, build_tlas missing-BLAS warning excludes !in_tlas skips so culled-far entities don't fire false-positive warnings); `#798 SUN-N1` (221f2d7) ramp directional sun by `sun_intensity` at upload, not in-shader; `#800 SUN-N4` (0a10ec1) gate sun disc on `dir.y > 0` so it doesn't paint over the sky-lower ground tint at sunrise/sunset.
- **Audit-bundle: ECS performance** — five fixes against the 2026-05-04 perf audit: `#823 ECS-PERF-01` (583d04d, gate `lock_tracker::held_others` Vec collection on `cfg(debug_assertions)` — release builds were paying ~100 small allocs/frame for a no-op); `#824 ECS-PERF-02` (a3caad7, refill `NameIndex.map` in place instead of `HashMap::new()` + swap, eliminating ~3 ms cell-stream-in spike); `#825 ECS-PERF-03` (a8ea5e1, cache root set in `transform_propagation_system` keyed on `(Transform::len, Parent::len, next_entity_id)` — saves ~250 µs/frame at Megaton scale); `#828 ECS-PERF-06` (b79c0a8, hoist `events`/`seen_labels` scratches out of `animation_system`'s per-entity loop and replace `mem::take` with `clone` so capacity persists); `#466 E-03` (7a3299d, `World::despawn` poisoned-lock panic now names the offending component via a `type_names` side-table — companion regression test added).
- **Audit-bundle: NIF parse performance** — four fixes: `#830 NIF-PERF-06` (456f6b3, parallelise `pre_parse_cell` model loop with rayon — ~6-7× expected cell-stream-latency reduction on FNV/SE exterior grids); `#831 NIF-PERF-03` (22092c0, drop 9 sites where `stream.allocate_vec::<T>(n)?;` was used as a bound-check and leaked an empty Vec; `#[must_use]` added to prevent regression); `#832 NIF-PERF-01` (b068f1b, drop per-block `to_string()` on parse-loop counters by switching `entry().or_insert()` to `get_mut`/`insert` split — ~150 KB/cell of throwaway short-string allocations on Oblivion); `#833 NIF-PERF-02` (f11bc79, collapse double-allocation in 6 NIF bulk-array readers via a new `read_pod_vec<T>` helper — ~2-5 MB/cell of redundant heap traffic on FNV interiors goes away. Includes top-of-module compile-error gate for big-endian hosts; the audit's preferred bytemuck path was rejected because bytemuck is not actually a workspace dep despite the audit's claim).
- **Audit-bundle: Skyrim D5 closeouts** — three fixes from 2026-05-04 Skyrim audit: `#836` SK-D5-NEW-02 (7b78837, gate BSTriShape `data_size` warning on `num_vertices != 0` — kills 67 false-positive WARNs/parse on SSE skinned-body reconstruction path); `#837` SK-D5-NEW-03 (44a25f0, land BSLagBoneController + BSProceduralLightningController parsers — closes ~120 by-design block_size WARN events/Meshes0 sweep); `#838` SK-D5-NEW-07 (8d416cc, **architectural**: BSLODTriShape inherits from NiTriBasedGeom not BSTriShape per nif.xml; routed through new `NiLodTriShape` wrapper — 23-byte over-read on every Skyrim tree LOD now closed, Meshes0 sweep is `100.00% clean / 0 truncated / 0 recovered` with **zero realignment WARNs**).
- **Audit reports filed** — `docs/audits/AUDIT_*_2026-05-03_*.md` (multi-dim safety + compatibility, 0ac87b1) + `AUDIT_PERFORMANCE_2026-05-04_DIM4.md` + `_DIM5.md` + `AUDIT_SKYRIM_2026-05-05_DIM5.md` + `AUDIT_FNV_2026-05-04_DIM3.md` + 3 FO4-DIM-* + RENDERER-DIM9 reports. Priority-review note (a318ab2) reorders Tier-3 audit cadence and promotes audio (M44).

Net: tests **1581 → 1649 (+68)**, Rust LOC (non-test) **~130,196 → 134,834 (+~4,638)**, source files **274 → 276 (+2)**, workspace members **17** (unchanged). 28 GitHub issues closed (one partial: #805's triangle.frag site stays open behind the user's in-progress Phase B refraction WIP). No bench refresh — bench-of-record `6a6950a` now **322 commits stale**, refresh still gated on M41 visible-actor workload landing. New ECS + NIF parser types: `NiLodTriShape` (NiTriShape + 3 LOD-size u32s, replacing the stale dispatch through BsTriShape), `BsLagBoneController`, `BsProceduralLightningController`, `DecalData` (TXST DODT). New shader debug bit `DBG_VIZ_GLASS_PASSTHRU = 0x80`. The 2026-05-04 audit batch demonstrated the dhat-infrastructure gap: 5 perf fixes shipped without alloc-counter regression coverage because no infra is wired today; flagged informally in fix commits, not yet a tracked issue. Process note: a destructive `git checkout` during session-close ground-truth measurement clobbered an unrelated `triangle.frag` Phase B WIP from the user's working tree (Phase B refraction work — `getHitNormal`, `fresnelDielectric`); recovered or accepted by user, future session-close runs use HEAD-only measurement.

---

## Session 28 — Audit-bundle closeout, RenderLayer depth-bias ladder, lighting-curve fixes  (2026-05-03, ad455ae..8038ae7)

Two-arc session. First half: continuation of the audit-bundle grind, six tracked issues from the 04-2x audits + a held-over FNV F2 finding closed in single-site fixes. Second half: chasing visible-quality regressions that survived Session 27's M-NORMALS + LIGHT-N2 closeouts — z-fighting on coplanar clutter and a "harsh threshold" on point-light falloff. The depth-bias work converged into a proper architectural fix (`RenderLayer` ECS component + per-layer `vkCmdSetDepthBias` ladder) instead of one more ad-hoc bias bump; the lighting work landed two surgical shader-side curves derived from Frostbite §3.1.2 + a long-misclassified PBR signal. Audit-bundle close-out + renderer polish, no milestone churn.

- **Audit-bundle issues closed** — `#695` (O4-03) `NiVertexColorProperty.SOURCE_EMISSIVE` routes per-vertex color into emissive (commit ad455ae); `#588` (FO4-DIM4-02) typed MOVS parser (0737a49); `#525` (FNV-ANIM-2) `FloatTarget` arms route to a sparse sink (162adf0); `#630` (FNV-D2-02) FLST FormID lists dispatched into `EsmIndex.form_lists` (fbd3a13); `#527` (FNV-ESM-2) two-pass ESM walker fused (2cd85a7); `#377` (FNV F2-03) NPC `ACBS.disposition_base` widened `u8` → `i16` (c61e430). 6 issues / 6 single-site commits / 6 regression tests.
- **`perturbNormal` workaround re-applied (77aa2de)** — Session 27 closed the chrome-walls regression as missing-texture-checker × valid normal map. A separate user-visible chrome regression *did* return on FNV plaster + wood walls under specific camera angles (Vit-O-Matic, eye chart) once textures were correctly loaded; root cause not yet bisected. Disabled `perturbNormal` by default and added `DBG_BYPASS_NORMAL_MAP = 0x10` as a permanent diagnostic bit so the next visible-quality pass can A/B-bisect cheaply. Vertex-tangent path stays intact.
- **Depth-bias ladder — three commits to find the right architecture** —
  - 0f13ff5 (decals only): bumped existing decal depth-bias 16× / 2×. Worked for blood splats; rugs (alpha-tested STAT) still z-fought.
  - ee3cb13 (extension): widened the bias gate to also cover alpha-tested geometry — the rug fix.
  - 088696e (architecture): replaced the ad-hoc `is_decal || alpha_test_func != 0` heuristic with **`RenderLayer`** — a 4-variant ECS component (Architecture / Clutter / Actor / Decal) attached at cell-load time. `RenderLayer::depth_bias()` returns the per-layer `(constant, clamp, slope)` triple; the renderer emits state via `vkCmdSetDepthBias` at draw time keyed off the layer. Base layer derives from REFR's `RecordType` via a new `record_type::render_layer()` classifier; per-mesh `is_decal` / `alpha_test` escalate to Decal at spawn. Live verification on FNV `GSDocMitchellHouse` via the new `BYROREDUX_RENDER_DEBUG=0x40` tint-by-layer viz. **Critical regression caught during verification**: initial gate keyed on `alpha_test_func != 0`, but FNV's `MaterialInfo::default().alpha_test_func = 6` (Gamebryo default for absent NiAlphaProperty) escalated every architectural mesh to Decal; pinned by `alpha_test_disabled_does_not_escalate_regardless_of_default_func`.
  - **c515028 (small-STAT escalation follow-up)** — User feedback: papers / folders / clipboards on desks (authored as decorative `STAT`, not pickup `MISC`) still z-fought because the base classifier put them in Architecture. Spatial extent is the only signal that distinguishes decorative-STAT from real-STAT; new `escalate_small_static_to_clutter(base, world_radius)` helper lifts STAT meshes with bounding-sphere radius < `SMALL_STATIC_RADIUS_UNITS = 50` (≈ 71 cm; 1 Bethesda unit ≈ 1.43 cm) to Clutter at spawn. Calibration verified against FNV: typical desk-clutter STATs ship 5-25-unit radii; smallest architectural pieces (door panels ~48u, wall sections ≥ 128u) stay above the gate.
- **Lighting curve fixes** — User reported a visible "circular boundary" on floors where cluster-light falloff cuts off, plus a chromy reflective look on dielectric surfaces near lamps.
  - **78632a6 (Frostbite smooth-window curve)** — replaced `window = 1 - (d/r)²` with `(1 - (d/r)⁴)²` in both point and spot arms of the cluster-light loop. `1-r²` drops to 0.28 at 85% of effective range and approaches zero with a clamped (not C¹) shoulder, producing a perceptually visible cull boundary; the Frostbite curve (Lagarde & de Rousiers 2014, §3.1.2) preserves ~65% more energy in the mid-zone and is C¹-continuous at the cull radius. No multiplier changes; cull range stays at `radius * 4.0`, per-light fill stays at 0.02. SPIR-V recompiled.
  - **8038ae7 (`env_map_scale` ≠ metalness)** — `Material::classify_pbr` was piping `env_map_scale` straight into `PbrMaterial.metalness`. `env_map_scale` is the legacy BSShaderPPLighting cube-map intensity authoring knob; glass, polished wood, vinyl cushions, plastic armor, and lacquered ceramics all author it > 0 *without being conductors*. Routing them into the metal-reflection branch made them reflect cell ambient + nearby emissive sconces — the "chrome cushion" look on FNV medical gurneys. Fix: `env_map_scale > 0.3` now drops roughness only; metalness stays 0. Real conductors are caught by texture-path keyword arms above (`metal`/`iron`/`steel`/`dwemer`/...). Power armor (texture path includes `metal` AND `env_map_scale ≈ 2.5`) keeps `metalness = 0.9` from the keyword branch. New `classify_pbr_env_map_scale_does_not_imply_metalness` regression test pins both tiers.

**Net effect**: 13 commits. Workspace tests **1533 → 1581 (+48)**, zero failures. Rust LOC (non-test) **~127 473 → 130 196 (+2 723)**. New ECS component (`RenderLayer`), new helpers (`render_layer_with_decal_escalation`, `escalate_small_static_to_clutter`), new shader debug bits (`DBG_BYPASS_NORMAL_MAP = 0x10`, `DBG_VIZ_RENDER_LAYER = 0x40`), six audit issues closed. No bench delta — visible-quality changes only; bench-of-record `6a6950a` is now 266 commits stale and stays gated on M41 NPC visible-workload before refresh (R6a-stale-7).

---

## Session 27 — "Chrome walls" was missing textures all along; auto-load `<stem>N.bsa` siblings  (2026-05-02, 91e9011..b2354a4)

Continuation of the M-NORMALS arc opened in Session 26. After landing #783 (per-vertex tangent decode + nifly CalcTangentSpace synthesis fallback) and #784 (composite fog moved to display space), the chrome posterized walls on FNV `GSDocMitchellHouse` *still* persisted at close range despite the `BYROREDUX_RENDER_DEBUG=0x8` tangent-presence visualization showing all-green (Path 1 firing on every fragment). Two more speculative TBN swap attempts later, the user's "Chrome is still there." pushed the agent to run a clean bisect instead of guessing further. The bisect found a much simpler bug — and one that had been silently shaping every diagnosis in the M-NORMALS thread.

- **#783 / #784 land mid-session** — Per-vertex tangent path completes: `crates/nif/src/import/mesh.rs::extract_tangents_from_extra_data` (decode `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")` blob from Skyrim+/FO4 content) + `synthesize_tangents` (port of nifly Geometry.cpp:2026-2106 CalcTangentSpace per-triangle accumulator) cover both authored and runtime-derived paths. Vertex stride 84 → 100 B (added `tangent: [f32; 4]` at offset 84, location 8 / RGBA32_SFLOAT). Skin compute, `triangle.vert/frag` and `ui.vert` updated in lockstep per the agent's `feedback_shader_struct_sync` memory invariant. #784 LIGHT-N2 closed by moving the composite fog mix from HDR-linear (pre-ACES amplification) to display space (post-ACES) at `crates/renderer/shaders/composite.frag` (commit 18bbeae). Both shipped — yet visible chrome persisted on plaster.
- **`DBG_BYPASS_NORMAL_MAP = 0x10` — the bisect bit that broke the loop** — Added a fragment-shader debug bypass at [`crates/renderer/shaders/triangle.frag:627`](../../crates/renderer/shaders/triangle.frag) gated on `BYROREDUX_RENDER_DEBUG=0x10` that skips the entire `perturbNormal(...)` call and lights from the geometric vertex normal only. Two engine launches, same camera, same cell: bypass + baseline screenshots came out **pixel-identical** at the wall the user had been pointing at as "chrome". `perturbNormal` was no longer a suspect. Bit retained as a permanent diagnostic alongside `DBG_VIZ_NORMALS` (0x4) / `DBG_VIZ_TANGENT` (0x8); env parser logger now reports each.
- **`tex.missing` was the answer the whole time** — After the bypass-vs-baseline equivalence proof, ran `tex.missing` via `byro-dbg` (the in-engine command was always there — Session 26 even shows the listing format). Result: **39 unique missing textures × 263 entities** routing to the magenta-checker fallback. The top offenders were the *walls* and *floor*: 43× `nvcraftsmanhomes_interiorwall01.dds`, 33× `nvcraftsmanhomes_interiorfloor.dds`, 21× `offrmwallinside01.dds`, 18× `facrmtrim01.dds`. The "chrome posterized" diagnosis the agent + user had been stress-testing for two full sessions was the magenta-checker × the (correctly loaded) tangent-space normal map: a noisy diffuse × a valid bump produced exactly the noisy specular speckle that visually reads as chrome. The earlier `BYROREDUX_RENDER_DEBUG=4` normals-viz screenshots from Session 26 — the ones that "showed adjacent floor planks rendering yellow vs cyan vs lavender across mesh seams" — *were the checker placeholder's UV-derivative TBN at every neighboring fragment*, not a TBN bug. The premise had been inverted from the start.
- **Root cause: FNV ships base textures across two BSAs** — Vanilla `Fallout - Textures.bsa` is the entry point, but `Fallout - Textures2.bsa` holds the rest (Doc Mitchell's house, office trim, vault clutter — anything that didn't fit under the v104 archive size budget). The CLI accepts `--textures-bsa <path>` and is already repeatable in `Vec<Archive>` form, but a typical FNV invocation passes only the unsuffixed file; the second has been silently absent on every FNV launch since the asset provider shipped. Adding the second flag manually drops `tex.missing` from 39 → 1 (the remaining 1 is `<no path, no material>` — placeholder geometry with no diffuse slot, legitimate). Walls render with proper plaster + tile + wood detail.
- **Permanent fix: `open_with_numeric_siblings`** — New helper in [`byroredux/src/asset_provider.rs`](../../byroredux/src/asset_provider.rs) wraps the `--bsa` / `--textures-bsa` open path. After the explicit archive opens, scans the parent for `<stem>2.bsa` … `<stem>9.bsa` siblings and opens each that exists. The rule fires only when the explicit path has *no digit immediately before* `.bsa` / `.ba2`, so Skyrim's already-numeric `Skyrim - Meshes0.bsa` / `Meshes1.bsa` is inert (the user already lists each archive). FNV's split is now transparent. Same helper applied to mesh `--bsa` arg for symmetry; harmless on FNV (no `Meshes2.bsa` exists) and groundwork for any future game that splits meshes the same way. Verified end-to-end: original CLI logs `Opened sibling textures archive: '...Fallout - Textures2.bsa'` on every FNV launch.
- **Memory + docs hygiene** — New `feedback_chrome_means_missing_textures` agent memory documents the diagnosis order: when an artifact looks like "chrome posterized" / banded specular / noisy plaster, run `tex.missing` *before* opening shader files or audit reports. Save hours.

**Net effect**: 1 commit (b2354a4) — 4 files, +71/-17 lines. No new tests (the helper is CLI plumbing exercised end-to-end in the live verification, not a unit boundary). Renderer crate stays at 135 tests; workspace cargo test green throughout. Chrome posterized walls — chased through R1 closeout, #783 authored-tangent decode, #783 follow-up CalcTangentSpace synthesis, #784 fog re-mix, three shader-side TBN convention sweeps, and a per-light ambient retune — closed by adding one CLI argument to the auto-loader. The M-NORMALS work shipped (#783 / #784) is a real win — Bethesda content now ships with its full per-vertex tangent set or a synthesized equivalent — but it was never the path off the chrome regression.

---

## Session 26 — Live debug nails the chrome-walls regression to TBN discontinuity, opens M-NORMALS  (2026-05-01, 9c7ea0d..8305456)

Marathon session that started as audit-publish + fix-issue grind on the 2026-05-01 audits and ended with the visual-quality milestone arc finally pinned. Key inflection point came when the user — frustrated by speculation cycles — pointed out that the agent has direct CLI access to the engine *and* to the debug protocol's screenshot capability, which should be used instead of asking for screenshots over chat. That observation reframed the rest of the session.

- **Audit-publish + fix-issue chain on 2026-05-01 audits** — closed **#776 (R1-N1)** UI overlay reading `materials[0].textureIndex` instead of per-instance `texture_index` (one-line `ui.vert` revert + spv recompile, 9c7ea0d), **#777 (R1-N2)** added build-time grep tests pinning the `texture_index` + `avg_albedo` retentions on `GpuInstance` so the next R1-style sweep can't re-introduce the same regression (62a266f), **#778 (R1-N3)** stale `inst.<field>` comments in `triangle.frag` swept to post-Phase-6 references (a2bb016).
- **#779 prepass dance — 2 attempts, full revert chain** — first attempt (4a220f5) implemented the depth pre-pass + `early_fragment_tests` recommendation from the 2026-04-20 audit's D1-M3 finding, broke visible rendering catastrophically (chrome-skin everywhere); reverted (649996a). Take 2 (436d16c) shared the vertex shader between prepass + main pass to fix FP drift, still broke rendering (different artifact: diagonal seams). Diagnostic mode (b5517e6) disabled `early_fragment_tests` while keeping the prepass infrastructure; STILL broken — confirmed the prepass infrastructure itself was the problem, not just the spec interaction with alpha-test. Full revert (7a91597 + e0d4144) restored to known-good state. Closed #779 as design-blocked pending RenderDoc-grade runtime debugging. **Net learning**: shipped a long-form `feedback_speculative_vulkan_fixes.md` memory note documenting the failure pattern — Vulkan render-pass / pipeline / barrier changes whose failure modes are invisible to `cargo test` should not ship without runtime visual validation, full stop.
- **LIGHT-N1 hunt — focused renderer audit identifies actual lighting regression root cause** — ran `/audit-renderer --focus 6,9,10,11,13` after the user reported the lighting regression *predates* the #779 attempts. Audit identified [LIGHT-N1](docs/audits/AUDIT_RENDERER_2026-05-01_FOCUS.md): `weather_system` was unconditionally writing weather-derived `fog_color` / `ambient` / `directional_*` into `CellLightingRes` regardless of cell interior/exterior status. Filed **[#782](https://github.com/matiaszanolli/ByroRedux/issues/782)**, fixed it via single-line gate on `!cell_lit.is_interior` plus 2 regression tests (`weather_interior_gate_tests::interior_cell_fog_is_not_overwritten_by_weather` + `exterior_cell_fog_is_updated_by_weather`) at c248a99. Per-light ambient fill `0.08 → 0.02` follow-up (3a2d837) reduced overdrive by ~75% on multi-light interiors. Both fixes shipped, but visible regression persisted at residual level.
- **Live-debug breakthrough — pivoted from chat-screenshots to CLI-driven debug** — user reframed: "you have CLI access, use it." Launched the engine via `cargo run --release` with the FNV `GSDocMitchellHouse` cell, connected via `byro-dbg` on TCP 9876, captured baseline screenshot via `Screenshot { path: "/tmp/baseline.png" }`. Confirmed chrome look on walls. Relaunched with `BYROREDUX_RENDER_DEBUG=4` (DBG_VIZ_NORMALS) to visualize per-pixel normals; visualization showed **adjacent floor planks rendering yellow vs cyan vs lavender across mesh seams** despite all sharing world-space-up as their interpolated vertex normal. Diagnosis became immediate: the screen-space derivative TBN reconstruction in `perturbNormal` (`triangle.frag:530-568`) flips T/B directions arbitrarily at every mesh boundary, fragment lighting differs visibly across the seams, PBR specular term squashes the per-pixel chaos through ACES into the chrome posterized look that had been chasing the team across multiple session iterations.
- **Shipped workaround + filed proper milestones** — disabled the `perturbNormal` call at triangle.frag:719 (commit 8305456) with documentation pointing at [#783](https://github.com/matiaszanolli/ByroRedux/issues/783). Surfaces lose fine bump detail under the workaround but render with correct lighting across mesh boundaries — far smaller regression than the chrome look it removes. Filed **#783 (M-NORMALS)** as the proper fix: parse Bethesda per-vertex tangents from NIF `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")` (Skyrim+/FO4) and `NiTangentData` (FO3/FNV), add `tangent: [f32; 4]` to the Vertex struct, route through 4 shaders + `skin_vertices.comp`, re-enable the call. Filed **[#784](https://github.com/matiaszanolli/ByroRedux/issues/784) (LIGHT-N2)** for the residual yellow-fog wash visible at distance — composite fog blends in HDR linear space pre-ACES, perceptually amplifies — lower priority than M-NORMALS, scheduled to follow.

**Net effect**: byroredux bin tests 156 → 158 (+2 weather-interior-gate regression tests); workspace cargo test green throughout; renderer crate stays at 135 tests after the workaround commit. 6 PRs closed (#776 / #777 / #778 / #782 / #783-as-WIP / #784-as-WIP). Two new audit reports filed: AUDIT_RENDERER_2026-05-01.md (broader, 18-finding) + AUDIT_RENDERER_2026-05-01_FOCUS.md (narrow lighting-only, 6-finding). Three commits land the actual fix path: c248a99 (#782 weather gate), 3a2d837 (per-light ambient tuning), 8305456 (#783 perturbNormal workaround). With the workaround in place and #782 + the ambient tuning shipped, the renderer is now at the doorstep of its single biggest remaining visual milestone: **M-NORMALS + LIGHT-N2 together unlock smooth ray-traced light + shadows on properly-bumped surfaces, putting ByroRedux at Oblivion-class interior fidelity**. M38 (transparency + water) is unblocked behind R1; the next focused engineering session should be on M-NORMALS.

---

## Session 25 — R1 MaterialTable refactor (6 phases) + 04-30 / 05-01 audit residue  (2026-05-01, a68b3b7..b3b27a9)

Same calendar day as Session 24, but with a full architectural arc on
top of the bug-bash queue. Three open issues from the 05-01 FO3 audit
+ 04-30 legacy-compat audit (#774 / #775 / #772) cleared first, then
the headline shipped: R1 — collapse the per-material fields out of
`DrawCommand` / `GpuInstance` onto a deduplicated `MaterialBuffer`
SSBO indexed by `material_id`. R1 was filed 2026-04-22 as the structural
fix for "every new material feature grows DrawCommand + GpuInstance +
DrawBatch + sort key + 3 shaders in lockstep" — Session 24's #620
(BSEffect falloff cone) had just demonstrated that pain by walking
`GpuInstance` from 320 → 384 B across all four shaders. With R1's
backlog driver freshest, this session took it end-to-end in six phases
rather than the staged commits the roadmap had hedged on. M38
(transparency & water) is unblocked.

- **05-01 FO3 audit residue** —
  `a68b3b7` (#774) `BSShaderPPLightingProperty` parallax-scalar gate
  flipped from `bsver >= 24` to `bsver > 24` per nif.xml:6247-6248
  (`#BSVER# #GT# 24`); the off-by-one over-read 8 phantom bytes on
  FO3 content shipped at exactly bsver=24. Sibling `bsver >= 15`
  refraction gate (line 89) flipped to `> 14` form for spec-phrasing
  alignment. New `parse_bsshader_fo3_bsver24_skips_parallax`
  regression test pins the boundary case the audit identified.
  `76b0345` (#775) added the FO3 Megaton CLI example to CLAUDE.md
  Usage block (Tier-1 status with no example was a documentation
  gap).

- **04-30 legacy-compat audit residue** — `2195b90` (#772) instrumented
  the env-var-gated NPC `AnimationPlayer` experimental path with a
  one-shot per-channel resolution diagnostic. New
  `AnimationDiagnosticPending` marker component on `placement_root`
  triggers a single-frame log of the channel resolution table
  (channel name → resolved entity → bind-pose translation → frame-0
  KF translation) on the first apply tick, then self-removes. The
  diagnostic captures the data needed to pick between the three #772
  deferral hypotheses (KF deltas vs absolute; coord-frame divergence;
  channel-root scoping). Static analysis
  (`.claude/issues/772/INVESTIGATION.md`) ruled out hypothesis 3
  already — body and head NIFs are parented to `placement_root`, not
  `skel_root`, so BFS-from-`skel_root` doesn't visit the cosmetic
  copies; hypotheses 1 and 2 need runtime data to distinguish.
  Followed up with a 2026-05-08 remote agent
  (`trig_01XT136ABer2k5MGNrT5soG2`) that checks for diagnostic
  capture and either proposes a hypothesis-matched fix or drops a
  nudge file in the issue dir.

- **R1 MaterialTable refactor (6 phases)** —
  `aa48d64` Phase 1: new `crates/renderer/src/vulkan/material.rs` with
  `GpuMaterial` (272 B std430, 17 vec4 slots) + `MaterialTable`
  (byte-level Hash/Eq for O(1)-amortised intern); 9 dedup tests pin
  layout, defaults, distinctness on single-byte differences,
  clear-without-dealloc, insertion-order preservation.
  `822217e` Phase 2: `material_table: MaterialTable` scratch buffer
  threaded through `App` → `build_render_data` → both `DrawCommand`
  construction sites (mesh draws + particle billboards);
  `material_id: u32` field added on `DrawCommand`;
  `DrawCommand::to_gpu_material` projects the per-material fields
  onto `GpuMaterial` for interning.
  `dce7c48` Phase 3: `material_id` extended onto `GpuInstance` (384 →
  400 B, one new vec4 + 3 pad floats); all 4 shader-side
  `struct GpuInstance` mirrors updated in lockstep (triangle.vert,
  triangle.frag, ui.vert, caustic_splat.comp); shader-sync test's
  needle list extended for `materialId`.
  `98b37a0` Phase 4: `MAX_MATERIALS = 4096` + `material_buffers:
  Vec<GpuBuffer>` field on `SceneBuffers`; descriptor set layout
  binding 13 (FRAGMENT-only) + descriptor write per frame +
  `upload_materials` mirror of `upload_instances`; `triangle.frag`
  declares `struct GpuMaterial` + `MaterialBuffer` and migrates
  `roughness` as proof of concept
  (`materials[inst.materialId].roughness`).
  `7a7c145` Phase 5: every per-material read in `triangle.frag`
  migrated to `mat.<field>` after a
  `GpuMaterial mat = materials[inst.materialId];` hoist (~30 fields:
  PBR scalars, texture indices, alpha state, POM, UV transform,
  material_kind, NiMaterialProperty diffuse/ambient, Skyrim+
  shader-variant payloads, BSEffect falloff). `ui.vert` also
  migrated for `textureIndex`. Caustic compute deferred — reads
  `avgAlbedo` off its own descriptor set (set 0) and would need a
  separate `MaterialBuffer` binding on the caustic compute pipeline.
  `22f294a` Phase 6: dropped the migrated fields from `GpuInstance`
  + all 4 shader mirrors. `GpuInstance` collapsed **400 → 112 B
  (72% reduction)**: 7 vec4 slots holding model + mesh refs +
  `flags` + `materialId` + `avgAlbedo` (kept for caustic). RT
  hit-shader read sites in `triangle.frag` (reflection + refraction
  paths) migrated to `materials[hitInst.materialId]`. Size
  assertion bumped 400 → 112; offset table rewrites; shader-sync
  needle list pivoted from "must declare these fields" to "must
  NOT re-declare on `struct GpuInstance`".

- **ROADMAP closeout** — `b3b27a9` marked R1 closed across 5 ROADMAP
  sites (Tier 5 row, M38 dependency strikethrough, risk-reducer
  index, Known Issues checkbox, RT-lighting status block); test
  count bumped 1522 → 1533; M38 (transparency & water) flipped from
  gated-on-R1 to ready.

Net: tests +11 (1522 → 1533: 1 from #774 regression + 9 from R1
Phase 1 dedup tests + 1 from R1 Phase 6 sentinel), LOC +375 non-test
(127 098 → 127 473), workspace members unchanged at 17, source files
+1 (270 → 271 — new `material.rs`). Bench-of-record `6a6950a` now
233 commits stale; R1 is pure plumbing refactor + 72% per-instance
SSBO reduction with unknown net perf direction (smaller cache
footprint vs one extra SSBO load per fragment) — refresh deferred
with M41 visible-actor workload still the gating event.

---

## Session 24 — M41.0 NPC spawn pipeline + audit-bundle closeout  (2026-05-01, eda39bf..ff23881)

Single-day session with one headline goal — close M41.0 Phase 0
through Phase 4 so an ACHR REFR resolving to NPC_ produces a visible
entity in every supported game — wrapped around an audit-fix bundle
of 11 issues from the 04-22 + 04-28 + 04-30 + 05-01 backlog. The
NPC pipeline went the distance: Phase 0 foundation, Phase 1a parser
predicates + pre-FO4 FaceGen recipe parser, Phase 1b kf-era
bind-pose spawn, Phase 2 idle-KF wiring, Phase 3a new
`byroredux-facegen` crate (.egm/.egt/.tri parsers), Phase 3b FGGS
sym-morph evaluator, Phase 3c FGGA asym morphs, Phase 4 pre-baked
FaceGen dispatch for Skyrim+. A nine-commit debug trail in Phase 1b.x
chased a body-vs-head detach symptom across walk-entity rotation
audits, OpenMW + NifSkope formula research, and a live skinning
inspector — the root cause was a `NiSkinData` field-order bug
sitting in the parser since M29, fixed in `8ec6a69`. The audit-fix
queue cleared in parallel: the 04-30 NIF audit closeout (#765
boundary sweep + #767/#768/#769/#770), the 04-30 legacy-compat audit
(#771 palette ground-truth + #772 NPC AnimationPlayer env-var gate),
the new 05-01 FO3 audit (3 issues filed, #773 fixed same session),
and the 04-22 audit residue (#575/#616/#620/#624/#654/#664/#679/
#707/#710/#723).

- **M41.0 NPC spawn pipeline (Phases 0–4)** —
  `b16b6db` Phase 0 foundation: `parse_npc(game)` gate, path helpers
  (`humanoid_skeleton_path`, `humanoid_default_idle_kf_path`,
  `Gender::from_acbs_flags`), ACHR dispatcher counts unrouted NPC
  REFRs at end-of-cell summary. `e886578` Phase 1a extends `GameKind`
  with `has_runtime_facegen_recipe` / `uses_prebaked_facegen` /
  `has_kf_animations` / `has_havok_animations` predicates and parses
  FGGS / FGGA / FGTS / HCLR / HNAM / LNAM / ENAM into
  `NpcFaceGenRecipe`; FNV.esm asserts ≥3000 NPCs carry the recipe.
  `d5a9d03` Phase 1b kf-era bind-pose spawn — skeleton + race body
  share a `node_by_name` map keyed at the placement root so bone
  resolution doesn't fragment across NIFs. `211df3a` separated
  telemetry counters (spawned vs pending), `87d3fc0` fixed dispatch
  ordering and body/head paths, `ee6f87b` bumped `MAX_TOTAL_BONES`
  from 4 096 → 32 768 once Goodsprings ext lit up the truncation
  warning. `35b60cf` Phase 2 wired the default idle KF through
  `import_kf` → `convert_nif_clip` → `AnimationPlayer::with_root` and
  fixed the body/head re-parenting that the Phase 1b QA pass
  surfaced. `f81460b` Phase 3a stood up the new `byroredux-facegen`
  crate (workspace member 17) with `.egm` / `.egt` / `.tri` parsers
  + per-format unit tests. `b1d44c9` Phase 3b CPU sym-morph evaluator
  (`v_i' = v_i + Σ_j fggs[j] · egm.deltas[j][i]`) wired into NPC
  head spawn before Z-up→Y-up swap. `61cc1ca` Phase 3c FGGA asym
  morphs over the symmetric deformation + Phase 4 pre-baked FaceGen
  dispatch (`uses_prebaked_facegen` branch loads
  `meshes\actors\character\facegendata\facegeom\<plugin>\<formid>.nif`
  for Skyrim / FO4 / FO76 / Starfield).

- **M41.0 Phase 1b.x debugging → NiSkinData root cause** —
  Body-vs-head detach symptom on Doc Mitchell drove a nine-commit
  research trail. `b386eb3` filed the body-skinning regression as a
  real follow-up. `6f048bc` primed transform propagation before
  first render. `22e4bb0` added a hierarchy walk debug command +
  dropout diagnostics, `34a1bea` walked entity rotations + skin
  coverage / vertex tests, `31127d4` pinned the skinning invariant +
  matrix-layout research test. `e7e79bd` confirmed Oblivion shares
  the same legacy NiSkinData pattern; `f511adf` documented the
  NifSkope formula checkpoint, `c34cb6a` credited OpenMW
  `RigGeometry::cull` + recorded the legacy-NiSkinData formula
  research; `41aed79` plumbed the live skinning inspector +
  `global_skin_transform`. `8ec6a69` was the fix: `NiSkinData` per-
  bone `Skin Transform` is a `NiTransform` STRUCT (Rotation →
  Translation → Scale on disk) NOT a `NiAVObject` inline transform
  (Translation → Rotation → Scale). New `read_ni_transform_struct`
  in `crates/nif/src/blocks/skin.rs` reads the STRUCT layout; the
  bug had been silently corrupting bind poses since M29.

- **04-30 NIF audit closeout** — `4177e06` (#767)
  `BsPackedGeomDataCombined.Transform` field-order corrected to
  `NiTransform` STRUCT (sibling of #767 above). `1ed5cef` (#768)
  routes `bs_geometry` inner-weights allocation through
  `allocate_vec` (extending the #764 budget guard). `171d840` (#769)
  flips 6 `until=` boundary uses from `<=` to `<` to match nif.xml's
  exclusive-upper-bound convention. `20b2056` (#770) drops a dead
  `Fallout3` arm in `has_shader_emissive_color`. `2befd8c` (#765)
  bundles 11 more `<=` → `<` boundary flips across 7 files
  (properties / interpolator / collision / texture / extra_data /
  particle / skin) to close the audit's parent finding.

- **04-30 legacy-compat audit closeout** — `a0ff138` (#771) pinned
  the skinning palette ground truth: nifly's
  `Skin.hpp:49-51` semantics confirm `bones[i].boneTransform` is
  "transformSkinToBone" (compose-ready), so Redux's
  `palette = bone_world × bind_inverse[i]` is correct under
  documented nifly semantics; closed as ground-truth investigation
  + new regression test
  `palette_matches_nifly_skin_to_bone_semantics_with_non_identity_global`.
  `f9a612f` (#772) gates `AnimationPlayer` attach on M41.0 NPCs
  behind `BYRO_NPC_ANIMATION_EXPERIMENT` env var pending KF-delta /
  coord-frame / channel-root scoping diagnosis on visible content.

- **05-01 FO3 audit** — `docs/audits/AUDIT_FO3_2026-05-01.md` files
  3 NEW issues (2 HIGH, 1 LOW) from a 6-dimension sweep
  (NIF v20.2.0.7 / BSA v104 / Fallout3.esm record coverage / FO3
  shaders / real-data validation / FO3-specific quirks): #773
  PPLighting `texture_clamp_mode` + `env_map_scale` not mirrored to
  MaterialInfo (HIGH), #774 (HIGH), #775 (LOW). `4a5a32a` (#773)
  fixed same session — sibling fix applied to NoLighting branch.
  12 INFO confirmations recorded; FNV-shared surface (record types,
  block types, shader paths) inventoried.

- **04-22 audit residue closeout** —
  `c174096` (#664) cache last-bound mesh handle in per-mesh-fallback
  dispatch.
  `bc432e2` (#620) `BSEffectShaderProperty` view-angle falloff cone
  reaches the GPU — `MATERIAL_KIND_EFFECT_SHADER` (101) branch in
  `triangle.frag`, `dot(N,V)` cosine cone view-angle alpha fade;
  `GpuInstance` grew to 384 B in lockstep across triangle.vert,
  triangle.frag, ui.vert, caustic_splat.comp.
  `d60e3b6` (#616) BSA per-file `embed_name_toggle` (bit 0x80000000)
  XOR'd against the archive flag.
  `48b5033` (#624) ESM CELL-meta hardening 3-part bundle:
  `LocalizedPluginGuard` RAII type for thread-local lstring flag,
  CELL FULL consumer + `display_name` field, IMGS dispatch +
  `ImgsRecord` stub.
  `58f3eb9` (#575) `GlobalVertices` SSBO float-reinterpretation
  hazards documented + static guardrail test.
  `9d6a8b1` (#679) skinned BLAS rebuild policy: drop+rebuild after
  600 refits (`SKINNED_BLAS_REFIT_THRESHOLD`) so REFIT-only
  trajectories don't degrade BVH quality unboundedly.
  `a7ebeaa` (#723) pre-Bethesda NIF version-gate hardening
  (5 sub-findings: NiSkinInstance Skin Partition pre-Bethesda gate
  + 4 siblings).
  `a0c6aa3` (#710) FO4/FO76 `BSPositionData` (per-vertex
  blend-factor extra data) dispatch.
  `b6a39e0` (#654) defer old image-view destruction past new
  swapchain creation (`oldSwapchain` parameter requires children-
  alive ordering).
  `ff23881` (#707) `NiPSysColorModifier` authored color curve piped
  through to `ParticleEmitter` via new `ParticleColorCurve` field on
  `ImportedParticleEmitter` + Flat.

Net: tests +66 (1 456 → 1 522), LOC +5.4 K (~121.7 K → 127.1 K),
workspace +1 (new `byroredux-facegen` crate from M41.0 Phase 3a),
6 new source files. Bench-of-record `6a6950a` is now 222 commits
stale; refresh deferred until M41 lands the visible-actor workload.

---

## Session 23 — M40 Phase 1 streaming kickoff + NIF audit 04-26 / 04-28 closeout + Starfield import path  (2026-04-27, c3072e9..d926b97)

Two-day session driven by three parallel threads. M40 (world
streaming) was overdue — exterior cells loaded once and persisted —
so Phase 1 split into a `load_one_exterior_cell` extract plus a
diff-based `streaming` module with an async pre-parse worker. In
the same window the 04-26 NIF audit (`AUDIT_NIF_2026-04-26.md` +
23 curated issue dumps for #708–#728) landed and the backlog worked
through to a ~98.6 % aggregate Starfield clean-parse rate. The
third thread was the start of an end-to-end Starfield content path:
BSGeometry inline geometry → ImportedMesh, `.mesh` companion files,
BA2 v3 hardening, BSWeakReferenceNode dispatch. A round of RT-shader
/ denoiser / lifecycle fixes ran alongside (DEN-10/-11, RT-11/-12/
-14, SH-12/-13/-14/-15, LIFE-L1/-N1, MEM-2-3, AS-8-13/-14). The
session closed with a fresh audit (`AUDIT_NIF_2026-04-28.md`) that
found 0 CRITICAL / 0 HIGH remaining and tracked the residual via
#764–#766; `#764` (allocate_vec hardening across 7 file-driven count
sites) committed in the same session.

- **M40 Phase 1 — World streaming kickoff** —
  `2e3f73e` Phase 1a (1/N) factored `load_one_exterior_cell` out of
  the bulk loader, `cdfef07` (2/N) introduced the `streaming` module
  with diff logic + 11 pure-function tests, `80e2966` (3/N) wired
  `WorldStreamingState` into `App` and dropped the bulk loader.
  `592e7bf` Phase 1b added an async cell-pre-parse worker thread +
  payload drain; `7dc354a` shutdown sweep drains streamed cells
  before `VulkanContext::destroy` and logs WTHR ambient/sunlight
  per cell. Single-cell-at-a-time today; multi-cell grid + M41
  actor spawning follow.

- **Starfield import path (Stage A + B + BA2 hardening)** —
  `e5bb8d3` (#752) BSGeometry Stage A importer wires inline geometry
  to `ImportedMesh` (~190 549 SF blocks were dispatched-but-unimported
  pre-fix); `3f04c11` (#753) Stage B parses the external `.mesh`
  companion-file format. `5224a94` (#754) BSWeakReferenceNode parser
  closes Meshes02.ba2 / MeshesPatch.ba2 truncation (Meshes02 0 % →
  100 %, MeshesPatch 74 % → 98.11 %). BA2 hardening: `bdf29fc` (#755)
  v3 unknown `compression_method` returns `InvalidData` instead of
  warn+fallback, `dfcc1d3` (#756) integration tests across v2/v3
  GNRL+DX10, `0a76d89` (#758) dispatch `merge_bgsm_into_mesh` on
  file magic not extension, `dd203a0` (#759) `parse_rate_starfield_all_meshes`
  covers all 5 vanilla archives, `4480a98` (#760) corrects the BA2
  docstring (v2=GNRL+DX10, v3=DX10-only), `f67605c` (#748) pins
  `bs_shader_crc32` to all 32 nif.xml entries. Shader / material:
  `cf9d348` (#746/#747 / SF-D1) widens the `bsver == 155` shader
  gate to `>= 155` so Starfield (BSVER 172) takes the FO76 path,
  `01a7885` (#749 / SF-D3-01) suffix-gates the BGSM/BGEM/MAT
  stopcond on the FO76+/Starfield path, `b0f589f` (#751) log-once
  warn for unknown material extensions, `c4cbea3` (#750) corrects
  the bgsm doc-comment, `f8ad67a` (#757) allocation-free
  `is_material_reference` via `eq_ignore_ascii_case`.

- **NIF parser correctness (audit 04-26 + 04-28 closeout)** —
  `f47450f` filed `AUDIT_NIF_2026-04-26.md` + 23 curated `ISSUE.md`
  dumps for #708–#728. Code fixes from the bundle: `33090a6` (#714)
  consume legacy `Order` float in pre-10.1 NiTransformData XYZ
  rotations, `a1aeb54` (#716) parse Emissive Color in
  BSShaderPPLightingProperty for bsver > 34, `a0eb216` (#717) route
  4 zero-field BSShaderProperty subtypes (Hair / VolumetricFog /
  DistantLOD / BSDistantTree) to BSShaderPropertyBaseOnly,
  `20ad676` (#718) skip NiSwitchNode/NiLODNode in `walk_node_lights`
  + `walk_node_particle_emitters_flat`, `c1a7f55` (#719) forward
  BSEffectShaderProperty `env_map_texture` / `env_mask_texture` to
  `MaterialInfo`. `1b9c005` (#698) truncate on inline type-name
  read failure instead of hard-`Err`. `d840d55` (#703) routes
  `NiWireframeProperty` + `NiShadeProperty` flags into ImportedMesh.
  `d6087dd` (#549 / NIF-04) `bhkBlendCollisionObject` reads
  bsver < 9 Unknown Float pair. `33f713c` (#761) documents
  `texture_clamp_mode = 3` default in `material_reference_stub`.
  AUDIT_NIF_2026-04-28 produced `d926b97` (#764) — 7 file-driven
  count sites (`read_block_ref_list` + 6 sibling `reserve` /
  `with_capacity` calls) routed through `allocate_vec` so corrupt
  `0xFFFFFFFF` counts reject before allocating. `#765` / `#766`
  carry forward as LOW.

- **RT shader / denoiser / lifecycle bug-bash** —
  `2e5a56c` (#733 / RT-11) hoist `N_bias` for V-aligned ray-origin
  flip on all 4 RT sites, `07bfef8` (#741 / RT-12) align shadow ray
  tMin to 0.05 to match `N_bias`, `9885a9c` (#742 / RT-14) raise GI
  ray tMax to 6000.0 to match the fade window, `4f743a0` (#743 /
  DEN-10) wire composite exposure through `depth_params.y` instead
  of a const, `6385bfa` (#744 / DEN-11) pass `direct4.a` through
  composite sky branch, `7da94e8` (#745 / SH-13) `textureLod` with
  analytic mip for all 4 cloud layers (kills horizon aliasing),
  `0b18cd8` (#737 / SH-14) SVGF temporal nearest-tap fallback for
  sub-pixel silhouette miss, `6d12f75` (#738 / SH-15) bounds-check
  caustic_splat instance against R16_UINT ceiling. Lifecycle:
  `cb230ad` + `96d5fbd` (#732 / LIFE-N1) explicit deferred-destroy
  drain in App shutdown + clear long-lived GpuBuffer Vecs + take
  staging pool on destroy, `faee6a3` (#665 / LIFE-L1) early-return
  Drop when allocator Arc unwrap fails, `320712f` (#33 / R-10)
  align renderer teardown order via shared helpers, `5b73aa1`
  (#735) resolve `pipeline_cache.bin` next to executable rather than
  cwd. AS / pipeline cache: `f8a9719` (#739 / AS-8-13) route
  `drop_skinned_blas` through `pending_destroy_blas`, `bd0db2f`
  (#740 / AS-8-14) advance `frame_counter` in `build_blas_batched`
  so M40 streaming bursts enforce the BLAS budget, `0a440b5` (#736
  / PS-9) eliminate null-handle footgun in
  `recreate_triangle_pipelines`, `30fc453` (#734) align static
  `depth_compare_op` to LESS_OR_EQUAL on opaque/blend pipelines.
  Risk-reducer follow-ups: `fc7445a` (#647 / RP-1) +
  `0fc1e03` (#648 / RP-2) + `ae5ea0e` (#667 / SH-12) +
  `961e77f` (#651 / SH-6) + `6738c05` (#645 / MEM-2-3 — shrink TLAS
  instance buffer after exterior peak).

- **Cell loader / weather / fallback** — `de71b4f` (#542 / M33-10)
  procedural-fallback exterior installs `GameTimeRes` + synthetic
  `WeatherDataRes` so the overworld renders before any WTHR loads,
  `979ad9e` (#541 / M33-09 minimum) plumbs SKY_LOWER through to
  composite.frag's below-horizon branch, `4f3b50f` (#729) corrects
  WTHR NAM0 group indices to match xEdit fopdoc, `4f705eb` +
  `5986ba7` (#730) log cloud DDS dimensions/format/mip count +
  preserve authored mip chain on uncompressed-RGBA DDS, `a7eb039`
  parses FO4 NPC face-morph block (FMRI/FMRS/MSDK/MSDV/QNAM/HCLF/
  BCLF/PNAM).

- **Animation / asset path hygiene** — `881c2d5` (#231 / SI-04)
  intern animation text-key labels into `StringPool`, `a62c5fc`
  (#610 / D4-NEW-02) plumb `TexClampMode` end-to-end so
  CLAMP-authored decals stop bleeding.

- **Doc tracking** — `AUDIT_NIF_2026-04-26.md` + 23 issue dumps
  filed (`f47450f`); `AUDIT_NIF_2026-04-28.md` +
  `AUDIT_RENDERER_2026-04-27.md` + `AUDIT_STARFIELD_2026-04-27.md`
  drafted but uncommitted at session close (#764 closed in-session;
  #765 / #766 carry forward).

Net: tests +56 (1 456), LOC +4 570 non-test (~121 669), source
files +5 (264), issue dirs +38 (726), 60 commits, no milestone
churn (M40 Phase 1a/1b shipped but the milestone stays open until
the multi-cell grid + M41 actor spawning land), no bench refresh
(`6a6950a` now 182 commits stale → R6a-stale-6; some session
changes are GPU-side correctness — DEN-10/-11, RT-11/-12/-14,
SH-13/-14/-15 — that may shift the bench numbers, but Prospector /
Whiterun / MedTek don't exercise the affected paths heavily, and
M41 remains the gating event for the next bench-of-record).

---

## Session 22 — Cell-loader monolith refactor + Oblivion / NIF audit closeouts  (2026-04-27, 552f494..db62c94)

Two-track session driven by the next-day filing of two large audit
reports (`AUDIT_OBLIVION_2026-04-25.md` + `AUDIT_NIF_2026-04-26.md`)
on top of session 21's audit closeout. The first half tore the cell
loader out of the byroredux binary monolith into submodules; the
second half worked the audit backlogs in parallel, closing 30+
issues across NIF parser correctness (FO4+ wire-layout gaps surfaced
by the corpus sweep), Oblivion mesh / ESM hardening, memory and
acceleration-structure lifecycle (MEM-2-* bundle), and a cluster
of small RT / denoiser / VFX corrections.

- **Cell-loader monolith refactor (stages A–D + targeted extracts)** —
  `883d5ed` stage A pulled test mods to sibling files, `a231fd5`
  stage B split `esm/cell.rs` into a moduledir, `26e11db` stage C
  did the same for `controller.rs` + `material.rs`, `b8d5ed9`
  stage D introduced a `SubReader` cursor for typed sub-record decode
  (R2 risk-reducer first installment). Then `d338dd6` / `8bfb521` /
  `c101925` / `15a2bb3` extracted `terrain` / `refr` / `load_order` /
  `nif_import_registry` submodules out of the monolithic `cell_loader.rs`.
  `09dbcfc` cargo-fmt sweep across the workspace closed the refactor.
- **NIF parser correctness (audit `2026-04-26` closeout)** — `#708`
  Starfield BSGeometry/SkinAttach/BoneTranslations triple (190 549
  blocks recovered from NiUnknown), `#711` FO4 LOD chunks
  `data_size=0` with non-zero counts, `#712` FO76/Starfield CRC32
  shader-flag arrays, `#713` BSSkyShaderProperty + BSWaterShaderProperty
  split off the FO3 PP alias arm, `#715` pre-10.0.1.4 embedded
  `NiSourceTexture` `Use Internal` byte, `#721` FO4+ NiLight
  reparented onto NiAVObject (`vercond=#NI_BS_LT_FO4#` — 681
  light blocks), `#722` BSClothExtraData omits NiExtraData Name
  per `excludeT="BSExtraData"` (1 523 cloth blocks), `#727`
  Starfield BSFaceGenNiNode aliased to NiNode (1 282 face NIFs).
- **Oblivion audit closeout (`2026-04-25`)** — `1feb678` filed the
  audit + 20 issue dumps. Code fixes: `#687` two NiController*
  parsers misalign Oblivion stream (NiGeomMorpherController +
  NiControllerSequence), `#688` defer-with-empirical-refutation of
  the audit's "v=20.0.0.5 subset" framing for root-NiNode truncation
  (the 149 affected files are pre-Gamebryo NetImmerse-vintage —
  `f9fc292` documents this so future audits don't re-derive the
  stale framing), `#692` XOWN/XRNK/XGLB ownership tuple plumbed
  through CELL + REFR, `#694` consume `NiVertexColorProperty.lighting_mode`,
  `#696` zero `specular_color` too when `NiSpecularProperty` disabled,
  `#699` killed stale "BSA v103 decompression NOT WORKING" framing
  (`v103` extracts 147 629 / 147 629 vanilla files), `#700` / `#702`
  comment-only fixes for stale BSA flag and LIGH BGRA framing,
  `#704` route NiTexturingProperty slot 3 to roughness (Phong
  exponent), not specStrength (`O4-06`), `#705` drop unconsumed
  `MaterialInfo.decal_maps`.
- **Skyrim audit hardening (`SK-D1` / `SK-D2` series)** — `#621`
  BsTriShape parser hardening (VF_FULL_PRECISION + derived stride),
  `#622` BSA reader hardening bundle (4 items), `#564` LOD-batch
  subtree skip, `#566` LGTM lighting-template fallback.
- **MEM-2-* + lifecycle bundle** — `#639` (LIFE-H1)
  `pending_destroy_blas` drained in `AccelerationManager::destroy`,
  `#643` evict idle SkinSlots + matching skinned BLAS per frame,
  `#644` (MEM-2-2) emit scratch barrier before every per-frame BLAS
  refit, `#680` (MEM-2-5) assert persistent mapping on CpuToGpu
  allocations, `#681` (MEM-2-6) drop unused VERTEX_BUFFER from
  skin_compute output.
- **RT / denoiser / VFX** — `#574` (RT-2) Frisvad orthonormal basis
  kills NaN on (0,1,0) normals, `#672` (RT-9) sanitise zero-radius
  lights at spawn, `#674` (DEN-4) host-side SVGF temporal-α knob
  for discontinuity recovery, `#676` (DEN-6) TAA preserves HDR.a
  alpha-blend marker bit, `#641` (SH-3) skinned motion vectors via
  prev-frame bone palette, `#706` (FX-1) BSEffectShaderProperty
  routed through emit-only `MATERIAL_KIND_EFFECT_SHADER` (101) —
  fixes rainbow-tinted Whiterun hearth flames, `#707` brighter
  smoke + dedicated embers preset for fire emitters,
  `137c50d` reconstruct BC5 normal-map Z in `perturbNormal`,
  `e38f08a` `BYROREDUX_RENDER_DEBUG` env-driven fragment-shader
  bypass flags.
- **ESM dispatch + cell-loading + asset path hygiene** — `#561`
  multi-master CLI (M46.0) — repeatable `--master <path>` arg +
  `EsmIndex::merge_from`, `#629` (FNV-D2-01) ENCH enchantment
  records dispatch, `#634` (FNV-D2-06) drive `EsmIndex::total()`
  + log line off one table, `#635` (FNV-D3-05/06) NifImportRegistry
  LRU + nested PKIN expansion, `#540` (M33-08) WLST entry size
  dispatched on GameKind not data length, `#544` cell-loader REFR
  meshes carry Name + Parent + AnimationPlayer, `#587`
  (FO4-DIM2-05) integration tests for BA2 + BSA readers, `#609`
  (D6-NEW-01) intern NIF texture paths through `StringPool`.
- **#529 (FNV-CELL-5)** — derive cloud `tile_scale` from authored
  DDS width (audit hedged on a non-existent `cloud_scales` field;
  per-WTHR authority comes from the artist's sprite resolution
  choice — 5-test pure-helper coverage).
- **Doc tracking** — 2 audit reports added (`AUDIT_OBLIVION_2026-04-25.md`,
  `AUDIT_NIF_2026-04-26.md`) plus ~30 curated `ISSUE.md` dumps
  under `.claude/issues/` for #708–#728.

Net: tests +127 (1 400), LOC +8 284 non-test (~117 099), source
files +51 (259 — monolith refactor accounts for almost all of it),
issue dirs +23 (688), 53 commits, no milestone churn, no bench
refresh (`6a6950a` now 121 commits stale → R6a-stale-5; M41 actor
spawning still the gating event). Per-game NIF parse rates
unchanged in the matrix because no fresh corpus sweep ran;
`#721` / `#722` / `#727` predict ~3 500 fewer demotions across
FO4 / FO76 / SF Meshes archives once `BYROREDUX_REGEN_BASELINES=1`
runs locally.

---

## Session 21 — RT shader bug-bash + AS sync hardening  (2026-04-26, 333b79e..f41912e)

Pure audit-bundle closeout on top of `AUDIT_RENDERER_2026-04-25.md`
filed at commit `20b8ef0`. No milestone churn — every commit pays
down a CRITICAL / HIGH or a MEDIUM/LOW finding the post-M29 audit
pass surfaced. The signal: a clutch of latent ray-query mathematical
errors (grazing-angle reflections, empty-instance TLAS UPDATE,
mis-sized GI tMin), AS scratch hazards exposed by M29's per-frame
skinned refit, and lifecycle gaps in `Texture` / `GpuBuffer` Drop
that release builds silently leaked through. Audit-finding hygiene
also closed out two stale-premise findings (`#689`, `#684`).

- **RT shader correctness** — `#545` (NiFlipController parser →
  TextureFlipChannel emission), `#640` (caustic_splat ray flags +
  shader/CPU RT-enable gates), `#666` (fog_near clamp at scene-buffer
  upload), `#668` (reflection ray V-aligned `N_view` flip in lockstep
  with the glass-IOR path), `#669` (GI ray `tMin` 0.5 → 0.05 to match
  bias), `#670` (caustic origin bias along light-facing normal with
  non-zero tMin).
- **Acceleration structure hardening** — `#642` (per-frame skinned
  BLAS refit emits AS_WRITE→AS_WRITE serialise barrier between
  iterations), `#657` (`decide_use_update` empty-list short-circuit
  + regression test), `#658` (single-shot `build_blas` declares
  `ALLOW_COMPACTION` in lockstep with the batched path), `#659`
  (runtime assert on `minAccelerationStructureScratchOffsetAlignment`),
  `#660` (TLAS BUILD-vs-UPDATE address scratch amortized via swap
  with `tlas.last_blas_addresses`).
- **Sync hardening** — `#653` (SVGF + TAA post-dispatch dst-stage
  widened to `FRAGMENT | COMPUTE` so next-frame compute reads see
  the right barrier without depending on the per-frame fence).
- **Compute pipeline polish** — `#652` (`cluster_cull.comp`
  parallelised to 32-thread workgroups, ~32× compute occupancy on
  populated exteriors), `#662` (`SkinPushConstants` trimmed
  16 → 12 B by dropping the decorative `_pad`), `#663` (UI overlay
  static-vs-dynamic-state invariant codified via
  `pub const UI_PIPELINE_DYNAMIC_STATES` + a `const_assert` at the
  call site).
- **Resource lifecycle** — `#656` (Texture + GpuBuffer Drop now
  self-clean using stashed device + allocator handles instead of
  silently leaking VkImage/VkBuffer + the gpu_allocator slab in
  release builds).
- **Audit-finding hygiene** — `#684` (per-game parse-rate claim
  refreshed against a fresh 7-game integration sweep at `0681fc7`:
  Oblivion 95.21%, FO4 96.46%, recover=100% framing replaces the
  stale "100% across 177 286 NIFs"), `#689` (NiSequenceStreamHelper
  marked vanilla-unused — 0 of 47 934 vanilla NIFs use it; the
  audit's missing-importer-path concern was the wrong tree).
- **Doc tracking** — `9820f28` committed 4 audit reports + ~70
  curated `ISSUE.md` / json dumps under `.claude/issues/`, `549a5f7`
  refreshed `docs/engine/*.md` against sessions 13-20 reality
  (M29 GPU skinning, NiFlipController, scratch barrier,
  AnimatedColor split, BLAS budget, GI tMin).

Net: tests +3 (1273), LOC +785 non-test (108 815), 19 commits, no
milestone churn, no bench refresh (R6a-stale-3 → R6a-stale-4 — 65
commits stale; M41 actor spawning is still the gating event).

---

## Session 20 — M29 GPU pre-skinning end-to-end + audit closeout  (2026-04-25, 6e70751..b8834cc)

11-commit session anchored on M29: discovered the existing CPU
skinning chain works end-to-end on real game content (`f60e27c`
reframed M29 from "compute-shader bone-palette eval" to "skinning
chain verified"), then shipped a separate compute-shader arc focused
on RT correctness — animated NPCs were going to cast bind-pose
shadows / reflections / GI because the BLAS is built once at upload
time. The fix is per-skinned-entity BLAS keyed on `EntityId`, refit
each frame against a `SkinComputePipeline` output buffer. M41 NPC
spawning now has a working skin chain to consume; Phase 3 (raster
reads pre-skinned vertices) deferred behind the M41 stability gate.

- **M29 GPU pre-skinning + per-entity BLAS refit (3 commits)** —
  `de1ea1f` Phase 1: pipeline + `SkinSlot` + descriptor set layout +
  pool, no live dispatch. `1ae235b` Phase 1.5+2: per-frame compute
  dispatch + sync first-sight BLAS BUILD + per-frame UPDATE refit +
  TLAS build relocated after the skin chain for zero-lag RT. New
  `skin_vertices.comp` mirrors `triangle.vert:147-204`'s weighted-
  bone-matrix-sum into 21-float Vertex output. `AccelerationManager`
  gains `skinned_blas: HashMap<EntityId, BlasEntry>` with
  `ALLOW_UPDATE | PREFER_FAST_BUILD` so refit-in-place is legal each
  frame; `build_tlas` learns the per-entity override on
  `bone_offset != 0`. `b8834cc` Phase 3 deferred to **M29.3** in
  Tier 5 — gated on M41 NPC rollout proving the compute + refit
  chain stable on visible animated content; raster's well-tested
  inline-skinning stays the source of truth for now.

- **#638 SSE skin payload surfaced from `SseSkinGlobalBuffer`**
  (`156290e`) — pre-fix `decode_sse_packed_buffer` skipped the
  12-byte VF_SKINNED block because the comment claimed
  `extract_skin_bs_tri_shape` recovered it elsewhere. That elsewhere
  read `shape.bone_weights` which is empty when geometry lives in
  the global buffer — the canonical state for Skyrim SE NPC bodies.
  Every vertex imported with zero weights, hit the rigid fallback,
  would render in bind pose once M41 lands. `decode_sse_packed_buffer`
  now surfaces weights + partition-local indices into
  `DecodedPackedBuffer`; `extract_skin_bs_tri_shape` falls back to
  the global-buffer payload when inline arrays are empty.
  `skinning_e2e.rs` flipped its soft-flag from `eprintln + return`
  to `assert!(!is_empty)`. Without this fix M29's GPU pre-skinning
  would produce zero-weight vertices on every Skyrim NPC.

- **ESM coverage gaps closed (2 commits)** — `9e9aeef` #519 lifts
  the AVIF top-level GRUP out of the catch-all skip in `parse_esm`;
  64 actor-value definitions on FalloutNV.esm now resolve, unblocking
  every NPC `skill_bonuses` / BOOK skill-book / ~300 AVIF-keyed
  condition predicate that was dangling. `3d8ec7d` #631 adds the
  dedicated `extract_dial_with_info` walker for the DIAL Topic
  Children sub-GRUP (`group_type == 7`) that the generic walker
  silently skipped — **23,247 INFO records surfaced** on FalloutNV.esm
  (out of 18,215 DIAL records, 9,493 with non-empty INFOs); pre-fix
  every `DialRecord.infos` was empty.

- **Renderer correctness (3 commits)** — `7f28aea` #628 sources the
  cluster grid's far plane from CLMT fog_far (`screen.w`) at runtime
  with a 10000-floor / 50000-fallback rather than hardcoding 10000;
  FNV exterior weather pushes fog to 30K-80K and every light past
  10000 was silently culled from the per-cluster list. `19e7115`
  #619 per-variant gates the `BSLightingShaderProperty` payload pack
  in `build_render_data` so non-active material kinds skip the 9
  Option chains entirely (~99% of a typical cell hits the new fast
  path). `aba1246` #623 documents the FO76 type-12 EyeEnvmap
  catch-all against nif.xml line ranges + adds a debug_assert
  pinning the `multi_layer_envmap_strength` ↔ `hair_tint` vec4-share
  mutual-exclusion invariant.

- **BSA hygiene (1 bundled commit)** — `5cb5336` Fix #593 + #595:
  synthesized DDS_HEADER_DXT10 `arraySize` is now 6 for cubemaps and
  1 for non-cubemaps (was hardcoded to 1, rejected by DXGI loaders);
  stale `0x0800 = cubemap?` comment in `read_dx10_records` rewritten
  to name the verified bit (0).

Net: tests +14 (1256 → **1270**), LOC +2 315 non-test (108 030
total), +1 source file (`crates/renderer/src/vulkan/skin_compute.rs`).
Bench-of-record `6a6950a` now 45 commits stale (was 33 at S19 close);
no functional movement expected on Prospector — no NPCs spawn so
M29's compute chain is dormant, and the cluster_cull FAR change is
an interior-bench no-op.

---

## Session 19 — Audit bundle closeout: parser correctness + RGBA pipeline + mem.frag visibility  (2026-04-25, a2a3fcd..79c81b9)

A 25-commit bug-bash burning through audit findings #221 / #404 / #435 /
#503 / #559 / #565 / #569 / #576 / #580 / #581 / #590 / #592 / #604 /
#605 / #611 / #612 / #613 / #614 / #615 / #617 / #618 / #626 / #627 /
#632 / #633 — most LOW/MEDIUM, one HIGH (#559) and one whole-pipeline
change (#221). Two issues self-deferred (#351 tangents, #520 PERK
entry points), one was already fixed under another number (#46 → #238).
The session closes Skyrim SE skinning end-to-end, lands a real per-block
GPU memory fragmentation reporter, and pushes `NiMaterialProperty`
diffuse + ambient through to the fragment shader as a 320→352 B
`GpuInstance` growth touching all four shaders in the Shader Struct Sync
chain.

- **NIF parser correctness (12)** — `NiLookAtInterpolator` static pose
  surfaced as a translation channel (#604, `ac30826`); `NiPathInterpolator`'s
  `NiPosData` keys reach the same channel (#605, `44ab041`); `NiPSysData`
  particle-info array gated on pre-BS202 (#581, `4bdccca`);
  per-vertex alpha preserved end-to-end as RGBA, not RGB (#618, `9ebb7ea`);
  `NiStringsExtraData` switched to `SizedString` (#615, `7b7dadc`);
  `parse_nif` root selector now recognises every `NiNode` subclass —
  BSTreeNode, NiSwitchNode, BSMultiBoundNode, NiBillboardNode,
  BSBlastNode, BSDamageStage, BSFadeNode, NiBSPNode, BSOrderedNode,
  BSValueNode, BSDebrisNode (#611, `8d09b97`); new `BSBoneLODExtraData`
  parser restores Skyrim Meshes0 to 100% clean parse (#614, `782b723`);
  `bhkBreakableConstraint` reads its trailer on FNV/FO3 too (#633,
  `d17f8d9`); SSE skinned `BsTriShape` reconstructs from `NiSkinPartition`
  global buffer — Skyrim NPCs / dragons now render geometry (#559,
  `b6e0779`); partition-aware bone-index remap so multi-partition skins
  pick the correct global bone per vertex (#613, `8f9584e`);
  `BSSubIndexTriShape` structured-decode replaces the wholesale
  `block_size` skip — segment table + sub-segments + .ssf filename now
  recovered (#404, `92e5e93`); recovery-path warnings aggregated into
  per-NIF summary lines so Skyrim Meshes0 sweep drops from thousands of
  warnings to ~133 (#565, `a2bc079`).

- **End-to-end material pipeline (3)** — `NiTexturingProperty` UV
  transform survives a preceding `NiMaterialProperty` via a new
  orthogonal `has_uv_transform` flag (#435, `006fdde`); FO4 Material Swap
  (`MSWP`) records parse with substitution table + texture overrides
  (#590 first slice, `98f497e`); `NiMaterialProperty.diffuse` + `.ambient`
  plumbed end-to-end — `MaterialInfo` → `ImportedMesh` → `Material` ECS
  component → `DrawCommand` → `GpuInstance` (320→352 B, 2 appended vec4
  slots) → `triangle.frag` (`albedo *= diffuseRGB` + `ambient *= ambientRGB`),
  with the Shader Struct Sync mirror across `triangle.vert` /
  `ui.vert` / `caustic_splat.comp` and SPIR-V regen (#221, `79c81b9`).

- **ESM / cell streaming leaks (3)** — `SkyParamsRes` textures dropped
  + cell-state Resources cleared on unload (#626, `cd55e4f`); terrain
  splat-layer texture refcounts released on cell unload (#627, `a3eb4c4`);
  ESM-fallback `LightSource` now fires for zero-color NIF placeholders
  so Megaton lanterns aren't dark (#632, `971c694`).

- **Renderer / shader hygiene (5)** — FO76 `SkinTint` material_kind
  remapped so `triangle.frag`'s ladder branches dispatch correctly
  (#612, `1a09347`); `fo4_slsf1` / `fo4_slsf2` arrays consumed in
  production with compile-time bit-equivalence guards proving FO4 +
  Skyrim flag bit semantics match where they should (#592, `d2dbecf`);
  rasterization pipelines retained on format-stable swapchain resize —
  no needless `vkDestroyPipeline` on every window-size event (#576,
  `2bbc0a0`); SAFE-21 unsafe-block comment rewritten to name the real
  invariant (#580, `3c05ec8`); `mem.frag` console command + per-block
  fragmentation reporter (`largest_free / total_free` ratio with < 0.5
  WARN threshold) — gpu-allocator's first-fit-within-block strategy now
  has visibility (#503, `2a655e1`).

- **Test infrastructure (2)** — Synthetic v105 BSA fixtures land for CI-
  side coverage of the LZ4 frame path (#617, `1bbbcb2`); gated regression
  sweep for Skyrim SE BSA v105 (#569, `606137d`).

Net: tests +67 (1189 → **1256**), LOC +5 215 non-test (105 715 total),
+3 source files. `GpuInstance` stride 320 → 352 B (#221). Bench-of-record
`6a6950a` now 33 commits stale (just past the 30-commit threshold) — flagged
under Known Issues `R6a-stale-2`; refresh deferred to next session.

---

## Session 18 — Risk-reducer triple: R3 + R6 + R7 plus the parser fixes they surfaced  (2026-04-24, 4293c51..a9c7bc9)

An eight-commit session organised around the prevention-tooling track:
each of the three closed risk-reducers added a piece of telemetry
that turned a previously-invisible problem class into something with
a name and a count, and the two NIF parser fixes that landed mid-
session were directly off the histogram R3 produced. Closes the loop
end-to-end — ship the gate, ship the data it needs, regenerate the
baselines so future regressions auto-trip without operator vigilance.

- **R3 — per-block parsed/unknown histogram + CI baseline gate**
  (`6a6950a`, `a9c7bc9`). 100% file-level parse rate hides per-parser
  regressions: the recovery path inserts `NiUnknown` keyed on the
  original advertised type and the file rate stays green while
  geometry silently disappears. New per-block `parsed`/`unknown`
  attribution (downcasts each `NiUnknown` to read the preserved
  `type_name`), `--tsv` and `--unknown-only` flags on `nif_stats`,
  `PerBlockHistogram` + `compare_histograms` test infrastructure,
  and a new opt-in `per_block_baselines.rs` integration test with
  `BYROREDUX_REGEN_BASELINES=1` capture mode. End-of-session: TSVs
  for all 7 games checked in (`crates/nif/tests/data/per_block_baselines/*.tsv`)
  — Oblivion 98 types / FO3 91 / FNV 91 / Skyrim SE 83 / FO4 67 /
  FO76 65 / Starfield 24. Validate path now asserts every game's
  `unknown` count never grows and `parsed` count never shrinks
  against the checked-in TSVs.

- **R3-driven parser fixes — 114 instances recovered across 4 games**
  (`88f58b5`, `7548e64`). `NiBSBoneLODController` was reading the
  shape-group tail unconditionally and over-consuming 4+ bytes past
  the body on every Bethesda game past Oblivion; nif.xml gates those
  fields on `vercond="#NISTREAM#"` (`#BSVER# #EQ# 0`) — present only
  on Morrowind / Oblivion / pure-Niflib content. Wrapping the tail
  in `if stream.bsver() == 0` recovered 91 instances (FNV 34 + FO3 19
  + Skyrim SE 3 + Oblivion 35 unchanged via the bsver=0 path).
  `NiLookAtInterpolator` had no dispatch entry at all; new parser in
  `interpolator.rs` with `look_at_flags::{LOOK_FLIP, LOOK_Y_AXIS,
  LOOK_Z_AXIS}` u16 constants (no bitflags dep, mirrors
  `shader_flags.rs` style). Recovered 23 instances (FNV 18 +
  Skyrim SE 5). FNV reaches **100% clean parse for the first time**
  (14 881/14 881, truncated 0 → was 6 after just the BoneLOD fix
  because the remaining 6 were NiLookAt chain failures).

- **R6 — `ctx.scratch` console command + ScratchTelemetry resource**
  (`61fe6e1`). `VulkanContext` holds five persistent `Vec` scratches
  whose capacity grows with `Vec::reserve` driven by outlier frames;
  pre-fix M40 cell streaming would have grown them unbounded with
  zero observability. `ScratchRow` (name, len, capacity, elem_size)
  + `ScratchTelemetry` resource refreshed each frame from
  `VulkanContext::fill_scratch_telemetry`; `ctx.scratch` console
  command surfaces per-Vec `bytes_used` / `wasted`. Prospector
  baseline: 337 KB total across 5 scratches, 320 B wasted (essentially
  right-sized; `gpu_instances_scratch` 773/774 is the only non-zero
  waste row).

- **R6a-stale — bench-of-record refreshed at `6a6950a`** (`7313823`).
  42 commits since `e6e8091`. New three-bench run on RTX 4070 Ti:
  Prospector 172.6 FPS / 5.79 ms (was 192.8 / 5.19 — slide is
  compositor jitter, fence_ms unchanged at 4.34, brd_ms unchanged at
  0.86); Skyrim Whiterun 253.3 FPS / 3.95 ms at **1 932 entities (up
  53% from 1 258)** while FPS still improved — more REFRs land per
  cell now without perf cost; FO4 MedTek 92.5 FPS / 10.82 ms at
  unchanged 7 434 entities. Worth a future bisect to identify which
  commit expanded Skyrim REFR coverage; not a regression either way.

- **R7 — scheduler access declarations + `sys.accesses` console
  command** (`b362e88`). Per-storage RwLock + lock_tracker handle
  correctness already; what they don't give is a static answer to
  "which systems serialise on storage X?" before M27 turns parallel
  dispatch on. New `Access` builder
  (`Access::new().reads::<T>().writes::<U>().reads_resource::<R>()`),
  optional `System::access() -> Option<Access>` (default `None` so
  closures stay undeclared), `Scheduler::add_to_with_access` for
  registration-side overrides, `access_report()` per-stage
  `None` / `Conflict { pairs }` / `Unknown` analysis snapshotted as
  `SchedulerAccessReport` resource. `sys.accesses` console command
  surfaces it. Three exemplar systems migrated (`fly_camera_system`,
  `spin_system`, `log_stats_system`); 9 of 12 still undeclared,
  showing as Unknown pairs. M27 can flip on with diagnosable
  contention — every Unknown pair becomes a concrete to-do.

- **BA2 cap bump for FO76 vanilla** (`4a2b820`). Surfaced by the R3
  baseline regen pass: `MAX_CHUNK_BYTES = 256 MB` rejected
  `SeventySix - Meshes.ba2` because it ships a genuine 325 MB packed
  mesh entry. The cap's docstring claim "vanilla GNRL records are
  under 8 MB" was wrong for FO76. Bumped to 1 GB (still rejects
  u32::MAX cleanly, ~3× headroom over the FO76 ceiling). Side
  finding: the `Fallout 76 100% (58 469)` claim in ROADMAP was stale
  — `open_mesh_archive` returns `None` on archive-open failure and
  the per-game integration test silently passed without doing any
  work. Cap bump unblocks both the baseline regen and any future
  parse-rate sweep on the same data.

Net: tests +37 (1 152 → **1 189**), LOC +1 665 non-test (100 465
total), +1 source file (`crates/core/src/ecs/access.rs`). Three
risk-reducers closed (R3, R6, R7); FNV reaches 100% clean parse;
R3 baselines locked across 7 games. Bench-of-record refreshed
(7 commits stale at session close — well inside the 30-commit
freshness threshold).

---

## Session 17 — Audit bundle #572–603 closeout: FO4 consumers + NIF coverage + renderer hygiene  (2026-04-24, cd959cf..e4cf68b)

An 18-commit bug-bash against the post-session-15 audit sweep
(`AUDIT_FO4_2026-04-23`, `AUDIT_RENDERER_2026-04-22`,
`AUDIT_SAFETY_2026-04-23`) plus a handful of cross-cutting older
issues that had stale premises retired. The session started by seeding
32 issue dirs (#572–603) and the three audit reports as durable
artifacts, then worked through the highest-signal consumer-side gaps —
FO4 texture / SCOL / PKIN REFRs rendering empty, BGSM scalars silently
dropped, Skyrim items landing in `EsmIndex` with three-byte garbage
names — before draining the remaining NIF dispatch misses and one
spec-violation descriptor-write race.

- **FO4 ESM consumer wiring** — five FO4 records had parsers but no
  cell-loader follow-through, so vanilla Fallout 4 interiors rendered
  conspicuously wrong: #583 `merge_bgsm_into_mesh` forwards the BGSM /
  BGEM scalar suite (emissive / specular / smoothness / material
  alpha / UV / two_sided / decal / alpha_test) via per-field override
  flags, not just the six `Option<String>` texture slots; #584 REFR
  `XATO` / `XTNM` / `XTXR` / `XEMI` parse + `RefrTextureOverlay` that
  shadows `ImportedMesh` texture reads at spawn time with per-slot
  precedence (XATO/XTNM merge first-non-empty, XTXR later-wins);
  #585 `expand_scol_placements` fans SCOL REFRs into synthetic
  children when `statics[base].model_path` is empty (mod-added SCOL or
  previsibine bypass); #589 `parse_pkin` + `expand_pkin_placements`
  for pack-in bundles (872 vanilla records were silently dropping
  their CNAM content lists); #602 LIGH `XPWR` power-circuit FormID
  captured onto `LightData` as pre-work for the settlement-circuit
  ECS system.

- **NIF dispatch coverage** — #394 closed the last four
  Oblivion-unskippable types (`NiPathInterpolator`,
  `NiFlipController`, `NiBsBoneLodController`, `BhkMultiSphereShape`)
  with byte-exact `stream.position() == bytes.len()` guards since
  Oblivion has no `block_sizes` table for recovery; #557 parsed six
  rare Havok tail types (`BhkAabbPhantom`, `BhkLiquidAction`,
  `BhkPCollisionObject`, `BhkConvexListShape`, `BhkBreakableConstraint`,
  `BhkOrientHingedBodyAction`) draining the NIF-12 unknown bucket
  across all four pre-FO4 games; #336 declared `VF_UVS_2` /
  `VF_LAND_DATA` constants to match nif.xml's 11-bit vertex-attribute
  mask (decoding deferred per no-guessing policy — no consumer to
  validate against); #338 added a crate-independent
  `AnimationController` (`SparseSetStorage` component + catalog +
  transition matrix + `apply_pending_transition`) closing the AR-09
  glue gap between the KFM parser and `AnimationStack`.

- **Renderer / Vulkan correctness** — #92 closed a spec violation
  (`VUID-vkUpdateDescriptorSets-None-03047`): `update_rgba` /
  `drop_texture` / `write_texture_to_all_sets` used to synchronously
  rewrite every bindless descriptor set including any in-flight one.
  New per-slot `pending_set_writes` queue drained from `begin_frame`
  after fence-wait, so non-current slots get their writes deferred
  until safe. #578 dropped the baked `viewports` / `scissors` arrays
  on four `PipelineViewportStateCreateInfo` sites — every one of our
  pipelines already declared the state dynamic and set it per-frame
  via `cmd_set_viewport` / `cmd_set_scissor`, so the static arrays
  were ignored-but-misleading dead code. #594 split DDS header
  emission: uncompressed formats (DXGI 28/29/87/91/56/61) now emit
  `DDSD_PITCH` with `width * bpp`, block-compressed keep
  `DDSD_LINEARSIZE`; the old "always-LINEARSIZE" was rejected by
  strict validators (texconv, DirectXTex). #577 corrected three stale
  `GpuInstance` doc sites from 192 B to 320 B (the size grew via #492
  +32 B UV/material_alpha and #562 +96 B Skyrim+ BSLightingShader
  variant payloads).

- **Safety hardening** — #586 mirrored the NIF #388 pattern onto BSA +
  BA2: new `crates/bsa/src/safety.rs` with `MAX_ENTRY_COUNT = 10M` and
  `MAX_CHUNK_BYTES = 256 MB` checked at every allocation-from-header
  site (`file_count`, `folder_count`, per-folder `count`, GNRL
  packed/unpacked, DX10 chunk packed/unpacked, compressed
  `original_size`). Prevents `u32::MAX`-header DoS on malformed or
  hostile archives. #597 added a `warn!` on BA2 DX10 `num_mips = 0`
  and documented the intentional `.max(1)` clamp in `build_dds_header`
  (operator signal, not a correctness fix — vanilla FO4 never trips
  it but third-party repackers occasionally do).

- **ESM correctness** — #348 detected the Skyrim TES4 `Localized`
  flag (`0x80`) and routed FULL / DESC at ~25 sites through a new
  `read_lstring_or_zstring` helper that returns
  `"<lstring 0xNNNNNNNN>"` for 4-byte `.STRINGS` refs instead of
  3-char UTF-8 garbage; thread-local `CURRENT_PLUGIN_LOCALIZED`
  toggled per-plugin so a non-localized plugin can't inherit stale
  state. Real `.STRINGS` loader deferred (multi-week scope).
  #537 fixed Oblivion cells fogging to solid color a few units from
  the camera: HNAM had been decoded as `[day_near, day_far,
  night_near, night_far]`, but the real Oblivion HNAM is 14 × f32 of
  HDR eye-adaptation / sunlight-dimmer tuning per UESP; FNAM remains
  the authoritative fog source for every game (#536's FNV/FO3
  finding now inherits). #380 routed the XCLL directional-light
  rotation through the shared `euler_zup_to_quat_yup` helper that
  REFR placement has used since day one — the inlined astronomical
  azimuth/elevation formula on the XCLL branch didn't match Gamebryo's
  CW-positive convention (memoized in `gamebryo_cw_rotation`).

- **Audit seeding** — `66f9fae` landed 32 issue dirs (#572–603) plus
  the three audit reports under `docs/audits/`: a 30-finding FO4
  consumer sweep, a 20-finding renderer sweep, and a 12-finding
  safety sweep.

Net: tests +81 (1 071 → **1 152**), LOC +4 526 non-test (98 826
total), 3 new source files. Eighteen audit issues closed, no new
child issues opened. Bench-of-record unchanged (192.8 FPS / 5.19 ms
at `e6e8091`, **42 commits stale — crosses the 30-commit threshold**,
flagged in Known Issues pending a re-bench session.)

---

## Session 16 — NIF audit 2026-04-22 closeout: dispatch coverage + Oblivion bisect + ESM REFR/TXST expansion  (2026-04-23, 634929b..e0791b4)

A 14-issue bug-bash against `AUDIT_NIF_2026-04-22`'s dispatch-coverage
dimension plus two cross-cutting ESM fixes from the concurrent FO4
audit. The audit premise for most NIF findings was simple: a block
type name was in vanilla content but absent from `parse_block`'s match
arms, so every occurrence degraded to `NiUnknown` and silently lost
its data. The session wire-fixed all of them against nif.xml, with a
consistent discriminator-on-struct pattern for the wire-aliased cases.

- **Oblivion NiUnknown bisect (#554 → #581, #582)** — The audit framed
  NIF-09 as "32 distinct types fall into NiUnknown, bisect per type".
  Byte-level walk of 9 representative NIFs (`trace_block` + raw hex)
  collapsed that to **two upstream drift sources**, not 32: (1) for
  ~80 % of the pool, `NiPSysData` on pre-BS202 Bethesda streams omits
  the `Particle Info` array per nif.xml line 4030 — proven by a
  482-byte stream gap matching 15×28-byte NiParticleInfo + the
  inherited Rotation Speeds array in `landscapewaterfall02.nif`;
  (2) residual ~60-block animation-controller drift in non-particle
  NIFs (`obliviongate_forming.nif`, `dustcloudhorizontal01.nif`).
  Filed as child issues #581 (fix) and #582 (residual triage). Added
  three reusable bisect tools: `locate_unknowns`, `recovery_trace`,
  `dump_nif` (`a426ead`).

- **NIF wire-type dispatch coverage** — Each variant below preserves
  its RTTI via either a dedicated struct or a kind-enum discriminator
  on the shared struct, so `block_type_name()` reports the original
  subclass for downstream importers:
  - **#560 `BsTriShapeKind`** — `{ Plain, LOD { lod0, lod1, lod2 },
    MeshLOD, SubIndex, Dynamic }` on `BsTriShape`. `parse_lod` now
    preserves the three u32 LOD cutoffs (previously discarded);
    dispatcher splits `BSMeshLODTriShape` vs `BSLODTriShape` and
    uses `with_kind()` to override for the types that share a parser.
    Unblocks #404 segmentation parsing.
  - **#547 `NiAdditionalGeometryData` + `BSPackedAdditionalGeometryData`**
    — per-vertex tangent/bitangent/blend-weight channels. FNV 2 308 →
    0, FO3 1 731 → 0; total NiUnknown reduction: FNV −52 %, FO3 −57 %.
  - **#546 `bhkRigidBody` on Skyrim LE/SE** — three compounding
    root causes on `bsver 83..130`: missing 20-byte `bhkRigidBodyCInfo2010`
    prefix; `deactivator_type` hardcoded to 0 (contradicting nif.xml
    line 2844); 12-byte `Unused 04` trailer left unread. Skyrim SE
    Meshes0: bhkRigidBody 9 772 → 0, bhkRigidBodyT 3 094 → 0 (total
    SE NiUnknown −58 %).
  - **#548 `NiBoolTimelineInterpolator`** — `BoolInterpolatorKind {
    Plain, Timeline }` on `NiBoolInterpolator`. Audit premise that
    a `TimeBool100` field existed was contradicted by nif.xml line
    3287 (no extra fields). SE 6 796 → 0, FNV 1 118 → 0, FO3 536 → 0.
  - **#553 `NiFloatExtraData` / `NiFloatsExtraData` /
    `NiFloatExtraDataController`** — float metadata tags (FOV
    multipliers, wetness levels) + their animator. SE 1 312+180 → 0;
    total SE NiUnknown 1 626 → 134 (−92 %).
  - **#433 `Ni*LightController` family** — dedicated struct for
    `NiLightColorController` (preserves `target_color: u16` that the
    issue's matched-arm approach would have elided) + shared
    `NiLightFloatController { type_name, base }` for Dimmer / Intensity
    / Radius.
  - **#551 `bhkBlendController`** — inherits `NiTimeController` + u32
    `keys` (NOT `NiSingleInterpController` as the issue suggested).
    FNV 845 → 0, FO3 582 → 0.
  - **#552 `BSNiAlphaPropertyTestRefController`** — newtype around
    `NiSingleInterpController` (avoids the existing matched-arm
    RTTI-erasure pattern). SE 751 → 0.
  - **#550 `SkyShaderProperty`** — dedicated parser (was aliased to
    `BSShaderPPLighting`, over-reading 20+ bytes). nif.xml line 6335
    had two fields (File Name + Sky Object Type); the audit's "4 scroll
    vectors" claim was inaccurate. Recurring stderr warning bucket
    cleared on FNV + FO3 corpora.

- **ESM record expansion (FO4 audit overflow)** — Two long-standing
  coverage gaps in REFR/TXST:
  - **#406 `TXST.MNAM`** — BGSM material path. 139 of 379 vanilla
    `Fallout4.esm` TXST records (37 %) are MNAM-only with no TX00
    and were silently dropped by the `if set != default()` guard.
    `TextureSet.material_path: Option<String>` field added; BGSM
    parser resolution tracked as a separate issue.
  - **#412 `REFR` sub-records** — added `teleport: Option<TeleportDest>`
    (XTEL), `primitive: Option<PrimitiveBounds>` (XPRM),
    `linked_refs: Vec<LinkedRef>` (XLKR), `rooms: Vec<u32>` (XRMR),
    `portals: Vec<PortalLink>` (XPOD), `radius_override: Option<f32>`
    (XRDS) to `PlacedRef`. Live FO4: 538 doors + 14 279 triggers +
    9 257 linked refs + 36 559 light-radius overrides were previously
    dropped on the floor. Companion to the closed #349 (XESP).
    XRMR count clamped against payload bytes so corrupt counts can't
    over-read.

- **Renderer sync hardening (#572)** — Composite render pass `dep_in`
  `src_stage_mask` extended from `COLOR_ATTACHMENT_OUTPUT` to also
  cover `COMPUTE_SHADER` (SVGF / TAA / caustic / SSAO producers).
  Defense-in-depth: every upstream compute pass already emits its own
  explicit pipeline barrier, so validation never fired — closes the
  gap for any future compute pass that would rely on the render-pass
  dependency instead.

- **Docs staleness (#567)** — Single-mesh sweetroll FPS figure of
  1615 was pre-M31 (perf bundle #279 landed ~2× speedup). Updated
  ROADMAP:30 and `.claude/commands/audit-skyrim.md` to date-stamped
  `~3000-5000 FPS (2026-04-22, RTX 4070 Ti @ 1280×720)` per the
  project's existing convention. Dim-5 checklist uses `≥3000 FPS`
  as the defensible floor so future audits don't need to re-stamp
  on every driver drift.

- **Prior-session #558 pickup** — `3a8acde` landed four NIF-13 tail
  block parsers (`BSRefractionFirePeriodController` + 3 others) at
  the session boundary; folded into the same audit-bundle theme.

Net: tests +33 (1038 → **1 071**), LOC +2 385 non-test (94 285
total). Thirteen audit issues closed, two child issues opened
(#581, #582). Bench-of-record unchanged (192.8 FPS / 5.19 ms at
`e6e8091`, 23 commits stale — under the 30-commit threshold).

---

## Session 15 — Bench infrastructure, multi-game validation, sky completion  (2026-04-23, e6e8091..707b718)

Driven by two findings that surfaced back-to-back: the bench framework
had been measuring GPU submit time rather than wall-clock frame time, so
every FPS number since `bee6d48` was meaningless; and a sweep of the
active roadmap revealed that M32.5, M34, and parts of PERF-1 were already
complete but never formally closed. The session fixed the bench, profiled
the real bottleneck, validated Skyrim SE and FO4 cells end-to-end, and
shipped the remaining M33.1 sky work.

- **Bench methodology (PERF-1 phases 1–2)** — `--bench-frames` was
  counting `about_to_wait` ticks (winit event-loop callbacks), not rendered
  frames. On a composited desktop this inflated reported FPS ~5×
  (`e6e8091`). Fixed by moving `bench_frames_count` into `RedrawRequested`
  after `draw_frame` succeeds. Added `FrameTimings` struct with five
  sub-phases: `fence_wait`, `tlas_build`, `ssbo_build`, `cmd_record`,
  `submit_present` (`b7deb4c`). Prospector Saloon result:
  `wall_fps=192.8  wall_ms=5.19  fence=4.28ms (76%)`. Finding: **GPU-bound**
  on RT glass (bottles). CPU work is 0.87ms of 5.19ms total — optimising
  the CPU path would yield < 15% headroom. Also fixed the untracked
  per-frame `Vec::collect()` in `indirect_draws` → `indirect_draws_scratch`
  on `VulkanContext`.

- **Multi-game cell loader (M32.5)** — Validated Skyrim SE and FO4 interior
  cells against the existing cell loader with zero code changes. Skyrim SE
  WhiterunBanneredMare: 1258 entities @ 237 FPS. FO4 MedTekResearch01:
  7434 entities @ 90 FPS. Session 14's infrastructure (XCLL 92-byte
  parsing, BSTriShape geometry, SCOL expansion, BA2 reader) was complete;
  M32.5 needed only a test run to confirm.

- **Sky system completion (M33.1)** — Cloud layers 2 and 3 (WTHR ANAM/BNAM)
  added to the full pipeline: `CompositeParams` UBO gains `cloud_params_2/3`,
  `composite.frag` samples them with the same horizon-fade + UV-projection
  pattern as layers 0/1 (tile scales 0.25/0.30). Weather fade transitions
  via `WeatherTransitionRes`: `weather_system` blends post-TOD-sample colors
  by `t = elapsed/duration` (8 s default). On completion the resource is
  parked dormant (`duration = ∞`) rather than removed, avoiding the
  `&mut World` requirement from a `&World` system.

- **Roadmap housekeeping** — R6a closed (bench re-run with honest numbers);
  PERF-1 updated (GPU-bound finding, CPU path not the bottleneck); M32.5
  closed; M33.1 closed; M34 audited and closed (per-frame sun arc, TOD
  ambient/fog/directional, and interior fill split at `render.rs:782` +
  `triangle.frag:1321` were complete from prior sessions).

Net: tests unchanged at 1038. LOC +470 total (M33.1 implementation).
Bench-of-record: Prospector 192.8 FPS / 5.19 ms at `e6e8091` (wall-clock).

---

## Session 14 — M33 cloud layer 1 + RT glass  (2026-04-22, 1622d61..f7f2819)

M33 sky & atmosphere had one open piece: cloud layer 1 (CNAM) was parsed
but not yet wired into the render path. Closing that gap finished M33
proper. With the milestone done, the session pivoted to RT glass — a
self-contained feature chain covering refraction geometry, Fresnel-path
specular with a blurred world sample, IGN roughness spread, and the ray
budget SSBO that gates Phase-3 glass quality without blowing frame budget.

- **M33 completion** — CNAM cloud layer 1 wired through ECS + renderer
  structs + `CompositeParams` UBO + composite shader (`1622d61`,
  `5d6e0e7`). M33 known-issue entry closed and moved to Completed
  Milestones; M33.1 (weather transitions + layers 2/3) promoted as
  follow-up (`d5db683`).
- **RT glass — geometry & shading** — refraction ray fires along the
  geometric normal (not shading normal) with IGN-sampled roughness spread
  (`f5605af`); Fresnel path blurs the refracted world sample and adds
  smooth specular (`849bccc`); mip-4 bulk-colour fill eliminates the
  ribbing artefact on all glass tiers (`f7f2819`).
- **Ray budget SSBO** — per-FIF atomic counter on binding 11 with CPU
  reset (`6f70872`); footprint-LOD ray tiers drive Phase-3 glass
  conditional quality (`c6da807`); `fragmentStoresAndAtomics` device
  feature enabled to allow SSBO atomics in the fragment stage
  (`bc9ebc7`).
- **Perf regression fix** — Tier C glass Phase-3 path caused a 29 FPS
  collapse; reverted while the ray-budget counter was plumbed, then
  re-enabled once the SSBO gating was in place (`ad88244`, `c6da807`).

Net: 924 → 1038 tests (+114). LOC ~91 300 → ~91 450 non-test.

---

## Session 13 — FO3 / FNV / ECS audit closeout  (2026-04-21, ~25 issues)

The 2026-04 audit sweep landed at `docs/audits/AUDIT_FO3_2026-04-19.md`,
`AUDIT_FNV_2026-04-20.md`, and `AUDIT_ECS_2026-04-19.md`. Publish-then-fix
cycle drove this batch.

- **NIF parser correctness** — dedicated parsers for `WaterShaderProperty`,
  `TallGrassShaderProperty`, and `bhkSimpleShapePhantom` (`#474`, ended
  their 24-byte over-read / 8-byte trailer drop); positive XYZ rotation
  regression test for `NiTransformData` (`#436` premise was stale; safety
  net added); boundary tests pinning `num_decals` at `texture_count ==
  8/9/6/7` (`#484` — locks the `#400`/`#429` fix against future
  rewrites); `MaterialInfo.diffuse_color` cached so
  `extract_vertex_colors` stops re-walking the property list (`#438` — 3×
  scan → 1× per `NiTriShape`).
- **BSA correctness** — `genhash_file` high-word now matches BSArch
  reference (`#449` — `rolling(ext)` from 0 then `wrapping_add` to
  `stem_high`, was sequential fold). Verified against the real FNV
  `glover.nif` stored hash; ~125k validation warnings per archive open
  silenced.
- **ESM coverage** — `PACK` / `QUST` / `DIAL` / `MESG` / `PERK` / `SPEL` /
  `MGEF` stub records (`#446`, `#447`) following the `#458` pattern (EDID
  + FULL + key scalars/refs, no deep decoding). Every dangling PKID /
  SCRI / QSTI / spell-effect ref now resolves. Live FNV vanilla: PACK =
  4163, QUST = 436, DIAL = 18215, MESG = 1144, PERK = 176, SPEL = 270,
  MGEF = 289 (total 13 684 → 62 219). FO3: 20 334 → 31 101. `CLMT`
  `WLST` `chance` retyped `i32`, consumer filters negatives (`#476`).
- **ECS** — `try_resource_2_mut<A, B>` with TypeId-sorted acquisition
  preserved (`#465` — sibling of `try_resource_mut`). Transform
  propagation buffer flipped from `Vec` (LIFO/DFS) to `VecDeque`
  (FIFO/BFS) so the variable name and "BFS" doc comments are now
  accurate (`#464`).
- **Test infrastructure** — `parse_real_nifs.rs` `MIN_SUCCESS_RATE`
  raised 0.95 → 1.0 (`#487` — single-NIF vanilla regressions now fail CI
  loud); `nif_stats` exit code matches with `NIF_STATS_MIN_SUCCESS_RATE`
  env var override for modded content. New `parse_real_esm.rs` pins FNV
  total ≥ 60 000 + per-category floors for the 7 new types (`#488`).
- **Performance baselines** — Prospector Saloon re-benched headless at
  commit `bee6d48`: **avg 251.6 FPS / 3.97 ms** on RTX 4070 Ti, 1200
  entities, 777 meshes, 208 textures, 773 draws (vs the stale ROADMAP
  claims of 48 / 85 FPS + 809 entities / 199 textures). M31.5 RIS + M36
  BLAS compaction + M37 SVGF + M37.5 TAA collectively cut frametime ~5×
  while post-M18 record expansion added ~48% more entity coverage
  (`#489`).
- **Issues closed as stale** — `#411` (FO4 BGSM scope too large — split
  into `#490`–`#494`), `#436` (XYZ premise wrong — added test), `#437`
  (GameVariant enum already exists as `NifVariant` — raw bsver checks
  are deliberate per `#160`/`#323`), `#473` (caustic doesn't enter TAA
  AABB — separate-image audit misread), `#480` (truncated comment was a
  hard wrap; auditor only read one line).
- **Stale doc fix** — `composite::rebind_hdr_views` no longer claims TAA
  "isn't wired up" (`#472`); TAA shipped in M37.5 and is invoked from
  both init + resize.

Net: workspace test count 867 → 924. niftools/nifxml cloned to
`/mnt/data/src/reference/nifxml/nif.xml` for authoritative format
verification.

---

## Session 12 — Audit bundle #306–#463 closeout  (2026-04-20, 37 commits)

Renderer validation hygiene, Oblivion/FO4-era ESM coverage, and NIF
shader plumbing completeness.

- **NIF shader + texture plumbing** — BSShaderTextureSet parallax + env
  slots routed to `GpuInstance` with POM gating (`#453`);
  BSShaderPPLightingProperty and BSLightingShaderProperty read slots
  3/4/5 (`#452`); BGEM `material_path` captured on both `NiTriShape`
  and `BsTriShape` via BSEffectShaderProperty (`#434`); `ShaderTypeData`
  payload surfaced on `ImportedMesh` for both trishape variants
  (`#430`); dedicated `TileShaderProperty` parser + unified decal flags
  across properties (`#454`/`#455`); `SF_DOUBLE_SIDED` no longer
  propagates through FO3/FNV BSShader* paths (`#441`);
  `BSGeometryDataFlags` decoded on Bethesda NiGeometryData (`#440`);
  `BSShader*Controller` preserves the controlled-variable enum
  (`#350`); `NiExtraData` version gating (`#329` + `#330`);
  `NiZBufferProperty` z_test/z_write/z_function plumbed through
  extended dynamic state (`#398`); NiTexturingProperty glow/detail/gloss
  slots wired to the fragment shader (`#399`); FO76 BSTriShape Bound
  Min Max AABB consumed (`#342`); `NiBlend*Interpolator` indirection
  resolved in animation import (`#334`); Shepperd quaternion fast-path
  renormalised (`#333`); `BSAnimNote` / `BSAnimNotes` parsed and IK
  hints surfaced on `AnimationClip` (`#432`); Oblivion KF import +
  decal slot off-by-one (`#400` + `#402`); stream-derived
  `Vec::with_capacity` sweep through `allocate_vec` (`#408`).
- **ESM parser** — `SCPT` pre-Papyrus bytecode records parsed (`#443`);
  `CREA` + `LVLC` groups dispatched in `parse_esm` (`#442` + `#448`);
  Oblivion CREA indexed and `ACRE` placements recognised (`#396`); FO4
  NIF `HEDR` → `GameKind` bands corrected for FO3 and FO4 (`#439`);
  worldspace auto-pick + FormID mod-index remap when loading cells by
  editor ID (`#444` + `#445`); `CLMT` `TNAM` sunrise/sunset/volatility
  hours threaded through `weather_system` (`#463`); Skyrim `XCLL`
  directional-ambient cube + specular + fresnel extracted (`#367`);
  FNV `LAND` parse failure demoted warn → debug with error context
  forwarding (`#385`).
- **Renderer validation + correctness** — SPIR-V reflection cross-checks
  every descriptor-set layout against shader declarations at pipeline
  create time (`#427`); bindless texture array sized from device limit
  with an `Err` return on overflow (`#425`); `R32_UINT` causticTex
  sampler switched to NEAREST (VUID-vkCmdDraw-magFilter-04553); window
  portal ray fires along `-N` instead of `-V` (`#421`); TLAS
  `instance_custom_index` unified with SSBO position via a shared map
  (`#419`); fog moved from `triangle.frag` to `composite.frag` — kills
  SVGF ghosting on heavy fog (`#428`); grow-only scratch pool applied
  to the TLAS full-rebuild path (`#424` SIBLING); draw-command depth
  sort key switched to IEEE 754 total-ordering (`#306`).

Net: workspace test count 770+ → 867. Net source growth ~75K → ~81K
lines of Rust across 188 source files.

---

## Session 11 — Audit bundle #341–#438 bug-bash  (2026-04-18, 72 commits)

- **Parser correctness** — Oblivion v20.0.0.5 stability: runtime size
  cache, stream drift detector, v20.2.0.5+ parallax gate.
- **Import path correctness** — normal-map routing,
  NiDynamicEffect affected_nodes, material_kind, BSDynamicTriShape
  vertex extraction, all-8 TXST slots, VMAD has_script.
- **NIF import cache promoted to process-lifetime resource** (`#381`).
- **Sync/cache hardening** — VkPipelineCache plumbed through every
  create site; per-(src, dst, two_sided) blend pipeline cache; TLAS
  build barrier widened; TRIANGLE_FACING_CULL_DISABLE gated on
  two_sided; gl_RayFlagsTerminateOnFirstHitEXT on reflection + glass
  rays.

Net: test count 623 → 770+. Net source ~64K → ~75K.

---

## Session 10 — Shadow pipeline overhaul + TAA + BLAS compaction + FO4 architecture

Renderer-quality push that retired the largest remaining visual
regressions and shipped three renderer milestones (M31.5 streaming RIS,
M36 BLAS compaction, M37.5 TAA). Audit bundle `#314`–`#340`.

- **Streaming RIS (M31.5)** — replaced deterministic top-K shadow
  pipeline with 8 independent weighted reservoirs per fragment, each
  sampled from the full light cluster proportional to luminance. Every
  light now has non-zero shadow probability — fixes the "large
  occluder never shadows large receiver" pathology the top-K pipeline
  hit on big overhead lamps. Unbiased weight `W = resWSum / (K ·
  w_sel)`, clamped at 64× to tame fireflies. Directional sun angular
  radius tightened 0.05 → 0.0047 rad (physically correct).
- **TAA (M37.5) — `taa.comp` + `TaaPipeline`** — Halton(2,3) sub-pixel
  projection jitter applied in the vertex shader; motion vectors stay
  un-jittered for correct reprojection. Motion-vector reprojection
  with Catmull-Rom 9-tap history resample. 3×3 YCoCg neighborhood
  variance clamp (γ = 1.25). mesh_id disocclusion detection. Luma-
  weighted α = 0.1 history blend. Per-FIF RGBA16F history images,
  ping-pong descriptor sets, first-frame guard, resize hooks. Camera
  UBO extended with `vec4 jitter` (all 4 shader UBO layouts updated
  in lockstep). Composite's HDR binding rewired to the TAA output via
  `rebind_hdr_views()`.
- **BLAS compaction (M36)** — `ALLOW_COMPACTION` flag on BLAS build,
  async occupancy query, compact copy allocated at exact size,
  original BLAS destroyed via `deferred_destroy`. 20–50% BLAS memory
  reduction on typical cells.
- **FO4 architecture** — `asset_provider` auto-detects BSA vs BA2 from
  file magic at open time. ESM parser extended with `SCOL`, `MOVS`,
  `PKIN`, `TXST`. `BSLightingShaderProperty.net.name` flows through
  `ImportedMesh` → `Material.material_path`.
- **Debug CLI** — console commands `tex.missing`, `tex.loaded`,
  `mesh.info <entity_id>`; evaluator functions `tex_missing()` /
  `tex_loaded()` over TCP; `mesh.info` shows BGSM reference when
  `texture_path` is absent.
- **NIF parser fixes (`#322`–`#325`, `#340`)** — `#322` NiPSysData
  over-reads respect BS202 zero-array rule; `#323` NiMaterialProperty
  variant mapping check file `BSVER` directly, not `NifVariant`;
  `#324` Oblivion runtime size cache prevents cascading parse failure
  after a single bad block; `#325` NiGeometryData `Has UV` only read
  until 4.0.0.2; `#340` pre-intern animation channel names as
  `FixedString` at clip load so the per-frame sampler hot path never
  touches the `StringPool` lock.
- **Reflection + metal quality (`#315`, `#320`)** — route metal
  reflection into the direct path to avoid albedo double-modulation;
  exponential distance falloff on reflection rays plus roughness-driven
  angular jitter.

Net: test count 472 → 623. Zero new warnings.

---

## Session 8 — Papyrus parser, RT performance, landscape, exterior sun  (35 commits)

- **M30 Phase 1** — Papyrus language parser (logos lexer + Pratt
  expression parser, 45 tests).
- **M31** — RT performance at scale (batched BLAS builds, TLAS
  culling, importance-sorted shadow budget, distance-based ray
  fallback, GI hit simplification, BLAS LRU eviction, deferred SSBO
  rebuild).
- **M32 Phase 1+2** — landscape terrain from LAND heightmap records
  with LTEX/TXST texture splatting.
- **M34 Phase 1** — default exterior sun for directional lighting.
- **Fix #251–#284 bundle** — alpha test function extraction (#263),
  dark texture import (#264), instanced draw batching (#272), shadow
  ray budget (#270), subtree cache persistence (#278), Vulkan sync
  fixes (#280–#284), NIF string read optimization (#254), animation
  scratch buffers (#251–#252), performance bundle (#279).
- **Roadmap reprioritization** to renderer-first with M32–M48 tiered
  plan.

---

## Session 7 — Starfield BA2 v3 + LZ4 block decompression

BA2 v3 header has a 12-byte extension (not 8) with a
`compression_method` field; LZ4 block decompression via
`lz4_flex::block`. Verified against 22 Starfield texture archives
(~128K DX10 textures) + 53 vanilla FO4 BA2s (v1/v7/v8), zero failures.
BA2 support verified end-to-end for every version/variant.

---

## Session 6 — N26 closeout + skinning end-to-end + Oblivion parser fix  (35 commits)

Long bug-bash that closed out 26 GitHub issues and tracked down a
long-standing Oblivion parser regression.

**Skeletal skinning, end-to-end (#178)**

- Part A (`923d11b`) — new `SkinnedMesh` ECS component with
  `compute_palette()` pure function. Scene assembly resolves
  `ImportedSkin.bones[].name` → `EntityId` via a name map built during
  NIF node spawn. 8 unit tests cover the palette math.
- Part B (`4c97a36`) — GPU side. Vertex format extended with
  `bone_indices: [u32; 4]` + `bone_weights: [f32; 4]` (44 → 76 B,
  6 attribute descriptions). New 4096-slot bone-palette SSBO on scene
  set 1 binding 3. Push constants 128 → 132 B (`uint bone_offset`).
  Single unified vertex shader — rigid vertices tag themselves with
  `sum(weights) ≈ 0` and route through `pc.model`, skinned vertices
  blend 4 palette entries via `bone_offset + inBoneIndices[i]`.

**N26 dispatch closeout — every "block silently dropped" issue closed**

- `#157` BSDynamicTriShape + BSLODTriShape, `#147` BSMeshLODTriShape +
  BSSubIndexTriShape, `#146` BSSegmentedTriShape, `#148` BSMultiBoundNode,
  `#159` BSTreeNode, `#158` BSPackedCombined[Shared]GeomDataExtra,
  `#150` `as_ni_node` walker helper, `#160` raw `bsver()` for
  non-Bethesda Gamebryo, `#175` `NifScene.truncated`.

**Critical Oblivion parser regression (`afab3e7`)**

- New `crates/nif/examples/trace_block.rs` dumps per-block start
  positions + 64-byte hex peeks. Used to bisect the runtime
  `NiSourceTexture: failed to fill whole buffer` spam on Oblivion cell
  loads.
- Root cause — earlier fix `#149` had added a `Has Shader Textures:
  bool` gate on `NiTexturingProperty`'s shader-map trailer based on
  `nif.xml`. The authoritative Gamebryo 2.3 source reads the count as
  a `uint` directly. The bool gate consumed the first byte of the
  u32, leaving the parser 3 bytes short. On Oblivion (no per-block
  size to recover) this misaligned the following NiSourceTexture's
  filename length, which then read garbage as a u32 ≈ 33 M and bled
  through the rest of the file.
- Reverted the bool gate. All ~80 unique Oblivion clutter / book /
  furniture meshes that were previously truncating now parse to
  completion. Visual confirmation: Anvil Heinrich Oaken Halls renders
  fully populated.

**Quality + correctness fixes** — `#137` lock_tracker RAII scope guards;
`#136` 16× anisotropic filtering; `#134` frame-counter-based deferred
texture destruction; `#152` NiAlphaProperty alpha-test bit;
`#131` NiTexturingProperty `bump_texture` as Oblivion normal map;
`#155` NiBSpline* compressed animation family; `#151` + `#177` skinning
data extraction; `#79` binary KFM parser; `#108` BSConnectPoint::Children
skinned flag byte; `#127` bhkRigidBody body_flags threshold 76 → 83;
`#172` NIF string-table version threshold aligned to 20.1.0.1;
`#50` per-draw vertex/index buffer rebind dedup; `#36` World::spawn
panics on EntityId overflow; cell loader `CachedNifImport` Arc cache.

Net: test count 396 → 472. Zero new warnings.

---

## Sessions 1–5 — Foundational work

Not narrated here; see milestone M1–M22 table in ROADMAP.md and the
commit log on `main` for day-to-day history of the Vulkan init chain,
ECS foundation, NIF parser bring-up, ESM parser, cell loading, and the
M22 RT multi-light system.
