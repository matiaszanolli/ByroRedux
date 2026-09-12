# Skyrim SE Compatibility Audit — 2026-09-11

**Scope**: `/audit-skyrim` — regression coverage + Skyrim-specific geometry/shader/equip
risk on top of the renderer control bench (Whiterun BanneredMare). 7 dimensions,
each run as an isolated Task agent against live source + (where applicable) real
on-disk Skyrim SE data at
`/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/`.

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 200
--json number,title,state,labels` fetched fresh at audit start
(`/tmp/audit/issues.json`); individual dimensions additionally pulled wider
`--state all` slices where noted.

---

## Executive Summary

Skyrim SE is ByroRedux's renderer **control bench**: both loose-mesh and
full-cell rendering already work (Whiterun BanneredMare, 6 equipped NPCs via
M41 OTFT/LVLI), so this audit is regression coverage against known-good code
plus the genuinely Skyrim-specific risk surface named in the skill (BSTriShape
packed geometry, `BSLightingShaderProperty` shader-type dispatch, NPC
equip/FaceGen, multi-master load order).

**Result: the core pipeline is solid.** Six of seven dimensions found either
zero defects or LOW-only hardening gaps. BSTriShape packed-geometry decode,
IEEE-754 half-float conversion, the SSE skinned-reconstruction tangent/axis
convention, the full Skyrim/FO4/FO76 shader-type dispatch table, the NPC
equip/FaceGen pipeline (race-skin-first + post-loop occupancy filter +
partition-level hiding), multi-master FormID remap + ESL decode + deleted-REFR
tombstones, and the BSA v105 LZ4-frame codec were all verified field-for-field
against `nif.xml` / real game data and found correct, several with fresh test
or extraction runs performed this session (92/92 `byroredux-bsa` unit tests,
65,637-file zero-error extraction sweep, real-`Skyrim.esm` cell-load tests).

**Two HIGH findings were confirmed**, both new-this-session:
- A GPU texture-handle leak in the `.bto` distant object-LOD spawner
  (Dimension 6) — every non-atlas per-sub-mesh texture resolved since #3412
  is acquired but never released on unload, pinning VRAM/descriptor slots for
  the session.
- A NIFAL canonical-boundary violation in the glass-material classifier
  (Dimension 7) — `classify_glass_into_material` only protects
  engine-synthesized `material_kind` values (`>= 100`); every authored
  low-range Skyrim `BSLightingShaderProperty.shader_type` (MultiLayerParallax
  ice, EnvironmentMap mirrors, Glow gems) is silently overwritten by a bare
  keyword match, contradicting the function's own doc comment.

One MEDIUM (the structural root cause enabling the HIGH above: no canonical
field carries the authored shader type once `material_kind` is repurposed as
dispatch) and six LOW findings round out the report — mostly test-gap and
doc-rot items, plus one unauthored-default inconsistency in
`BSEffectShaderProperty.env_map_scale`.

**Total: 9 findings** — 0 CRITICAL, 2 HIGH, 1 MEDIUM, 6 LOW.

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

**Result: 1 LOW.** All four checklist items (VF_* flag bits + half-float
decode, flag-combination coverage + index stride + skin extraction, SSE
tangent/axis-convention regression guard, alpha-property cascade) verified
**correct** field-by-field against `nif.xml` and the existing regression
tests. Five candidate findings were drafted during the pass and disproved by
further reading (recorded in the dimension file so they aren't re-derived).

- **SKY-D1-2026-09-11-01** (LOW) — `BSTriShape`'s particle-data trailing read
  is gated on `bsver < FALLOUT4` (a broad range) where `nif.xml` gates the
  field on `#BS_SSE#` (BSVER exactly 100). No vanilla content reaches the gap
  (Skyrim LE ships `NiTriShape`, not `BSTriShape`), so blast radius is limited
  to modded/backported/synthetic geometry with a mis-detected BSVER header.
  `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:659-668`.

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch

