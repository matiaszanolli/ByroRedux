# Issue #1277: Epic: Complete the NIF→Engine translation layer (Fallout geometric + material divergence)

**State**: OPEN
**Labels**: enhancement, nif-parser, import-pipeline, high

## Body

## Epic — Complete the NIF→Engine translation layer

Fallout-line interiors (FNV/FO3/FO4) are broken at the **geometric** level — not
just material — while Skyrim interiors render correctly. Same engine, same shader
(verified zero `if (game == …)` branches in `triangle.frag`). The divergence is
**upstream, in the per-game translation layer** that feeds the canonical
`Material` and `Transform`.

**Exhibit A**: a FNV casino interior renders a grossly oversized / mis-proportioned
cylindrical wall element while the railings, stools, tables and NPC around it are
correctly placed — a geometry/transform defect, plus a hard interior sun-shaft and
a posterized fixture.

**Initial geometric root-cause hypothesis — FALSIFIED 2026-05-27** (see comment
below): scanned 10 837 FNV architecture AV blocks; **0** carry non-uniform scale
or shear. `svd_repair_to_quat` discards nothing for FNV. The "broken geometry"
look is driven by the confirmed **material collapse (A)** + **interior sun leak
(C)**, not a transform-fidelity bug.

### Design / full writeup
`docs/engine/nif-engine-translation-layer.md` (checked in) — parent of the
in-progress `docs/engine/material-abstraction.md`.

### Child workstreams
- [ ] **A — Material convergence** (in progress; `material-abstraction.md` steps 4–5)
- [x] **B — Geometry/transform**: ~~preserve non-uniform scale/shear~~ DOWNGRADED — measured 0 non-uniform/shear across FNV architecture; no translation loss exists. Residual per-REFR placement only if a defect survives A+C.
- [ ] **C — Interior vs exterior lighting**: stop the M34 default sun leaking into interiors
- [ ] **D — Runtime/visual audit dimension**: headless telemetry diff per game
- [ ] **E — Translation-completeness audit**: assert canonical Material/Transform is convention-identical across games

### Meta-cause (answers "audits ignore plain-sight issues")
No audit dimension inspects rendered output or per-game translation *fidelity* —
the impactful Fallout bugs were all found by a manual headless telemetry sweep
(`FALLOUT_SYMPTOMS_2026-05-26.md`), never by an `audit-*` skill. Workstreams D+E
are the structural fix.

### Invariant (must hold)
No new `if (game == …)` branch enters the shader or renderer. All convergence
happens at the parser→engine boundary.


## Comments (4)

### matiaszanolli — 2026-05-28T02:09:31Z

## Geometry hypothesis FALSIFIED by measurement (2026-05-27)

Built `crates/nif/examples/dump_transforms.rs` and scanned every AV-bearing block
in FNV architecture for baked non-uniform scale / shear in the rotation matrices:

| Corpus | NIFs | AV blocks | non-identity rot | non-uniform scale | shear | max col-norm spread |
|---|---:|---:|---:|---:|---:|---:|
| `architecture/` (all) | 2034 | 10 837 | — | **0** | **0** | — |
| `architecture/strip/` (casinos) | 263 | 2 089 | 427 (20%) | **0** | **0** | **0.00000** |

Tool is proven live (20% of casino matrices are genuine non-identity rotations),
yet column-norm spread and column off-diagonal are **exactly 0**. FNV matrices are
perfectly orthonormal; the only scaling is the scalar `NiTransform.scale`
(0.25–2.0), carried correctly by the uniform-scale model. `svd_repair_to_quat` is
a no-op on this content — **it discards nothing.**

**The Exhibit-A "broken geometry" is not a transform-fidelity bug.** Re-ordered
likely drivers:
1. **Material collapse (Axis A, confirmed)** — FNV keyword path flattens every
   surface to `metalness 0 / roughness 0.8`; a large curved column reads as a
   featureless matte-brown mass.
2. **Interior sun leak (Axis C)** — the hard floor light-shaft is the M34 default
   exterior sun in an interior cell.
3. **Residual per-REFR placement** — only if a defect survives A+C; needs
   interactive `pick`/`mesh.info`. No systemic translation loss feeds it.

Workstream **B** is downgraded: there is no geometry-translation loss to fix for
FNV. Design doc updated (`docs/engine/nif-engine-translation-layer.md` §3, §6).


### matiaszanolli — 2026-05-28T02:17:30Z

## REFR placement: math verified post-mode-1 (2026-05-27)

User report: "walls in FNV which are rotated 90 degrees or straight out of place."

The "90°-off walls" symptom is the **already-documented** failure mode of the
pre-2026-05-26 XYZ-product Euler convention (`cell_loader/euler.rs:32`). The
ZYX-product / OpenMW formula shipped as default in commit `20074410` and is on
HEAD. Verification this session:

- `REFR_ROTATION_MODE = AtomicU8::new(1)` (ZYX); CLI override needs explicit
  `--rotation-mode N`.
