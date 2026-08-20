# Legacy Compatibility Audit — 2026-08-20

**Base:** `bb0b92f2` · **Type:** full `/audit-legacy-compat` sweep, all 7 dimensions ·
**Run as part of** the `comprehensive` audit-suite sweep (25 audits).

## Scope

All seven dimensions were run: coordinate-system correctness (Z-up→Y-up), NIFAL
cross-layer mapping shape, the material translation boundary, PHYSAL's source
axis, EXAL/WATAL, per-game translation-survey patterns (A/B/C), and subsystem
coverage vs the legacy engines.

**Delta weighting.** 335 commits since 2026-08-16, overwhelmingly session-70
WATAL water work. This sweep therefore drove its effort at the water layer, per
the dispatch: *is WATAL's canonical model a genuine superset of what the seven
source engines authored, or does it quietly drop a game's semantics?* Five of
the six findings answer that question, and every one of them is backed by direct
scans of vanilla masters rather than by reading the code's own comments.

**Source-availability statement (read before weighing any Dimension 7 claim).**

| Reference | Status |
|---|---|
| Gamebryo 2.3 source (`/media/matias/Respaldo 2TB/…/Gamebryo_2.3/`) | **NOT MOUNTED** — same as the 2026-08-16 sweep. Not used. |
| `/mnt/data/src/reference/nifxml/nif.xml` | Available — used as the authoritative NIF spec (it takes precedence over the 2.3 source anyway). |
| `/mnt/data/src/reference/gamebryo-v26`, `gamebryo-v32`, `havok-2007…`, `havok-2013`, `openmw`, `nifly` | Available; not load-bearing for any finding below. |
| Vanilla masters: `Oblivion.esm`, `Fallout3.esm`, `FalloutNV.esm`, `Skyrim.esm`, `Fallout4.esm`, `SeventySix.esm`, `Starfield.esm` | **All seven available and all seven scanned.** This is the evidence base for LC-D6-01, LC-D6-02, LC-D6-03 and LC-D5-01. |

Nothing was skipped for lack of a reference; where the 2.3 runtime semantics
would have been the only way to settle a question, the question is not raised.

