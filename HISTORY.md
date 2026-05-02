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
