# #3223 — OBL-2026-08-20-D3-01: WATR.MNAM is the literal string "lava" on Oblivion's two damaging planes and parse_watr has no MNAM arm — no authored lava discriminator reaches canonical (63 cells)

**Issue**: #3223 — https://github.com/matiaszanolli/ByroRedux/issues/3223
**Finding ID**: `OBL-2026-08-20-D3-01`
**Severity**: MEDIUM
**Dimension**: 3 — ESM record coverage / WATAL parse boundary
**Audit**: `/audit-oblivion` — `docs/audits/AUDIT_OBLIVION_2026-08-20.md` (HEAD `bb0b92f2`, 2026-08-20 comprehensive suite)
**Labels**: medium, legacy-compat, import-pipeline, bug
**Filed**: 2026-08-20 · `/audit-publish`

---

**Audit**: `/audit-oblivion` — `docs/audits/AUDIT_OBLIVION_2026-08-20.md` (Dim 3 — ESM record coverage / WATAL parse boundary), HEAD `bb0b92f2`
**Finding ID**: `OBL-2026-08-20-D3-01`

- **Severity**: MEDIUM
- **Status**: NEW

## Location

`crates/plugin/src/esm/records/misc/water.rs:1282-1372` — `parse_watr`'s sub-record `match`. Arms exist for `ANAM`, `FNAM`, `TNAM`, `NNAM`, `NAM1`-`NAM5`, `DATA`, `DNAM`, `GNAM` — **none for `MNAM`**, and none for `SNAM`.

## Description

TES4 `WATR` carries an `MNAM` zstring — the Construction Set's **Material** field, the Havok surface-material name. On vanilla `Oblivion.esm` it is the literal ASCII `lava\0` on exactly two records, and an empty `\0` or absent on the other 21.

It is the only **authored, non-heuristic** discriminator between water and lava anywhere in Oblivion's data, and the engine currently has none:

- `WaterKind` (`crates/core/src/ecs/components/water.rs:48-66`) has only `Calm` / `River` / `Rapids` / `Waterfall`
- the classifier (`byroredux/src/env_translate.rs:912-947`) is a pure EditorID keyword match on `"rapid"` / `"waterfall"` / `"falls"` / `"river"` / `"stream"` — **none of which any of Oblivion's 23 EditorIDs contains**

**Every Oblivion lava plane therefore becomes `WaterKind::Calm`**: refractive, buoyant, swimmable, blue-water-shaded.

## Evidence

Byte-level decode of the `WATR` GRUP in `Oblivion.esm`:

```
OblivionCitadelLavaPlane  FNAM=01  MNAM=6c 61 76 61 00 ("lava")  DATA[100]=5000
OblivionLavaTest01        FNAM=01  MNAM=6c 61 76 61 00 ("lava")  DATA[100]=50
CamoranLava               FNAM=01  MNAM=absent                   DATA(2)=65535
CamoranLava02             FNAM=01  MNAM=absent                   DATA(42) tail=50
OblivionOil01             FNAM=01  MNAM=00                       DATA(62) tail=0
(the remaining 18 records: FNAM=02 or 00, MNAM=00 or absent, tail=0)
```

The two `MNAM="lava"` records are **precisely** the two carrying a non-zero damage value in the full-length `DATA` layout — the two channels corroborate each other.

`SNAM` (the surface sound, present on **17 of 23** records) is dropped by the same `match`.

## Impact

**63 vanilla `Oblivion.esm` cells** reference a damage-flagged `WATR`:

```
 45  OblivionLavaTest01     (OblivionRDCaves*, OblivionRD00*, MS13OblivionCave*,
                             DAPeryiteCave01, OblivionMqKvatchSmallTower02, …)
 15  OblivionCitadelLavaPlane (OblivionRD002Citadel*, OblivionRDCitadel05, …)
  2  OblivionOil01
  1  CamoranLava02
 ---
 63  total (61 with a non-zero damage value)
```

— the entire Deadlands / Oblivion-realm content set plus `MS13OblivionCave*`, `DAPeryiteCave01` and the Kvatch towers. All of them present as ordinary calm water.

Once **#3145**'s damage fix lands, the damage will apply but the *surface* will still be classified, shaded and simulated as water.

## Related

- **#3145** (`ESM-D5-06` / `LC-D5-01`) — the damage + `FNAM` half of the same record and the same 20-line `match`. **File the fixes together.**
- **#3198** (`FNV-D1-02`) — the other half of the same shape: the `WaterKind` classifier's token set is Skyrim vocabulary, so all 78 FNV records also classify `Calm`. Oblivion has the identical problem; this issue is the *parser* gap (no `MNAM` arm at all), #3198 is the *classifier* gap.
- **#3200** (`FNV-D4-01` / `FO3-D3-01`) — the exact analogue one era later: FO3/FNV author water hazard via `WATR.XNAM` and `parse_watr` has no `XNAM` arm either. Three eras, three undecoded hazard channels, one `match`.
- **`OBL-2026-08-20-D5-02`** — `docs/engine/watal.md` §4 lists neither `MNAM` nor `SNAM`.

## Suggested Fix

Add an `b"MNAM" => out.material_name = read_zstring(&sub.data)` arm and surface it on `WatrRecord`. Decide the canonical consumer separately — the minimum useful step is to let `env_translate`'s classifier read it so a future `WaterKind::Lava` (or a `WaterMaterial` hazard flag) has an **authored** input rather than another EditorID keyword list.

**Do not** invent a lava `WaterKind` from the EditorID string; that is exactly the guessing this field exists to prevent.

## Completeness Checks
- [ ] **SIBLING**: `SNAM` (17/23 records) is dropped by the same `match`; and #3200's `XNAM` is the FO3/FNV analogue — consider one pass over `parse_watr`'s arm set against the per-era sub-record census
- [ ] **CANONICAL-BOUNDARY**: the water/lava discriminator resolves once at the parser -> `WaterMaterial` boundary, never re-derived from an EditorID at render or physics time
- [ ] **TESTS**: a fixture `WATR` carrying `MNAM = "lava"` asserts the field survives to `WatrRecord`, and that an `MNAM`-less record does not fabricate one