- Multi-axis regression tests at `crates/core/src/math/coord.rs` pin the formula
  geometrically against two ground-truth vectors (`rx=ry=π/2` and `rx=rz=π/2`).
- Hand-verified for the 90°-about-Z case: REFR-Euler helper and NIF-node
  `zup_matrix_to_yup_quat` both produce `Ry(-π/2)` from the same physical
  rotation — paths are consistent.
- **No bypass path exists** for ESM/NIF data: every world-data Euler→Quat
  conversion routes through `byroredux_core::math::coord::euler_zup_to_quat_yup`
  or the diagnostic dispatcher. Ad-hoc `Quat::from_rotation_*` only appears in
  fly-camera / debug spin / test fixtures.

**So the residual "90°-off walls" symptom is NOT a rotation-formula bug.**
Remaining candidates, in order of likelihood:

1. **Wrong base mesh placed at right location** — FormID remap collision, missing
   master, or plugin load-order resolving the REFR's `name` to a different STAT
   than the original cell author intended. Looks geometric; isn't.
2. **Build mismatch** — the report's screenshot was taken with a build before
   `20074410` (2026-05-26 19:21).

Next concrete step: identify the specific cell + REFR via interactive `pick` /
`mesh.info` (need cell name from the user) OR add a static `refr.dump <cell>`
console subcommand that lists every spawned REFR's `(form_id, base_form_id,
base_mesh, pos, euler, computed_quat, final_scale)` for visual triage against
FNVEdit ground truth.


### matiaszanolli — 2026-05-28T02:23:51Z

## Static A/B sweep across 4 Strip casino cells — the mode flip moves ~30% of REFRs

Built `crates/plugin/examples/cell_rot_sweep.rs`: applies all 4 dispatcher Euler
modes to every REFR in a cell, reports per-REFR quat angle between modes.

| Cell | REFRs | mode 1 ≠ mode 0 | Top example | Δ |
|---|---:|---:|---|---:|
| Gomorrah00 | 1394 | **445 (31.9%)** | `dungeons\NVGamorrah\…\NVGamorrahTheaterPoleCap.NIF` | 180° |
| TOPSCasino | 1579 | 309 (19.6%) | **`dungeons\NV_CAsino_TOPS\NV_TOPS_Column01.NIF` (×4)** | **180°** |
| ULCasino | 1079 | **406 (37.6%)** | **`dungeons\NV_CasinoLo\NV_UltraLux\NV_UltraLuxRm2xFreeCol02.NIF` (×4)** | **180°** |
| Lucky38CasinoFloor01 | 2455 | 671 (27.3%) | `dungeons\NVLucky38\NVLucky38CasFloorDesk04.NIF` | 180° |

The top mode-sensitive REFRs are not clutter — they are **structural columns,
free columns, casino floor desks**, placed with multi-axis Eulers like
`(180°, 0°, 270°)` where XYZ vs ZYX diverges by exactly 180°.

**Working hypothesis**: the 2026-05-26 ship of mode 1 (`20074410`, OpenMW
ZYX-derived) was a regression for FNV. The OpenMW reference + the regression
tests both *presume* ZYX — the tests derive their ground truth from the same
assumption, so they don't independently verify against engine reality. If
Bethesda's FNV convention is actually XYZ (the pre-fix empirical default that
was sign-off-tested on `GSDocMitchellHouse`'s Z-only REFRs), then ~30% of
multi-axis casino REFRs got broken, not fixed, by yesterday's commit.

**Cheapest verification**: relaunch the casino cell with `--rotation-mode 0`
and compare visually. If the misplaced walls / columns snap into place, the
2026-05-26 fix is a regression and we revert (or scope-limit ZYX to specific
games / record types).

Tool: `cargo run -p byroredux-plugin --example cell_rot_sweep -- <ESM> <CELL_EDID>`


### matiaszanolli — 2026-05-28T03:38:54Z

## Per-game translation survey — full inventory (2026-05-28)

Four parallel scans across NIF parser / NIF importer / ESM + cell-loader / renderer.
Full writeup: [docs/engine/per-game-translation-survey.md](docs/engine/per-game-translation-survey.md).

**TL;DR**: The renderer is genuinely clean — zero `if (game == …)` branches in
shaders or renderer Rust. The invariant from `feedback_format_translation.md`
holds at the renderer boundary. **The abstraction layer is incomplete upstream**
— ~70+ per-game branches across parser/importer/cell-loader, some scattered as
hardcoded BSVER constants where helpers already exist but are bypassed, others
as outright gaps (FO4 `bhkNPCollisionObject` silently dropped, FO4-only records
with no game guards).

### Why Fallout is worse than Skyrim (the user's original observation)

Structural answer from the survey:

1. **Fallout spans the widest BSVER range** (FO3=24 → FO76=155). Skyrim sits in
   a narrow band (LE=83, SE=100). Every BSVER boundary in the parser bites
   Fallout; few bite Skyrim.
2. **Fallout introduced format-incompatible changes at every major version** —
   FO4 brought half-float verts + inline tangents + BGSM + `BsDismemberSkinInstance`
   + `bhkNPCollisionObject` + BSXFlags-bit-5-repurposed + SCOL/PKIN/MOVS/MSWP;
   FO76 brought CRC32 shader flags + `bound_min_max` + SkinTint=4 renumbering;
   Starfield brought `BSGeometry` + UDEC3 + `.mat` JSON.
3. **`bhkNPCollisionObject` silently dropped** — every FO4 cell has no static
   collision; the trimesh-from-render-geometry workaround (commit `15016ee0`)
   is a band-aid, not the fix.
4. **FNV `classify_pbr_keyword` collapses everything to matte `roughness=0.8`**
   — single biggest contributor to "Fallout looks like a different engine"
   (already documented in `material-abstraction.md` Leak B).
5. **Fallout-only records (SCOL/PKIN/MOVS/MSWP) have no game guards** — works
   today by accident.
6. **`flags2 bit 21` triple collision** — FO3/FNV `ALPHA_DECAL` vs Skyrim
   `Cloud_LOD` vs FO4 `Anisotropic_Lighting`. Three games, same bit, three
   meanings. Importer routes around this but consumers can't see which path
   produced their `is_decal` boolean.

Skyrim is "easier" because its BSVER band is narrow, its property class is one
(`BSLightingShaderProperty`), its inline LSP carries usable PBR scalars (so the
PBR-collapse never fires for vanilla Skyrim), and BGSM is optional (mods only).

### Existing abstraction primitives — both bypassed

Two enums exist but neither has trait-based dispatch or a consistent feature-flag
API:

- `NifVariant` ([`crates/nif/src/version.rs:271`](../../crates/nif/src/version.rs#L271))
  — has ~7 feature-flag helpers (`has_effects_list`, `has_material_crc`, …) but
  parser uses raw `stream.bsver() > 34` / `>= FALLOUT4` everywhere instead.
- `GameKind` ([`crates/plugin/src/esm/reader.rs:85`](../../crates/plugin/src/esm/reader.rs#L85))
  — used cleanly in some parsers (CLIMATE WLST, items WEAP/ARMO/AMMO) but
  completely absent in others (SCOL/PKIN/MOVS/MSWP, XCLL byte-length dispatch).

No `trait .*Variant` exists anywhere in `crates/`.

### Cross-cutting patterns the abstraction layer needs

**Pattern A — bypassed helpers**: ~30 raw `bsver()` comparisons should call
named `NifVariant` helpers. Pure-refactor, zero behavior change, lock the
regression class with a custom lint/test.

**Pattern B — feature-flag-on-enum → trait-per-variant**: `NifVariant`'s impl
block is becoming a directory. Lift to `GameVariant` trait with strategy
methods (`extract_collision`, `extract_tangents`, `classify_pbr`,
`extract_shader_flags`).

**Pattern C — variant-enum struct shapes**: where record *fields* differ per
game (not just a flag), the right shape is `enum CellLighting { Oblivion {…},
Fnv {…}, Skyrim {…} }`, not a flat struct with `Option` everywhere. Same for
`WeaponData`, `ShaderFlags`, etc.

### Concrete starter tasks (prioritised)

Each independently shippable, each closes a specific finding:

1. **`extract_collision` per-variant** — close FO4 `bhkNPCollisionObject` gap
   (highest user impact: player physics).
2. **Convert `BSLightingShaderProperty::parse` to variant dispatch** — split
   the 12-BSVER-comparison monolith. Highest complexity-reduction win.
3. **Add `GameKind` gates to SCOL/PKIN/MOVS/MSWP** — five-line fix each.
4. **`CellLighting` variant enum** — replace 28/40/92-byte size dispatch.
5. **Migrate raw `bsver()` comparisons to `NifVariant` helpers** — mechanical.
6. **`ShaderFlags` variant enum** — bridge BSVER<132 vs ≥132 CRC32.
7. **`classify_pbr` per-variant strategy** — unifies FNV-keyword / FO4-BGSM /
   Skyrim-LSP / FO76-`.mat`. Largest user-visible win (fixes Fallout-matte-plastic).
8. **Cross-game translation-completeness test harness** — Axis E regression guard.

Tasks 1, 3, 4 are ~1 day each. Tasks 2, 5, 6 are 2–3 days. Task 7 is the
in-progress canonical-material workstream. Task 8 is workstream E from the
epic.

### Layer-by-layer counts

| Layer | Findings | Notes |
|---|---:|---|
| NIF parser | 15 sections, ~40 sites | Mostly bypassed helpers + needed-helper gaps |
| NIF importer | 13 sections | Shader-property dispatch, tangent extraction, collision gap |
| ESM + cell-loader | 20 sections | Mix of well-gated (items/climate) and ungated (SCOL/XCLL) |
| Renderer + shaders | 0 violations | **Clean** — invariant holds |