**Method.** Every claimed single-boundary contract was traced to its callers.
Every candidate finding was re-read against HEAD and then actively attacked
before being kept — two candidates died that way (see
[Disproved Candidates](#disproved-candidates)). Deduplicated against the 400
issues cached at `/tmp/audit/issues.json` (range #2671–#3103) and against
`docs/audits/`. No `cargo` command was run. No source file, game file, or GitHub
issue was modified.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 2 |
| LOW | 3 |
| **Total** | **6** |

All six are NEW. **All four of the previous sweep's findings were published,
fixed and closed** (#2992, #3096, #3097, #3098) and each fix was re-verified as
still in place at HEAD — no regressions.

The three canonical-translation layers remain structurally intact:
zero per-game branches downstream of any `translate()` boundary, zero bare
`bs_version` comparisons anywhere in the tree, one `(x, z, -y)` axis swap, one
populated-`Material` producer, no `game ==` branch in `extract_ragdoll`.
Dimensions 1–4 are clean.

**The water layer is where this sweep's yield is, and it is not a shape problem
— it is a fidelity problem.** WATAL's *structure* is correct (one ESM boundary,
two callers, no downstream branch). What it gets wrong is the per-game byte
semantics it feeds into that boundary, and it gets them wrong in a way the
layer's own doctrine was supposed to prevent: the same 20-byte wire block is
decoded by five sibling functions in one file, and three of the five are
misaligned by exactly one field. Because `resolve_water_material` then folds one
of those misread values into every canonical noise amplitude, the canonical
reference game — Skyrim, the game WATAL is explicitly modelled on — renders its
water ~20× flatter than authored on every vanilla record.

### Per-dimension finding counts (every dimension enumerated)

| Dimension | CRIT | HIGH | MED | LOW | Findings |
|---|---:|---:|---:|---:|---|
| 1. Coordinate-system correctness (Z-up→Y-up) | 0 | 0 | 0 | 0 | **none — clean** |
| 2. NIFAL — canonical NIF→ECS mapping shape | 0 | 0 | 1 | 0 | LC-D2-01 (mesh-water slice) |
| 3. Material translation boundary | 0 | 0 | 0 | 0 | **none — clean** |
| 4. PHYSAL — per-game Havok → solver (source axis) | 0 | 0 | 0 | 0 | **none — clean** |
| 5. EXAL / WATAL — exterior + water → renderer & solver | 0 | 0 | 1 | 1 | LC-D5-01, LC-D5-02 |
| 6. Per-game translation-survey patterns (A/B/C) | 0 | 1 | 0 | 2 | LC-D6-01, LC-D6-02, LC-D6-03 |
| 7. Subsystem coverage vs legacy | 0 | 0 | 0 | 0 | **none — clean** (all 3 prior findings closed & verified) |

---

## Dimension 1: Coordinate-system correctness (Z-up → Y-up)

**Findings: 0.**

Every 2026-08-16 result re-verified at HEAD after 335 commits:

- **Single `(x, z, -y)` producer.** A regex sweep for the swizzle over the whole
  tree returns exactly one production site,
  `crates/core/src/math/coord.rs:73` (inside `zup_to_yup_pos`). The only other
  two hits are in throwaway diagnostic example binaries whose filenames carry a
  `_tmp_` prefix (`crates/nif/examples/`) — not production, and not new.
- **No new bare `4096.0` cell math.** Every production `4096.0` is either
  `EXTERIOR_CELL_UNITS` (`crates/core/src/math/coord.rs:41`) or an unrelated
  quantity: a UV epsilon (`crates/physics/src/water.rs:287`, `1.0/4096.0`), the
  combustion light scale (`crates/renderer/src/shader_constants_data.rs:214`),
  or a `#[cfg(test)]` fixture. The COORD-3 collapse holds.
- **REFR Euler dispatcher.** Every production caller routes through
  `byroredux/src/cell_loader/euler.rs::euler_zup_to_quat_yup_refr` (re-exported
  at `byroredux/src/cell_loader.rs:109`); the two live call sites are
  `byroredux/src/cell_loader/placement_lod.rs:173` and the transition path. No
  caller hardcodes a rotation mode; no caller re-derives the ZYX product.
  `crates/nif/src/anim/keys.rs:125` still calls the shared core helper (the
  #2434 fix).

The water delta touched none of this.

---

## Dimension 2: NIFAL — canonical NIF→ECS translation contract (mapping shape)

**Findings: 1 (MEDIUM).**

**Downstream per-game-branch scan is clean.** `grep -rn "GameKind|bsver|NifVariant"`
over `crates/renderer/src`, `crates/core/src` and `crates/physics/src` returns
three hits, **all three of them assertion strings inside a shader-hygiene test**
(`crates/renderer/src/vulkan/volumetrics.rs:3008,3012,3049` — the test asserts
the GLSL contains no `GameKind`/`Fallout`/`Skyrim` token). Zero code branches
downstream of any boundary. This survived the entire session-70 water push,
which added a large amount of per-game water semantics — none of it leaked
downstream.

**Pattern A is clean at the source.** `grep -rnE "bs_version\s*(>=|<=|==|>|<)\s*[0-9]+"`
over `crates` + `byroredux` returns **0** non-test hits. The named-helper
discipline holds.

The one finding is in the newly-grown mesh-water slice.

### LC-D2-01: mesh-bound water gates `blend_normals` on a flag bit the NIF format does not define — every vanilla `BSWaterShaderProperty` mesh flips the canonical default to `false`

- **Severity**: MEDIUM
- **Dimension**: NIFAL — canonical NIF→ECS translation contract (mesh-water slice, new in session 70)
- **Location**: `byroredux/src/material_translate.rs:139-147` (the flag-gate block inside `water_material_from_mesh`); pinned by `byroredux/src/material_translate.rs:756-796`
- **Status**: NEW
- **Description**: `water_material_from_mesh` copies the parsed
  `BSWaterShaderProperty.water_shader_flags` word onto `WaterMaterial::shader_flags`
  and then, whenever that word is non-zero, decides
  `water.blend_normals = shader_flags & (1 << 16) != 0`. The authoritative NIF
  spec defines that word as the `WaterShaderPropertyFlags` bitfield, and it has
  **fourteen** options — bits 0 through 13 — with a format default of `0xC4`.
  Bit 16 is not part of the file-side enum, so it cannot be set by any authored
  NIF. The consequence is not "a flag is ignored": because
  `WaterMaterial::default().blend_normals` is `true`
  (`crates/core/src/ecs/components/water.rs:311`), the gate **inverts** the
  canonical default for every mesh whose flag word is non-zero — i.e. for every
  mesh that authored *anything*, including the vanilla default `0xC4`. The
  in-code comment attributes bits 15/16 to CommonLibSSE, which describes the
  *runtime* object, not the wire format the parser reads; the two enums are
  being conflated at the translate boundary.
- **Evidence**: `nif.xml` (`/mnt/data/src/reference/nifxml/nif.xml`), the
  authoritative spec, defines the field and its bitfield:
  ```xml
  <field name="Water Shader Flags" type="WaterShaderPropertyFlags" default="0xC4" />

  <bitflags name="WaterShaderPropertyFlags" storage="uint" prefix="BSWSP" versions="#SKY_AND_LATER#">
      bit 0 DISPLACEMENT   bit 1 LOD        bit 2 DEPTH       bit 3 ACTOR_IN_WATER
      bit 4 ACTOR_IN_WATER_IS_MOVING        bit 5 UNDERWATER  bit 6 REFLECTIONS
      bit 7 REFRACTIONS    bit 8 VERTEX_UV  bit 9 VERTEX_ALPHA_DEPTH
      bit 10 PROCEDURAL    bit 11 FOG       bit 12 UPDATE_CONSTANTS  bit 13 CUBEMAP
  </bitflags>
  ```
  The default `0xC4` is `DEPTH | REFLECTIONS | REFRACTIONS` — non-zero, so the
  gate fires; bit 16 is clear, so `blend_normals` becomes `false`. The parse
  chain is intact and does not truncate the word
  (`crates/nif/src/blocks/shader.rs:545` reads a full `u32`,
  `crates/nif/src/import/material/dedicated_shader.rs:619` forwards it,
  `byroredux/src/material_translate.rs:96` copies it), so the word reaching the
  gate is exactly the authored one. The two unit tests that "pin" the behaviour
  (`mesh_water_honors_authored_optical_flag_gates`,
  `mesh_water_honors_authored_reflection_and_blend_flags`) construct
  `water_shader_flags = 1 << 16` and `(1 << 6) | (1 << 16)` — values no authored
  NIF can produce — so the suite is green against synthetic input that vanilla
  data never supplies, and `assert!(!water.blend_normals)` at
  `material_translate.rs:789` locks in the wrong answer for the realistic case.

  Secondarily, the same block honours only bit 6. `DISPLACEMENT` (0),
  `DEPTH` (2), `REFRACTIONS` (7), `PROCEDURAL` (10), `FOG` (11) and
  `CUBEMAP` (13) are parsed and dropped — even though `docs/engine/watal.md` §6
  states the canonical type "still carries DISPLACEMENT/LOD/DEPTH/REFLECTIONS/
  REFRACTIONS flags so the per-game translate can disable rays for opaque
  waterfalls … without a shader per-game branch". That is precisely the case the
  spec calls out and the code does not implement: an opaque waterfall authored
  with `REFRACTIONS` clear still gets refraction rays.
- **Impact**: Every dedicated water mesh in Skyrim and later — waterfall sheets,
  cascade panels, mill races, the localized water NIFs the cell loader does not
  spawn as planes — loses authored multi-layer normal blending, because the
  canonical `true` is flipped to `false` by a bit that is structurally always
  clear. Visually this reads as flat, single-layer water on exactly the assets
  that need chop most. It is silent: no warning, and the test suite is green.
  Blast radius is bounded to mesh-bound water (cell/worldspace planes take the
  `env_translate` path, which sets `blend_normals` from the WATR `FNAM` byte and
  is correct), which is why this is MEDIUM and not HIGH.
- **Related**: LC-D5-02 (the same slice is an undeclared second `WaterMaterial`
  producer); `docs/engine/watal.md` §6.
- **Suggested Fix**: Replace the two ad-hoc constants with the spec's
  `WaterShaderPropertyFlags` bit names and decide `blend_normals` from a bit that
  exists in the file enum — or, if no file bit carries that meaning, drop the
  gate entirely and let the canonical `true` stand rather than inverting it from
  an always-clear bit. Rewrite the two tests to use the format default `0xC4` as
  the realistic case. While there, wire `REFRACTIONS` (bit 7) and `DEPTH` (bit 2)
  into the canonical flags so watal.md §6's opaque-waterfall claim becomes true.

---

## Dimension 3: Material translation boundary (NIFAL reference slice)

**Findings: 0.**

- `byroredux/src/material_translate.rs:279` (`translate_material`) remains the
  sole populated-`Material` producer. The only other production `Material {`
  literals are `byroredux/src/material_translate.rs:442`
  (`translate_texture_only_material`, cleared last sweep — it owns no scalar
  literals and routes through `resolve_pbr`) and the seven constructors in
  `byroredux/src/cornell.rs`, which are the self-contained `--cornell` RT
  reference harness with no game data. Everything else is `#[cfg(test)]`.
- The deleted `Option`-override + render-time `classify_pbr` path has not
  reappeared.
- Both regression guards hold: the three `EmissiveSource` variants still share
  one scale (no normalization introduced), and `NiFogProperty` remains the
  documented deliberate skip.

The water work did not touch this boundary — `water_material_from_mesh` lives in
the same module but produces a `WaterMaterial`, not a `Material`, and *consumes*
the canonical `Material` rather than producing one. Checked specifically and
cleared; the `Material` contract is unaffected. (The `WaterMaterial` half of that
is LC-D5-02.)

---

## Dimension 4: PHYSAL — per-game Havok articulation → solver (source axis)

**Findings: 0.**

- **Extract is still game-agnostic.** `grep -rn "GameKind|game =="` over
  `crates/nif/src/import/collision/` returns **zero** hits.
  `extract_ragdoll` still switches on `BhkConstraintData` only.
- The per-game seam is still only the constraint CInfo decode
  (`crates/nif/src/blocks/collision/constraints.rs`), with per-era byte
  advancement asserted in `bhk_constraint_tests.rs`.
- Both 2026-08-16 closures (PHYS-01 `is_t` propagation, PHYS-02 authored perp
  axis) are still in place.

Solver-end items belong to `/audit-physics` and were not duplicated: #2887,
#2888, #2889 and #3067 remain open there. The documented limitations (FO4+ /
FO76 / Starfield packed Havok, the cone+2-plane approximation, captured-but-
unused motors) were re-confirmed as limitations, not re-filed.

Note for the physics owner, not filed here: LC-D6-01 below changes what
`WaterFlow`/`WaterContact` are fed, but only the *visual* amplitude fields are
affected; `PhysicsWaterConstants` is engine-defined and unaffected.

---

## Dimension 5: EXAL / WATAL — per-game exterior environment → renderer & solver

**Findings: 2 (1 MEDIUM, 1 LOW).**

**Boundary shape re-verified and holding.** `resolve_water_material` has exactly
two production callers (`byroredux/src/cell_loader/water.rs:364` for the cell
plane, `:758` for the worldspace LOD plane); `default_water_for_worldspace` has
exactly one (`byroredux/src/cell_loader/exterior.rs:946`). No second
`SkyParamsRes` / `WeatherDataRes` / `CellLightingRes` construction site appeared.
The sun model still derives from `tod_hours` + `weather::SUN_SOUTH_TILT` with no
fabricated latitude field. EXAL-06/07 remain correctly scoped under #2371/#2372.

The two findings are both about the water arm's *content*, not its shape.

### LC-D5-01: Oblivion's water damage and water flags never reach the canonical tier — every plane of Oblivion lava is harmless, and watal.md records the omission as a game distinction that the data disproves

- **Severity**: MEDIUM
- **Dimension**: EXAL / WATAL — per-game water authoring → canonical
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:1293-1310` (the `b"FNAM"` arm's `matches!` gate, which lists five `GameKind`s and omits `Oblivion`), `:1327-1345` (the `b"DATA"` arm's `!matches!(game, GameKind::Oblivion)` guard on the damage capture), `:468-548` (`decode_data_oblivion`, which reads no damage field); consumer at `byroredux/src/cell_loader/water.rs:492-501`
- **Status**: NEW
- **Description**: The canonical water-damage path is fully built and live —
  `WaterPlane::damage_per_second` → `WaterContact::damage_per_second`
  (`crates/physics/src/water.rs:372`) → `water_damage_for_contact`
  (`byroredux/src/systems/character.rs:1048`), applied per frame at
  `character.rs:480-481`. It is populated by a filter that requires
  `record.water_flags.or(record.legacy_flags)` to have bit `0x01` set and
  `record.legacy_damage` to be `Some`. **Oblivion can satisfy neither.** The
  `FNAM` arm's guard is an explicit five-game allowlist that omits
  `GameKind::Oblivion`, so `water_flags` and `legacy_flags` are both `None` for
  every TES4 record — and a unit test at `water.rs:1468-1470` *pins* that
  (`assert_eq!(oblivion.water_flags, None)`). The damage value is equally
  unreachable: the `DATA` arm captures `legacy_damage` only from a 2-byte
  sub-record and only when the game is not Oblivion, while `decode_data_oblivion`
  reads floats from offsets 0–96 and never touches the trailing `u16`.
  `docs/engine/watal.md` §4 records the result as a per-game *distinction* —
  "legacy water damage | Oblivion: **SENTINEL** | FO3/FNV: AUTHORED when `FNAM`
  bit 0x01 is set | Skyrim: SENTINEL" — i.e. the layer asserts Oblivion does not
  author it. Oblivion does.
- **Evidence**: Direct scan of vanilla `Oblivion.esm` (TES4's 20-byte record
  header, all 23 `WATR` records, damage read as the trailing `u16` of `DATA`):
  ```
  FNAM byte distribution: {0x02: 16, 0x01: 5, 0x00: 2}

  EDID                         FNAM   DATA len   trailing u16
  OblivionCitadelLavaPlane     0x01      102          5000
  OblivionLavaTest01           0x01      102            50
  CamoranLava02                0x01       42            50
  CamoranLava                  0x01        2         65535
  OblivionOil01                0x01       62             0
  DefaultWater / SewerWater /
  SwampWater / … (16 records)  0x02   102/86             0
  XPBlood, Blood               0x00   102/42             0
  ```
  The correlation is exact and self-proving: `FNAM` bit `0x01` is set on
  **precisely** the five lava/oil records and on nothing else, and four of those
  five carry a non-zero damage value while all eighteen non-flagged records carry
  zero. That is the same `FNAM` bit-0x01 semantic the code already implements for
  FO3/FNV — Oblivion is simply excluded from the arm. Note also `CamoranLava`,
  whose entire `DATA` is the 2-byte damage payload (65535): the
  `!matches!(game, GameKind::Oblivion)` guard sends even *that* to
  `decode_data_oblivion`, which reads nothing from a 2-byte buffer.
- **Impact**: Every damaging water surface in Oblivion is canonically harmless.
  The Oblivion realm's Citadel lava (5000 dmg/s authored), Camoran's Paradise
  lava, and the Oblivion oil pools all resolve to `damage_per_second = 0.0` and
  can be swum through without effect. This is not latent: the character swim /
  drown / water-damage path shipped in the 2026-08-10 WATAL Phase 2/3 checkpoint
  and runs every frame. It is silent — there is no "record authored damage but we
  dropped it" signal anywhere — and it is *doubly* silent because watal.md
  documents the gap as intentional, so a reader checking the spec is told the
  behaviour is correct. Scored MEDIUM rather than HIGH because Oblivion is not
  the reference title and the corpus is five records; the blast radius within
  Oblivion, however, is "the entire Oblivion-realm hazard model".
- **Related**: LC-D6-01 (the sibling misalignment in the same file's Oblivion
  decoder); `docs/engine/watal.md` §4 table row "legacy water damage".
- **Suggested Fix**: Add `GameKind::Oblivion` to the `b"FNAM"` arm's `matches!`
  gate and set `legacy_flags` for it (TES4 bit `0x01` = causes damage, `0x02` =
  reflective — both already match the FO3/FNV meaning the arm implements). Read
  the trailing `u16` of the Oblivion `DATA` payload into `legacy_damage` inside
  `decode_data_oblivion`, and let a 2-byte Oblivion `DATA` fall through to the
  damage-only path as the other games do. Correct the watal.md §4 row from
  SENTINEL to AUTHORED. Pin it with a fixture built from `OblivionCitadelLavaPlane`
  (`FNAM = 0x01`, damage `5000`).

### LC-D5-02: `water_material_from_mesh` is an undeclared second `WaterMaterial` producer — WATAL §3's single-site contract is now false, and the two producers classify `WaterKind` with divergent token sets

- **Severity**: LOW
- **Dimension**: EXAL / WATAL — boundary shape
- **Location**: `byroredux/src/material_translate.rs:92-147` (`water_material_from_mesh`), `:151-172` (`water_kind_from_mesh_name`) vs `byroredux/src/env_translate.rs:912-948` (the WATR classifier); contract text at `docs/engine/watal.md` §3 item 1
- **Status**: NEW
- **Description**: `docs/engine/watal.md` §3 states the contract for the water
  boundary as "**Single site.** Both the bulk `--grid` loader and the streaming
  bootstrap call these — no second construction of `WaterMaterial`/`WaterFlow`
  anywhere." That is no longer true. Session 70 added the mesh-water slice, and
  `water_material_from_mesh` constructs a `WaterMaterial` from a NIF
  `Material` in a different module, reached from the cell and loose-NIF spawn
  paths. The design position is defensible — a NIF water mesh has no WATR record,
  so it genuinely cannot use the per-record translation, and the doc comment says
  so — but the *spec* still claims one site, which removes the auditable
  invariant: a future third producer has nothing to violate. The same split has
  already produced one concrete divergence: `WaterKind` is now classified in two
  places with different token sets. `env_translate.rs` matches
  `rapid` / `waterfall` / `falls` / `river` / `stream` (deliberately demoting
  waterfall names on horizontal cell planes); `water_kind_from_mesh_name` matches
  `waterfall` / `falls` / `rapid` / `river` / `stream` / **`canal`**. `canal`
  exists in exactly one of the two, so an asset named for a canal classifies as
  `River` through the NIF path and `Calm` through the ESM path.
- **Evidence**: `grep -rn "WaterMaterial {"` over the tree returns two
  production constructors — `byroredux/src/material_translate.rs` (via
  `WaterMaterial::default()` then field assignment, `:94-147`) and the
  `env_translate.rs` boundary; the remaining hits are in `#[cfg(test)]` blocks
  (`crates/physics/src/water.rs:906,923,944,977`,
  `byroredux/src/systems/water.rs:680`, `byroredux/src/commands/water.rs:287,422`,
  `byroredux/src/systems/character.rs:1389`,
  `byroredux/src/render/water_wave_params_tests.rs:38`). watal.md §2 does describe
  the mesh-water path in prose ("Dedicated NIF mesh-water shaders now also cross
  NIFAL…"), so this is contract drift between §2 and §3, not undocumented code.
- **Impact**: Documentation-vs-code, not runtime — hence LOW. The cost is
  auditability: the layer's headline invariant no longer describes the layer, and
  the divergent `canal` token is the first symptom of two classifiers drifting
  apart.
- **Related**: LC-D2-01 (a substantive bug inside the same new producer).
- **Suggested Fix**: Amend watal.md §3 to declare **two** boundaries with an
  explicit split of responsibility — `resolve_water_material` owns
  WATR-record-backed water, `water_material_from_mesh` owns
  `*WaterShaderProperty`-backed mesh water — and state that neither may consume
  the other's inputs. Then hoist the shared `WaterKind` token list into one
  function that both call, with the horizontal-plane waterfall demotion applied
  by the caller rather than baked into the token match.

---

## Dimension 6: Per-game translation-survey gaps (Pattern A/B/C)

**Findings: 2 (1 HIGH, 1 LOW).**

Patterns A and B are clean (see Dimension 2 for the scan results). The findings
here are Pattern C: divergent per-game struct shapes for a record that is, in
fact, one shape.

### LC-D6-01: the WATR rain/displacement simulator block is misaligned by one field in the Oblivion, FO3/FNV and Skyrim decoders — Skyrim reads a constant `0.05` as `normal_magnitude` and the boundary multiplies every canonical noise amplitude by it

- **Severity**: HIGH
- **Dimension**: Per-game translation-survey gaps (Pattern C) — divergent decode of one wire structure
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:521-531` (Oblivion: `displacement` from `[76, 88, 92]`, `rain_start_size` from `96`), `:589-608` (FO3/FNV: `displacement` from `[72, 84, 88]`, `rain_start_size` from `92`, `normal_magnitude` from `96`), `:803-838` (Skyrim: `displacement` from `[72, 84, 88]`, `normal_magnitude` from `92`, `noise_falloff` from `96`); the correct sibling is `:993` / `:1129` (FO4 / FO76: `displacement` from `[92, 84, 88]`). Canonical consumption at `byroredux/src/env_translate.rs:815-823`.
- **Status**: NEW
- **Description**: WATR's appearance payload contains two consecutive
  five-float simulator blocks — the CK/GECK water dialog's *Rain Simulator*
  (force, velocity, falloff, dampener, starting size) and *Displacement
  Simulator* (same five). The file has one layout for all of
  Oblivion/FO3/FNV/Skyrim, differing only by a +4-byte shift on Oblivion. **Five
  sibling decoders in one file disagree about where that block starts, and three
  of the five are off by exactly one field.** The proof is internal and does not
  need an external spec: each decoder already places *rain force* and
  *displacement force* twenty bytes apart (Oblivion 60 → 80; FO3/FNV/Skyrim
  56 → 76), which fixes the rain block at five floats and therefore fixes rain
  *starting size* at 76 (Oblivion) / 72 (FO3/FNV/Skyrim) and displacement
  *starting size* at 96 / 92. Yet all three of those decoders read the
  displacement block's "starting size" from the rain block's last slot, and read
  `rain_start_size` (FO3/FNV, Oblivion) or `normal_magnitude` (Skyrim) from the
  displacement block's last slot. The FO4 and FO76 decoders in the same file get
  it right — `[92, 84, 88]` against force@76 / velocity@80 — which is what the
  other three should read.

  The Skyrim variant is the damaging one. `apply_skyrim_dnam_tail` assigns
  `p.normal_magnitude = read_f32_at(data, 92)` with the comment "Skyrim's
  physical normal magnitude precedes the noise falloff", but offset 92 is the
  displacement simulator's starting size. `resolve_water_material` then folds
  that scalar into **all three** canonical noise amplitudes.
- **Evidence**: Three independent lines, all from vanilla masters.

  **(a) The block structure, read straight off `Oblivion.esm`.** A 102-byte
  `DATA` (`SEBrellachWater`), floats from offset 56:
  ```
  off  56   60    64     68   72     76    80   84    88     92    96
  val  …    0.1   0.6   0.985 2.0   0.01   0.4  0.6  0.985  10.0  0.05
            └──────── rain (5) ─────────┘  └────── displacement (5) ──────┘
  ```
  and the identical block in `FalloutNV.esm` / `Fallout3.esm` (`NVCleanWater03`,
  196-byte `DNAM`) shifted −4:
  ```
  off  56    60    64     68    72     76   80   84     88    92
  val  0.1   0.6  0.985   2.0   0.01   0.4  0.6  0.985  10.0  0.05
       └──────── rain (5) ────────┘    └────── displacement (5) ─────┘
  ```
  Same authored defaults, same 5+5 grouping, in two different games. `0.01` is
  the rain starting size and `0.05` is the displacement starting size in both.

  **(b) Offset 92 is constant across the entire Skyrim corpus.** Scanning all 34
  `WATR` records in `Skyrim.esm` (31 × `DNAM` 228 B, 3 × 232 B):
  ```
  off  92: min=0.05  max=0.05  distinct=1     ← read as `normal_magnitude`
  off  76: min=0     max=1     distinct=5     ← displacement force  (correct)
  off  88: min=0     max=7     distinct=6     ← displacement dampener (correct)
  off 184: min=0.083 max=0.928 distinct=28    ← noise amplitude scale 1 (correct)
  ```
  A "physical normal magnitude" that is byte-identical across a still Katariah
  pond, `MarkarthWaterFlow` and `RiverWaterFlowSE` is not an authored per-water
  control; a simulator default is.

  **(c) The two decoders contradict each other on the same offsets of the same
  structure.** Placing an `FalloutNV.esm` `DNAM`(196) beside a `Skyrim.esm`
  `DNAM`(228) shows field-for-field alignment through offset 192 — wind/wave
  prefix at 0–16, sun power/reflectivity/fresnel at 16/20/24, an unused word at
  28, fog near/far at 32/36, three RGBA colours at 40/44/48, the simulator blocks
  at 56–96, three noise-layer directions in degrees at 100/104/108, three layer
  speeds at 112/116/120, fog amounts at 132/140, underwater near/far at 144/148,
  noise UV scales at 172/176/180, noise amplitudes at 184/188/192 — with Skyrim
  appending 32 bytes. (Offset 52 reads `0xCDCDCDCD` in Skyrim — MSVC
  uninitialised-memory fill — independently confirming 52..56 is padding.)
  Against one identical structure, `decode_data_fo3nv` says offset 92 is
  `rain_start_size` and `apply_skyrim_dnam_tail` says it is `normal_magnitude`.
  At most one can be right; per (a) and (b), neither is.
- **Impact**: For **every one of the 34 vanilla Skyrim water records**,
  `normal_magnitude` resolves to `0.05`. `byroredux/src/env_translate.rs:815-823`
  clamps that to `[0.01, 8.0]` and multiplies all three
  `mat.noise_amplitude_scales` by it, so authored amplitudes of ~0.65–0.93 reach
  the GPU as ~0.03–0.05. Those values are `ampScale` in
  `crates/renderer/shaders/water.frag:311-313`, which scales the tangent-space
  tilt of every sampled normal (`normalize(vec3(n.xy * ampScale, n.z))`) — so
  Skyrim water surface normals are roughly **twenty times flatter than
  authored**, on the canonical reference game, across every lake, river and
  interior pool. That reads as mirror-flat, over-reflective water rather than as
  an obvious bug, which is exactly why it has survived: it is a plausible-looking
  wrong picture, not a crash, and no test can catch it because the pinning test
  (`water.rs:1788`, `assert_eq!(w.params.normal_magnitude, 0.05)`) was written
  from the code's own output and encodes the defect as expected behaviour. The
  secondary effect is the swapped `displacement[0]` / `rain_start_size` in all
  three decoders, which feeds the canonical ripple-width and rain-ripple-scale
  paths each other's values on Oblivion, FO3, FNV and Skyrim. This is the
  WATAL analogue of the `_audit-severity.md` "wrong/divergent canonical out of a
  `translate()`" row: one boundary, no per-draw fallback to mask it, and the
  reference game is the worst-affected.
- **Related**: LC-D5-01 (the sibling Oblivion gap in the same file);
  `docs/engine/watal.md` §9 Q5 lists "physical normal magnitude at 92" among the
  offsets it calls MEDIUM-confidence and asks to be verified before relying on —
  this is that verification, and it fails.
- **Suggested Fix**: Fix the block first, then re-derive the two orphans.
  (1) In all three decoders read the displacement block as
  force / velocity / falloff / dampener / **starting size** at
  `+0/+4/+8/+12/+16` from the displacement-force offset the decoder already
  uses (Oblivion 80, FO3/FNV/Skyrim 76), matching the FO4 sibling; read
  `rain_start_size` from rain-force `+16` (Oblivion 76, others 72).
  (2) Delete the `normal_magnitude = data[92]` assignment from
  `apply_skyrim_dnam_tail` and the `rain_start_size = data[92]` assignment from
  `decode_data_fo3nv`, leaving `normal_magnitude` at its neutral `1.0` sentinel
  until an offset is byte-decoded and confirmed.
  (3) Offset 96 is now the single unresolved slot and the two decoders still
  disagree about it (`normal_magnitude` vs `noise_falloff`); resolve it with the
  extract→trace method before assigning either. (4) Replace the tautological
  `assert_eq!(…normal_magnitude, 0.05)` pin with a real-data assertion in
  `crates/plugin/tests/parse_real_esm.rs` that no scalar folded into
  `noise_amplitude_scales` is invariant across a game's whole WATR population —
  invariance across 34 authored records is the signal that caught this.

### LC-D6-02: `decode_data`'s 144–220 tail is unreachable on every vanilla record, and assigns offsets that contradict `decode_data_fo3nv` for the same fields

- **Severity**: LOW
- **Dimension**: Per-game translation-survey gaps (Pattern C)
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:363-366` (the `len >= 186` early delegation) and `:396-427` (the tail reads that follow it)
- **Status**: NEW
- **Description**: `decode_data` opens with `if data.len() >= 186 { return decode_data_fo3nv(data); }`, then goes on to read offsets 144, 148, 152, 156, 172, 176, 180, 184, 188, 192, 196, 204, 208, 212, 216, 220 from the buffer. Every one of those needs `len >= offset + 4`, so the whole block is reachable only for a `DATA` payload of length 148–185 — a window no supported game emits. The observed vanilla `DATA` lengths are: Oblivion 2 / 42 / 62 / 86 / 102 (and Oblivion is routed to `decode_data_oblivion` before reaching here anyway), FO3 and FNV 2 or 186, Skyrim 2, FO4 0. The block is therefore dead on all real data. It is also *inconsistent* dead code: it maps 152/156 to `effect_controls[0..2]` and 196/204 to `effect_controls[2..4]`, and 144/148 to the underwater fog pair, which for a sub-186-byte record cannot be the same fields `decode_data_fo3nv` reads at those offsets in the long layout.
- **Evidence**: Sub-record length census over the five installed masters (`DATA` on `WATR`):
  ```
  Oblivion.esm   DATA  2×1   42×2   62×1   86×2  102×17     (→ decode_data_oblivion)
  Fallout3.esm   DATA  2×42  186×11
  FalloutNV.esm  DATA  2×70  186×8
  Skyrim.esm     DATA  2×34
  Fallout4.esm   DATA  0×42
  ```
  No length falls in `[148, 185]`.
- **Impact**: None at runtime — it is dead. The cost is that it reads as a live
  fallback path during audit and maintenance, which is how a decoder ends up with
  five mutually inconsistent offset maps for one structure (LC-D6-01).
- **Related**: LC-D6-01.
- **Suggested Fix**: Delete the unreachable tail from `decode_data` and rename
  the function to say what it is — the short compatibility shape for damage-only
  stubs and synthetic fixtures — so the file has exactly one offset map per real
  layout.

### Verified, owned elsewhere — not re-filed here

The dispatch supplied a lead from `/audit-esm` in this same suite: FO3/FNV
`WATR.DNAM` is decoded by a 52-byte Skyrim *prefix* reader, dropping bytes
52–196. **Verified against the legacy side and confirmed exactly**, with the
incidence figures reproduced independently:

```
Fallout3.esm    WATR 53   DNAM 196×41 (77.4%)  DNAM 184×1   DATA 186×11
FalloutNV.esm   WATR 78   DNAM 196×69 (88.5%)  DNAM 184×1   DATA 186×8
```

`parse_watr`'s `b"DNAM"` arm (`water.rs:1346-1360`) routes `GameKind::Fallout3NV`
to the `_ =>` fallback `decode_dnam_pre_fo4`, which returns early at
`if data.len() < 52` and reads nothing past offset 52 — while
`apply_skyrim_dnam_tail` (the tail decoder for the *same* structure) is applied
only to `GameKind::Skyrim`. The two decoders are complementary: `decode_dnam_pre_fo4`
reads the 0–16 prefix that `decode_data_fo3nv` treats as opaque, and
`decode_data_fo3nv` reads the 56–192 tail that `decode_dnam_pre_fo4` drops.
Neither is applied to the FO3/FNV `DNAM` majority. The layer consequence, which
belongs in this report: `docs/engine/watal.md` §4's per-game table marks the
FO3/FNV column **AUTHORED** for `fog_near`/`fog_far`, `wave_amplitude`/
`wave_frequency`, noise UV scales, underwater fog and the specular tail — but for
88% of FNV and 77% of FO3 records those resolve to SENTINEL, so the "canonical is
a genuine superset" claim silently fails for the majority of Fallout water.

The byte-accounting finding is `/audit-esm`'s; it is recorded here so that if
that report does not file it, it is not lost. The doc half is LC-D6-03 below.

### LC-D6-03: watal.md §4's per-game payload table misstates which sub-record and which size each game actually uses

- **Severity**: LOW
- **Dimension**: Per-game translation-survey gaps — spec vs. corpus
- **Location**: `docs/engine/watal.md` §4, row "WATR appearance payload"; §2 "Decode + translate"
- **Status**: NEW
- **Description**: The GameVariant table states the payload as
  "Oblivion DATA ~102 B | FO3/FNV DATA 186/196 B (opaque 16 B prefix) | Skyrim
  DNAM 228/232 B; FO4/FO76 201 B; Starfield 152 B+". Two of those are wrong
  against vanilla data. (a) FO3/FNV's dominant carrier is **`DNAM` at 196 bytes**,
  not `DATA`; `DATA` at 186 covers only 11/53 FO3 and 8/78 FNV records, and the
  string "196 B" is attributed to `DATA` when 196 is the `DNAM` size. Because the
  table never names `DNAM` in the FO3/FNV column, a reader cannot discover from
  the spec that the majority path even exists — which is the documentation half
  of the 52-byte-prefix gap above. (b) FO76 is **148 bytes**, not the 201 the
  table shares with FO4.
- **Evidence**: Sub-record census over all seven installed masters:
  ```
  Oblivion.esm    WATR 23   DATA 102×17, 86×2, 62×1, 42×2, 2×1      (no DNAM)
  Fallout3.esm    WATR 53   DNAM 196×41, 184×1   DATA 186×11, 2×42
  FalloutNV.esm   WATR 78   DNAM 196×69, 184×1   DATA 186×8,  2×70
  Skyrim.esm      WATR 34   DNAM 228×31, 232×3   DATA 2×34
  Fallout4.esm    WATR 42   DNAM 201×40, 188×2   DATA 0×42
  SeventySix.esm  WATR 47   DNAM 148×47          DATA 0×47
  Starfield.esm   WATR 15   DNAM 152×15          DATA 0×15
  ```
  The code is consistent with the corpus even where the doc is not —
  `decode_dnam_fo76` reads no offset past 112, so it operates correctly inside
  148 bytes; only the table is wrong.
- **Impact**: Documentation only. It matters because §4 is the artefact an
  implementer consults to decide whether a per-game arm is needed, and it
  currently understates both which record carries FO3/FNV water and how much of
  it exists.
- **Related**: the `/audit-esm` DNAM-prefix finding; LC-D5-02 (§3 drift in the
  same document).
- **Suggested Fix**: Rewrite the "WATR appearance payload" row from the census
  above, naming `DNAM` explicitly in the FO3/FNV column with its 196-byte size
  and its share of the corpus, and splitting FO76 (148 B) out of the FO4 cell.
  Add the census itself to §9 as the standing ground truth so the row can be
  re-checked rather than re-guessed.

---

## Dimension 7: Subsystem coverage vs legacy

**Findings: 0.**

All three of the previous sweep's SUBSYS findings were published, fixed, and the
fixes verified as still in place at HEAD:

| Prior | Issue | Verified at HEAD |
|---|---|---|
| SUBSYS-2026-08-16-01 (weapon reach/speed had no landing site) | #3096 CLOSED | `ItemKind::Weapon` now carries `reach` / `speed` (`crates/plugin/src/esm/records/items.rs:120-128`), populated on the Oblivion (`:202-203`) and FO4 `DNAM` (`:261-263`) arms and returned at `:332-333`. |
| SUBSYS-2026-08-16-02 (`NiTimeController` envelope discarded) | #3097 CLOSED | `crates/nif/src/anim/entry.rs:254,337` — the merged clip now derives `cycle_type` from the authored flag word (`CycleType::from_u32`) instead of the `Loop` literal. |
| SUBSYS-2026-08-16-03 (REFR `XLOC` never parsed) | #3098 CLOSED | `crates/plugin/src/esm/cell/walkers.rs:893` has the `b"XLOC"` arm; a canonical `Locked` component (`byroredux/src/components.rs:80-86`) is stamped in `cell_loader/spawn.rs:536,753,824` and gated in `interaction.rs:938`. |

The Dimension 6 finding from that sweep (PAT-D6-2026-08-16-01, FO4 weapons
decoding to all-zero stats) is also closed as #2992 and verified: `parse_weap`
now has a `GameKind::Fallout4` `b"DNAM"` arm at `items.rs:258`.

No regressions. The scene-graph decomposition, transform model, property→pipeline
mapping, animation model and string-interning checks all re-verified against
`docs/legacy/` and `nif.xml` with nothing new; the non-uniform-scale collapse
remains the documented known fidelity gap (#2456's `is_non_orthonormal` guard is
still in place). SUBSYS-05 remains open as #2221 and is not re-filed.

---

## Disproved Candidates

Recorded so a future sweep does not re-chase them.

### `translate_material` gained a `WaterMaterial` producer — **disproved**

`byroredux/src/material_translate.rs` now returns a `WaterMaterial` from
`water_material_from_mesh`, which looked like a NIFAL Dimension-3 violation (a
second populated-`Material` site in the material boundary module). It is not:
the function *consumes* the canonical `Material` and produces a different
canonical type. `translate_material` remains the sole populated-`Material`
producer. The genuine observation — that it is a second `WaterMaterial` producer
— is filed under Dimension 5 as LC-D5-02, at LOW, because the split is
defensible and only the spec text is wrong.

### FO76's WATR decoder over-reads its 148-byte payload — **disproved on real data**

watal.md §4 documents FO76's payload as 201 bytes (shared with FO4), while
`SeventySix.esm` ships 148 bytes on all 47 records — which suggested
`decode_dnam_fo76` would read past the buffer or silently mis-map a tail. It does
neither: its highest offset is 112 (`crates/plugin/src/esm/records/misc/water.rs:1150`), comfortably inside 148, and
every read is bounds-checked through `read_f32_at`. Only the doc is wrong, and
that is carried as LC-D6-03. **Not filed as a decode defect.**

### Additional candidates checked and cleared

- **A new duplicate `(x, z, -y)` swap.** Two hits outside `coord.rs` are in
  `crates/nif/examples/_tmp_sf_d2_*.rs` — throwaway diagnostic binaries, not a
  production path. Tech-debt at most; belongs to `/audit-tech-debt`, not here.
- **A per-game branch leaking into the shaders or `crates/physics`.** The only
  `GameKind` tokens downstream are inside a shader-hygiene test's forbidden-word
  list.
- **A second `WaterFlow` producer.** `WaterFlow::for_kind` is the single
  constructor; both `env_translate` and `material_translate` call it rather than
  building the struct.
- **`decode_dnam_fo4`'s displacement offsets.** Checked as a fourth instance of
  LC-D6-01 and found **correct** — `[92, 84, 88]` against force@76 / velocity@80
  is the properly aligned 5-field block, and it is what the other three decoders
  should be reading. Same for FO76.

---

## Deduplication

`/tmp/audit/issues.json` (400 issues, #2671–#3103) was keyword-scanned for every
finding: `water|watr|watal|displacement|rain|noise|normal.magnitude`,
`oblivion|lava|damage`, `blend.normals|shader.flag|BSWater`,
`fnam|dnam|watr`, plus the Dimension 1–4 keyword sets
(`coordinate|euler|4096|rotation-mode`, `material|ragdoll|hinge|havok`).
`docs/audits/` was scanned for prior write-ups of each. Per the dispatch, issue
numbers below #2671 cannot be re-queried and are carried on the prior report's
word.

| Finding | Nearest existing | Verdict |
|---|---|---|
| LC-D6-01 | No issue mentions WATR offsets or `normal_magnitude`. #2872 (`WaterFlow.speed` unit conversion) and #2787 (`ampScale`/`freqScale` sentinel duplication) are adjacent but are about the *renderer's* handling of already-resolved values, not the decode | **NEW** |
| LC-D2-01 | #2787 is the closest (water.frag `ampScale` sentinels); different layer, different mechanism | **NEW** |
| LC-D5-01 | No issue mentions Oblivion water, lava or water damage | **NEW** |
| LC-D5-02 | #2790 was a watal.md §2 staleness fix (CLOSED); this is §3 and a different claim | **NEW** |
| LC-D6-02 | No match | **NEW** |
| LC-D6-03 | #2790 (CLOSED) covered a different watal.md §2 paragraph | **NEW** |

Skipped as already OPEN and owned elsewhere: #2221, #2371, #2372, #2787, #2876,
#2887, #2888, #2889, #2969, #3067. Verified as CLOSED-and-still-fixed:
#2992, #3096, #3097, #3098 (this audit's own prior findings), #2870, #2872,
#2790.

## Verification

Read-only source review plus read-only scans of **seven** vanilla masters
(`Oblivion.esm`, `Fallout3.esm`, `FalloutNV.esm`, `Skyrim.esm`, `Fallout4.esm`,
`SeventySix.esm`, `Starfield.esm`) — sub-record census, per-offset value
distributions across whole WATR populations, and field-aligned side-by-side dumps
of an FNV `DNAM`(196) against a Skyrim `DNAM`(228). The TES4 scan uses Oblivion's
20-byte record header; the other six use the 24-byte header. No build or test
command was run. No source file, game file, or GitHub issue was modified.
Scan scripts and per-dimension notes are at `/tmp/audit/legacy-compat/`.

## Summary

- **Findings:** 6 (all NEW) — 0 CRITICAL, 1 HIGH, 2 MEDIUM, 3 LOW.
- **Prior sweep:** 4/4 findings published, fixed and verified still fixed. No
  regressions anywhere in the audit's scope.
- **Boundary health:** NIFAL / EXAL / PHYSAL / WATAL all structurally intact —
  zero per-game branches downstream of any `translate()`, zero bare `bs_version`
  comparisons, one axis-swap producer, one `Material` producer, one WATR
  `resolve_water_material`. Dimensions 1, 3, 4 and 7 are clean.
- **Where the gaps live:** not in the layers' shape but in the per-game byte
  semantics feeding them. Session 70 built a lot of water surface area very
  quickly, and the decode tier now has five sibling functions covering one wire
  structure with five different offset maps.
- **Answer to the dispatch's question** — is WATAL's canonical model a genuine
  superset? *Structurally yes, factually not yet.* Skyrim really is the right
  canonical schema and the boundary really is single-site. But the model
  currently (a) reads a simulator default as Skyrim's normal magnitude and
  flattens the reference game's water ~20×, (b) declares Oblivion's water damage
  a SENTINEL when Oblivion authors it on exactly the five lava/oil records that
  need it, and (c) marks FO3/FNV fields AUTHORED in its own table that resolve to
  SENTINEL for ~85% of that corpus. Each is a fidelity gap inside a correct
  structure, which is the good failure mode — all three are fixed at the decode
  tier without touching the boundary or anything downstream of it.
- **Highest-value fix:** LC-D6-01. It is a handful of offset corrections in one
  file, it fixes the canonical reference game's water, and the FO4 decoder in the
  same file already shows exactly what the corrected code looks like.

Suggested next step:
```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md
```

TALLY: CRITICAL=0 HIGH=1 MEDIUM=2 LOW=3