**Result: 3 LOW.** All six checklist items verified clean: every numeric
Skyrim/FO4 shader type dispatches to the correct `ShaderTypeData` arm with the
right trailing-field count; the FO76 `BSShaderType155` enum cannot
cross-contaminate the Skyrim/FO4 tables (two independent boundaries enforce
this); Skyrim SLSF1/SLSF2 flag bits match `nif.xml` bit-for-bit; the #1241 PBR
scalar chain reaches `ImportedMesh` intact; and the Disney/Burley `pbr.glsl`
lobe is **provably unreachable** for vanilla Skyrim because the NIF import
path hardcodes `is_pbr: false` — only an external BGSM/BGEM merge can set it.

- **SKY-2026-09-11-D2-01** (LOW) — Skyrim's `BSEffectShaderProperty` import
  path constructs `env_map_scale = 0.0` for the absent-on-Skyrim field (BSVER
  < 130), then copies it unconditionally onto `MaterialInfo`, whose own
  declared default is `1.0` — three sites in one pipeline disagree on what
  "no value" means. Currently latent (affected meshes are tagged
  `material_kind = 101`, short-circuiting lit shading before the field is
  read). `crates/nif/src/import/material/dedicated_shader.rs:578`.
- **SKY-2026-09-11-D2-02** (LOW) — The #2328 `env_map_scale_consumed` latch
  protecting a dedicated shader's value from being clobbered by a later
  legacy property is only honoured by the legacy writers; the two Skyrim+
  dedicated writers (`shader_data.rs` EnvironmentMap arm,
  `dedicated_shader.rs` effect-shader path) never set the latch. Same residual
  shape as the already-closed #3514/#3517 for `refraction_strength`/
  `texture_clamp_mode`. Currently unreachable on vanilla Skyrim content.
