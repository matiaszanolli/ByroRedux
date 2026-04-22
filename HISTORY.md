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
