# Oblivion (TES4) Compatibility Audit — 2026-07-02

**Scope**: ByroRedux readiness for *The Elder Scrolls IV: Oblivion* content — NIF
v20.0.0.5 retail + the v10.x NetImmerse tail (both sizeless), BSA v103, the live
ESM path, Oblivion legacy shaders, NIFAL canonical material translation, and the
exterior blocker chain.

**Method**: All 7 dimensions executed. Each checklist item re-read against current
source; every claim independently confirmed at file:line; regression suites run
against vanilla `Oblivion.esm` / `Oblivion - Meshes.bsa` where available. Findings
attempted-to-disprove before inclusion.

**Data availability**: `Oblivion/Data/` present. Real-data validation ran for
Dimensions 2, 3, and 6.

**Dedup baseline**: `gh issue list` (21 open issues, 2026-07-02) scanned — no open
issue covers any Oblivion NIF/BSA/ESM finding. Prior reports in `docs/audits/`
scanned.

---

## Executive Summary

Oblivion compatibility is in a **mature, regression-guarded state**. This audit
found **zero code defects** and **one LOW-severity documentation note** (a stale
sentence in the audit skill's own checklist, not in the codebase).

Current compatibility level (live numbers, not skill text):

| Layer | State (verified this sweep) |
|-------|------------------------------|
| NIF parse (v20.0.0.5 + v10.x tail) | **99.93%** clean (8 026 / 8 032), recover 99.99%, **0 failures, 0 unknown block types** — `nif_stats` over `Oblivion - Meshes.bsa`, 2026-07-02. Matches ROADMAP exactly, zero drift. |
| Archive extract (BSA v103) | End-to-end; version gate + 16-byte folder record + `embed_file_names` guard all hold (#699 regression guard). |
| ESM parse (live path) | 16-byte ACBS Oblivion arm (#1650), 8-byte CLMT WLST (#540), pre-Skyrim XCLL, `is_oblivion` ATTR/DNAM/VNAM branches — all present + correct. Both ignored real-data parity tests (`clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`) pass against vanilla `Oblivion.esm`. |
| Render (legacy shaders) | NiTexturingProperty→MaterialInfo, raw monitor-space colors (no sRGB), NiWireframeProperty→LINE + flat_shading (#869), Disney BSDF gate stays 0 (no BGSM/BGEM). |
| NIFAL canonical translate | Metalness/roughness resolve-once via `Material::resolve_pbr`; no per-draw `classify_pbr`; `EmissiveSource::Material` tagged on the NiMaterialProperty arm. |
| Interior cell | Renders end-to-end (Anvil Heinrich Oaken Halls). |
| Exterior cell | TES4 worldspace + LAND wiring implemented and game-agnostic (parse + load ✓); Oblivion-aware (worldspace selector, NAM2 default water, climate). **Only an on-device exterior render bench remains.** |

**Top blocker (only one)**: an on-device exterior render bench for a TES4
worldspace. This is quality/verification, not missing code — the parse+load path
is wired and game-agnostic (same shape FO3 was). The stale "BSA v103 is broken"
framing is dead (#699) and was NOT regenerated.

---

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.5 + v10.x tail)

12 of 13 checklist items confirmed as holding regression guards. `cargo test -p
byroredux-nif` green (846 lib + integration tests). No code defects.

#### OBL-D1-NOTE-01: Audit-skill checklist item #4 wording contradicts correct code
- **Severity**: LOW
- **Dimension**: NIF Version Handling
- **Location**: `.claude/commands/audit-oblivion/SKILL.md` (Dimension 1, checklist item on #1509); code is correct at `crates/nif/src/blocks/controller/morph.rs:88-114`
- **Status**: NEW (documentation-hygiene; code is correct)
- **Description**: The skill checklist states *"doghead.nif is v10.2.0.0 bsver 9 must keep the field."* This is backwards. The #1509 fix gates the trailing `Num Unknown Ints` on `bsver > 9`; doghead is bsver **9**, so `9 > 9` is false and the field is correctly **SKIPPED**. The field is KEPT for Oblivion's bsver-**11** morph rigs (e.g. `obgatemini01.nif`), which is what the #687 fix relies on. The checklist's own body text correctly says the gate must be `bsver > 9`, so the "must keep field" clause is internally contradictory.
- **Evidence**: `morph.rs` gate is `version >= V10_2_0_0 && version <= V20_0_0_5 && bsver > 9`. The regression test is literally named `nigeommorpher_v10_2_bsver9_skips_trailing_unknown_ints` and asserts the field is NOT read for doghead.
- **Impact**: None on runtime. Risk is only that a future editor "fixes" the code to match the erroneous checklist sentence and re-breaks doghead + its 15 trailing blocks.
- **Related**: #1509, #687
- **Suggested Fix**: Amend the skill checklist to "doghead.nif is v10.2.0.0 bsver 9 must **skip** the trailing field; Oblivion bsver-11 rigs keep it."

### Dimension 2 — BSA v103 Archive

Regression guard. No findings. `BSA_V_OBLIVION = 103` (`archive/mod.rs:32`);
version rejected outside {103,104,105} (`open.rs:40`); folder-record size is
`if version == BSA_V_SKYRIM_SE { 24 } else { 16 }` (`open.rs:100`, correct — v103
AND v104 are 16 B); `embed_file_names = version >= BSA_V_FO3_SKYRIM && flag`
(`open.rs:75`, so v103 never embeds names, and the v103 "Xbox archive" bit is
correctly ignored). `nif_stats` round-trips 8 032 NIFs through the v103 extract
path with 0 extract failures.

### Dimension 3 — ESM Record Coverage (live path)

No findings. Oblivion-specific decode branches present and correct:
- **16-byte ACBS (#1650)**: `actor.rs:612` — `GameKind::Oblivion && len >= 16` arm gated **before** the FNV `len >= 24` arm (`actor.rs:623`), reading `flags@0`, `level i16@10`. Tests `oblivion_16byte_acbs_parses_level_and_gender` + `fnv_ignores_16byte_acbs` pass.
- **CLMT WLST (#540)**: `climate.rs` — Oblivion 8-byte `(form_id: u32, chance: i32)` entries vs later 12-byte layout, game-selected.
- **XCLL lighting**: `cell/mod.rs` — pre-Skyrim 36/40-byte layout handled distinctly from the Skyrim 92-byte / Starfield 108-byte extensions.
- **`is_oblivion` sub-branches**: `actor.rs:851+` — ATTR (≥16), DNAM (≥8), VNAM (≥8), PNAM/UNAM/XNAM gated on `GameKind::Oblivion` to avoid cross-game bleed.
- **Real-data parity**: `clas_oblivion_knight_against_vanilla` and `race_oblivion_data_and_subs_against_vanilla` both pass against vanilla `Oblivion.esm` (un-ignored, this sweep).

### Dimension 4 — Rendering Path for Oblivion Shaders

No findings. Confirmed:
- No `srgb_to_linear` applied to legacy `NiMaterialProperty` colors (raw monitor-space, per 0e8efc6).
- `NiWireframeProperty { flags: 1 }` → `MaterialInfo.wireframe` → `vk::PolygonMode::LINE` (`material/walker.rs:1012+`, `material/mod.rs:615-618`); `flat_shading` forwarded through `material_translate.rs:104` (#869 guards hold).
- Typed particle-emitter path present: `apply_emitter_params` (`systems/particle.rs:29`) fed by the typed NIF emitter blocks; unit-tested.
- **Disney BSDF gate stays 0**: `is_pbr` is hardcoded `false` on every NIF geometry import path (`mesh/ni_tri_shape.rs:239`, `mesh/bs_tri_shape.rs:240`, `mesh/bs_geometry.rs:247`); it is only ever set true by the BGSM/BGEM merge, which Oblivion never authors. `MAT_FLAG_PBR_BSDF` is therefore unreachable for the entire Oblivion material universe.

#### OBL-D4-NOTE-01: Emitter runtime uses name-heuristic presets (informational)
- **Severity**: LOW
- **Dimension**: Rendering (particles)
- **Status**: NEW (intentional behavior — recorded so it is not refiled)
- **Description**: The typed NIF emitter blocks parse and route to `apply_emitter_params`, but the runtime overlays authored kinematics/size onto a name-heuristic preset rather than fully deriving all visuals from the block. This is by design (`#707`); Oblivion emitters animate. Recorded so future audits don't refile it as a drop.

#### OBL-D4-NOTE-02: NiDitherProperty / fog-related legacy properties intentionally dropped (informational)
- **Severity**: LOW
- **Dimension**: Rendering (legacy properties)
- **Status**: NEW (intentional behavior — recorded so it is not refiled)
- **Description**: A small set of legacy Gamebryo properties (e.g. `NiDitherProperty`, per-mesh fog) are deliberately not honored — they have no meaningful analogue in the RT pipeline. Not a silent bug; recorded to prevent re-flagging.

### Dimension 5 — NIFAL Canonical Material Translation

No findings. Confirmed:
- Metalness/roughness resolve **once** (`Material::resolve_pbr`, `crates/core/src/ecs/components/material.rs`); `static_meshes.rs:309-310` reads `m.roughness`/`m.metalness` directly with an explicit "no per-draw keyword scan / classify_pbr fallback" comment (#1480).
- `emissive_source = EmissiveSource::Material` set in the `NiMaterialProperty` arm (`material/walker.rs:641-642`), distinct from the Skyrim/FO4 `BSLightingShaderProperty` arm.
- `MAT_FLAG_PBR_BSDF` stays 0 (cross-referenced with Dim 4).

### Dimension 6 — Real-Data Validation

No findings — **zero drift** from the checked-in baseline. `nif_stats` over
`Oblivion - Meshes.bsa` (2026-07-02): total 8 032, clean 8 026 (**99.93%**),
truncated 6 (38 blocks dropped), **failures 0**, **unknown block types 0** across
81 distinct types. The 6 truncated files are exactly the expected pre-Gamebryo
NetImmerse markers (`marker_radius.nif`, `marker_divine.nif`, `marker_travel.nif`,
`marker_map.nif`, `marker_arrow.nif`, `marker_temple.nif`) — inline-string
type-name content that truncates gracefully rather than failing. No new block
types, no `parsed` shrinkage, no `unknown` growth.

### Dimension 7 — Exterior Blocker Chain & Quirks

No findings. Confirmed:
- Exterior parse+load path is wired and game-agnostic (`cell_loader/exterior.rs`) with Oblivion-aware handling: worldspace selector (#1655/#444), NAM2 default water (Tamriel sea-level Z=0 for Oblivion WRLD lacking DNAM), per-worldspace climate.
- Pre-v3.3.0.13 inline-type detection logs at `log::debug!` (`lib.rs:381`), not `warn` — no full-sweep spam. The only `warn` on this path (`lib.rs:405`) fires once per marker file on a failed inline-name read (6 files total), not per-block — acceptable.
- No Oblivion-specific record type is missing from the cell loader beyond the FNV-aligned set to place exterior REFRs.

---

## Blocker Chain (to "exterior cell renders")

Interiors already render end-to-end (Anvil Heinrich Oaken Halls). The remaining
chain is verification, not missing code:

1. TES4 worldspace + LAND wiring — **implemented, game-agnostic** (parse + load ✓).
2. CELL exterior REFR placement — **implemented** (shares the FNV-aligned path).
3. On-device exterior render bench for a TES4 worldspace (Tamriel) — **pending**.

Do NOT regenerate the stale BSA-v103 framing; v103 extraction has worked
end-to-end since 2026-04-17 (#699).

---

## Regression Guard List (verified still holding this sweep)

- **v10.x stride-drift family** — #1506 (NiInterpController/NiQuatTransform), #1507 (NiPSysData `Num Added Particles` `!BS202` gate), #1508 (NiBlendInterpolator three-band split), #1509 (NiGeomMorpherController `bsver > 9`): all land on the next block boundary; `nif_stats` truncation count steady at 6.
- **NiTexturingProperty raw u32 count** (#149) — no leading `Has Shader Textures: bool` gate (`properties.rs:336-337`).
- **BSStreamHeader dual-band** (#170) — `header.rs:137-143` matches nif.xml; the out-of-band non-Bethesda file test still refuses the header read.
- **`user_version >= V10_0_1_8` threshold** — `header.rs:114`.
- **NetImmerse v10.x leading group_id** — `blocks/mod.rs:298-299`, band `[10.0.0.0, 10.1.0.114)`, bhk-subtree exception preserved (#1329/#1337).
- **u16 vs u32 flag width** — `base.rs:82-86`, raw `bsver > 26`.
- **bhk motion_type full enum** (#1652) — `import/collision.rs:145-154`.
- **Collision import** — BhkMultiSphereShape / BhkConvexListShape translate, not dropped (`import/collision.rs`).
- **BSA v103 extraction** (#699) — 0 extract failures over `Oblivion - Meshes.bsa`.
- **16-byte ACBS** (#1650), **8-byte CLMT WLST** (#540), Oblivion parity tests.
- **Disney BSDF gate stays 0** — `is_pbr = false` on all NIF import paths.
- **Raw monitor-space legacy colors** (0e8efc6) — no sRGB on NiMaterialProperty.

---

## Verdict

Oblivion support is healthy. No new bugs. One LOW documentation note
(OBL-D1-NOTE-01, a stale sentence in this audit skill's checklist). All 12+
regression guards verified against live source and real vanilla data with zero
drift. The sole remaining item is an on-device exterior render bench — quality
verification of already-wired code.