- **SKY-2026-09-11-D2-03** (LOW) — Test-coverage gap: only `shader_type = 0`
  of the thirteen no-trailing-data Skyrim shader types has a wire-level
  byte-position assertion pinning the `None` fallthrough arm. No live defect;
  a missing tripwire on a dispatch table whose failure mode is exactly the
  class of silent byte-drift bug that took four prior audits (#455/#474/#550/
  #713/#717) to catch.

### Dimension 3 — NPC Equip + FaceGen (M41)

**Result: 0 findings.** All five checklist items confirmed matching current
code exactly: the six Bannered Mare NPCs resolve full equip state via
production insertion (not test-only scaffolding), verified against real
`Skyrim.esm`; the race-skin-first + post-loop occupancy filter (#2093/#2094)
plus the finer-grained per-triangle `hidden_biped_mask` for skin and FaceGen
head (#3408/#3409) are implemented exactly as described; `expand_leveled_form_id`'s
single-/multi-pick LVLI split matches the post-#3217 fix; FaceGen heads parse
via `BSDynamicTriShape` with confirmed **no** runtime morph call on the
Skyrim/Fallout4/76/Starfield path (`uses_prebaked_facegen()`); and
`BSDismemberSkinInstance` partition data correctly feeds both classic and SSE
skin extraction. One test-fidelity observation (not filed) noted for a future
editor's benefit.

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression

**Result: 0 new findings.** All nine checklist items traced to working code
with passing tests, including tests **run fresh this session** against real
`Skyrim.esm`: `parse_real_skyrim_esm` (590 cells, 18,244 statics, 37
worldspaces, Winking Skeever resolves with 981 refs and populated extended
lighting), the real-load-order STRINGS test, and the full `esm::cell` +
`load_order` suites (165+13 passed, 0 failed). Repeatable `--master` remap,
ESL/light-master FormID decode, and both tiers of deleted-REFR tombstone
handling (placement-level + base-record-level) all verified correct with no
doc-rot reintroduced. Two pre-existing OPEN issues (#4072, #4073) on the
STRINGS mechanism itself intersect this dimension's checklist but are
parser-mechanism-level, owned by `/audit-esm`, and not re-filed here. The
control-bench guard (item 9) could not be physically re-run in this sandbox
(no Vulkan device) — flagged incomplete, not failing; no code-path reason
found to expect drift from the recorded 5,765-entity/93.1 FPS baseline across
the 64 intervening commits (all ESM/plugin correctness fixes unrelated to
Skyrim record counts).

### Dimension 5 — BSA v105 (LZ4)

**Result: 0 findings.** Clean pass, verified with fresh extraction runs
against the full real Skyrim SE corpus (not just code reading): 65,637 files
across all 11 vanilla v105 archives, 0 errors, 0 magic mismatches; the
checked-in real-data regression tests (`bsa_real.rs`, 18,862 NIFs, 0 errors);
and the full `byroredux-bsa` unit suite (92 passed, 0 failed, 11 ignored). The
v105 codec dispatch is confirmed LZ4 **frame** (`lz4_flex::frame::FrameDecoder`),
pinned against a block-codec regression by an unconditional synthetic test
(#1558/SK-D5-01). Folder record layout, embedded-name flag gating, and the
per-file compression-toggle XOR semantics vs. archive default all match
reference behavior. Zero-based sibling auto-load (`Textures0` → `…1`–`…9`)
is unnarrowed and independently corroborated by the sweep. A prior LOW finding
from the 2026-09-05 audit (no present-only AE/Creation-Club tier) is confirmed
fixed (#3924) and not re-reported.

### Dimension 6 — Specialty Blocks + Real-Data Rendering

**Result: 1 HIGH, 2 LOW.**

- **SKY-2026-09-11-D6-01** (HIGH) — `.bto` object-LOD sub-mesh textures are
  acquired (`resolve_texture`, refcounted) but never released:
  `ObjectLodBlock` has exactly one texture field (the shared worldspace
  atlas), documented as the only handle dropped on unload, while #3412 added
  per-sub-mesh texture resolution for the ~34% of vanilla bindings that name a
  distinct (non-atlas) texture — none of those handles are ever released, in
  both `unload_object_lod_block` and the `entities.is_empty()` early return.
  This is the same leak shape #1537 and #2758 already closed on the sibling
  terrain-LOD and early-return paths; #3412 reopened it on the object-LOD
  path. Accrues on every quad load across the level-4/8/16 object-LOD ring
  during exterior traversal; never reclaimed; pins VRAM + bindless descriptor
  slots against `TextureRegistry` LRU eviction.
  `byroredux/src/cell_loader/object_lod.rs:65-79,348-431,473-481,490-505`.
- **SKY-2026-09-11-D6-02** (LOW) — `ROADMAP.md`'s prose "Parser coverage"
  summary (line ~519) contradicts its own compatibility matrix (~165 lines
  below) on three numbers: 184,886 vs. the matrix's 603,207 total files (the
  matrix explicitly names 184,886 as superseded by #3369/#3466), and stale
  100%/FO76-clean/Starfield-99.99% claims vs. the matrix's current
  FO76-98.18%-with-truncation-tail and Starfield-99.98%. Documentation-only,
  but it's precisely the premise a future audit would cite.
- **SKY-2026-09-11-D6-03** (LOW) — `BSTreeNode` SpeedTree wind-bone lists
  parse correctly and reach `ImportedNode.tree_bones`, but nothing downstream
  of the NIF import tier reads the field (grep confirms the only occurrence
  outside `crates/nif` is a `None` test fixture). Not a defect — the canopies
  render statically and the data terminates at the NIFAL boundary by design
  today — but recorded so a future audit doesn't re-derive "the parser
  doesn't support this" when it does.

Checklist items 1 (BSLODTriShape/BSMeshLODTriShape/BSSubIndexTriShape
dispatch, #838 regression guard), 2 (BsLagBoneController/
BsProceduralLightningController dedicated parsers, #837), 3 (node unwrapping +
BSPackedCombined*GeomDataExtra), 4 (`.btr` distant-terrain LOD + sibling-archive
texture resolution, correctly releases both texture refs unlike the `.bto`
path), 6 (multi-band `LodBandLadder` selection + hysteresis, cross-checked
against vanilla `Ultra.ini` `[TerrainManager]` distances), and 7 (VWD
full-model culling confirmed still-unbuilt forward scope, not a regression)
all verified clean. Item 8 (real-data render trace) verified at the
translate-boundary code level only; no smoke render was attempted this
session (per the standing no-parallel-engine-launch instruction) and the
33,424-file/100% Skyrim SE mesh-sweep figure (item 5) was not independently
re-measured.

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice)

**Result: 1 HIGH, 1 MEDIUM.** All four checklist items verified PASS: single
canonical boundary (`translate_material`, four production call sites, no
second path); `Material::classify_pbr` is genuinely deleted (no definition,
no call site, guarded by a workspace-hygiene source-text test); `resolve_pbr()`
correctly runs before `classify_glass_into_material` so forced-glass
overwrite wins; Skyrim `emissive_multiple` correctly routes through
`EmissiveSource::Lighting` (gated on authored-non-default) rather than
`Effect`.

- **SKY-D7-2026-09-11-01** (HIGH) — `Material.material_kind` is an overloaded
  union: low range `0..=20` is the verbatim authored Skyrim
  `BSLightingShaderProperty.shader_type`; `>= 100` is engine-synthesized.
  `classify_glass_into_material` protects only the synthesized range
  (`material_kind >= 100`) — every authored low-range shader type falls
  straight through to a bare keyword+coverage+metalness heuristic promotion
  to `MATERIAL_KIND_GLASS`, with no way back. Confirmed reachable by two real
  Skyrim populations: `MultiLayerParallax` (11) on layered ice surfaces
  (`icefrozen01`/`icecavewall01`/`icelakesurface`, widened into keyword
  reachability by #3359) loses its inner-layer-parallax dispatch in
  `triangle.frag` and takes flat glass refraction instead; and glowing soul
  gems (`gem` keyword) lose their glow dispatch to flat glass — the exact bug
  shape #2710 already fixed for the effect-shader carrier but not for the lit
  carrier. The `is_mirror_pane` arm additionally hard-zeroes `material_kind`
  unconditionally, destroying an authored `EnvironmentMap` (1) dispatch. The
  function's own doc comment (`helpers.rs:59-61`) asserts a keyword alone
  cannot override an authored shader type — true only for the effect-shader
  carrier, not the lit carrier this bug affects.
  `byroredux/src/helpers.rs:99-135`, called from
  `byroredux/src/material_translate.rs:649`.
- **SKY-D7-2026-09-11-02** (MEDIUM) — Structural root cause of the HIGH
  above: `ImportedMaterial.shader_type` never crosses the NIFAL boundary into
  `Material`. Only the per-variant *payload* (`shader_type_fields`) crosses;
  the discriminator that identifies which variant it belongs to does not, so
  once `material_kind` (seeded from the same raw value) is reassigned by the
  glass classifier, no canonical field retains the original authored
  provenance — forcing at least one downstream consumer
  (`TextureSlotContext` at cell-spawn) to read back into the raw
  `ImportedMaterial` tier directly, a NIFAL single-boundary violation in the
  making. `crates/nif/src/import/types.rs:760`,
  `byroredux/src/material_translate.rs:486-647`.

Both findings share one root cause and one suggested-fix direction: add an
explicit canonical `source_shader_type` field distinct from the
engine-dispatch `material_kind`, and gate `classify_glass_into_material`'s
override on the same external-material provenance check the effect-shader
carrier already requires. Content-side reachability (how many vanilla draws
actually hit the overwrite) was not measured this session — a BSA-wide BSLSP
scan or `--bench-hold`/`byro-dbg` census would size it.

---

## Shader-Type Coverage Matrix

`ShaderTypeData` variants (`crates/nif/src/blocks/shader.rs`) × parse /
import / render completeness for Skyrim (BSVER 83/100):

| Variant | Numeric type(s) | Parse | Import (→ `ImportedMesh`) | Render (`triangle.frag` / RT) |
|---|---|---|---|---|
| `None` | 0,2,3,4,8,9,10,12,13,15,17,18,19,20 | Complete (zero-byte, verified no over-read) | N/A | Default-lit |
| `EnvironmentMap` | 1 | Complete | Complete (`env_map_scale`) | Consumed, but **at risk of being overwritten to `material_kind=0` by `is_mirror_pane`** (SKY-D7-01) |
| `SkinTint` | 5 | Complete | Complete | Consumed |
| `HairTint` | 6 | Complete | Complete | Consumed |
| `ParallaxOcc` | 7 | Complete | Complete | Consumed |
| `MultiLayerParallax` | 11 | Complete | Complete (payload fields survive) | **Dispatch reachable only if `material_kind` survives the glass classifier — confirmed lost on ice-keyword assets (SKY-D7-01)** |
| `SparkleSnow` | 14 | Complete | Complete | Consumed |
| `EyeEnvmap` | 16 | Complete | Complete | Consumed |
| `Fo76SkinTint` | (FO76 numeric 4, distinct table) | N/A for Skyrim (separate `parse_shader_type_data_fo76`, structurally unreachable from Skyrim BSVER) | — | — |

Test-coverage note: only shader_type 0 of the fourteen `None`-mapped values
has a wire-level byte-position pin (SKY-2026-09-11-D2-03, LOW).

---

## Cell-Load Regression Status

- TES5 cells parse through the unified `esm/cell/` walker; compressed GRUPs
  decompress correctly (confirmed indirectly via real-`Skyrim.esm` record
  counts matching prior-session figures, not a collapse to near-zero).
  `parse_real_skyrim_esm` passes fresh this session: 590 cells, 18,244
  statics, 37 worldspaces; `SolitudeWinkingSkeever` resolves with 981 refs and
  populated extended lighting (ambient `[0.318, 0.294, 0.224]`, 92-byte XCLL
  on all 590/590 cells).
- Multi-master remap, ESL/light-master decode, and both tiers of
  deleted-REFR tombstone handling all verified correct against real data and
  the full `esm::cell`/`load_order` test suites (165+13 passed this session,
  0 failed).
- **Whiterun BanneredMare control-bench guard: INCOMPLETE, not FAILED.** The
  live ROADMAP bench-of-record (5,765 entities / 93.1 FPS TAA-native / 108.6
  FPS FSR-Quality, refreshed at commit `4c9a5b36`) could not be physically
  re-run in this sandbox (no Vulkan/display device available to any of the
  dimension agents). The 64 commits since that refresh were inspected and are
  all ESM/plugin correctness fixes (TPLT chain resolution, embedded-FormID
  remap guard, SCRI/CLMT.WLST global remap, terrain texture layer, DIAL
  remap) that do not touch BanneredMare's own record types or change which
  STAT/REFR/WEAP/ARMO/LIGH records resolve — no code-path reason was found to
  expect the recorded figures to have moved, but this was not empirically
  confirmed. A real `--bench-frames 300 --bench-hold` run against
  `WhiterunBanneredMare` is the only way to close this out.

---

## Total Findings: 9

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 2 |
| MEDIUM | 1 |
| LOW | 6 |

**By dimension:**

| Dimension | CRITICAL | HIGH | MEDIUM | LOW | Total |
|---|---|---|---|---|---|
| 1 — BSTriShape packed geometry | 0 | 0 | 0 | 1 | 1 |
| 2 — Shader-type dispatch | 0 | 0 | 0 | 3 | 3 |
| 3 — NPC equip + FaceGen | 0 | 0 | 0 | 0 | 0 |
| 4 — Multi-master load order | 0 | 0 | 0 | 0 | 0 |
| 5 — BSA v105 (LZ4) | 0 | 0 | 0 | 0 | 0 |
| 6 — Specialty blocks + rendering | 0 | 1 | 0 | 2 | 3 |
| 7 — NIFAL canonical material | 0 | 1 | 1 | 0 | 2 |
| **Total** | **0** | **2** | **1** | **6** | **9** |

**Most severe findings:**
- **SKY-2026-09-11-D6-01** (HIGH) — `.bto` distant object-LOD spawner leaks a
  GPU texture handle + bindless descriptor slot per distinct non-atlas
  sub-mesh texture on every unload, unbounded over a session's exterior
  traversal.
- **SKY-D7-2026-09-11-01** (HIGH) — the NIFAL glass-material classifier
  silently overwrites authored Skyrim `BSLightingShaderProperty` shader types
  (ice MultiLayerParallax, mirror EnvironmentMap, glowing gems) with
  `MATERIAL_KIND_GLASS`, contradicting its own no-override doc guarantee.

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-09-11.md
```

Label every finding `game:skyrim` + `legacy-compat`, plus its own domain
label (`renderer`/`memory` for D6-01; `nifal`/`import-pipeline` for the two
D7 findings; `nif-parser` for D1/D2's LOW findings; `doc-rot` for D6-02).
