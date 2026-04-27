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
