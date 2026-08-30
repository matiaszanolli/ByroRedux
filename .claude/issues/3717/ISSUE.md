# #3717: NIF-2026-08-30-D2-01: NiDynamicEffect's pre-4.0.0.2 affected-nodes fields are never read, on the one band where the miss cascades

**Labels**: bug, nif-parser, low, nif
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIF_2026-08-30.md` · **Severity**: LOW · **Dimension**: Version Gating
**Game affected**: any content at NIF version <= 4.0.0.2 — the pre-Gamebryo Morrowind variant band, which Oblivion's own `Data/` ships (5 files)

## Location
- `crates/nif/src/blocks/base.rs` — `NiDynamicEffectData::parse`

## Description
nif.xml gives `NiDynamicEffect` **two** affected-nodes field groups; the parser implements only the later one. The `since="10.1.0.0"` pair is read; the `until="4.0.0.2"` pair is not read at all, so a `NiLight` or `NiTextureEffect` in a v<=4.0.0.2 file under-reads by 4 bytes plus `4 × Num Affected Nodes`.

## Evidence
nif.xml:3497-3505 —
```
<field name="Num Affected Nodes"     type="uint" until="4.0.0.2" />
<field name="Affected Nodes"         type="Ptr"  length="Num Affected Nodes" until="3.3.0.13" />
<field name="Affected Node Pointers" type="uint" length="Num Affected Nodes" since="4.0.0.0" until="4.0.0.2" />
<field name="Num Affected Nodes"     type="uint" since="10.1.0.0" vercond="#NI_BS_LT_FO4#" />
<field name="Affected Nodes"         type="Ptr"  length="Num Affected Nodes" since="10.1.0.0" vercond="#NI_BS_LT_FO4#" />
```

Verified against current source: the parser's only array read is gated `pre_fo4 && stream.version() >= NifVersion::V10_1_0_0`; there is no `<= V4_0_0_2` arm. The `switch_state` gate (`>= V10_1_0_106`) correctly matches its own `since="10.1.0.106"`, and the `bsver < FALLOUT4` gate correctly matches `#NI_BS_LT_FO4#` (nif.xml:16).

## Impact
Bounded and currently unrealised. Files below v4.0.0.2 have neither a `block_sizes` table nor a header block-type table (`inline_type_names`), so there is no recovery anchor — a 4-byte under-read there truncates the rest of the scene rather than corrupting one block, which makes the potential impact disproportionate to the field's obscurity.

**Explicitly unattested**: the 624,702-file corpus contains 5 files in this band (Oblivion's `marker_radius.nif` at v3.3.0.13 and four v4.0.0.2 markers), all parsing clean with zero truncations, so none carries a `NiLight` or `NiTextureEffect`. Reported as a spec-conformance gap on a reachable band, **not** as a live defect.

## Related
#1750 / TD2-001 (the consolidation that gave this gate one home — the fix belongs in the same function), #721, #1240.

## Suggested Fix
Add the `until="4.0.0.2"` arm alongside the existing one (`Num Affected Nodes` always, then `Affected Nodes` as `Ptr` for `<= 3.3.0.13` / `Affected Node Pointers` as `uint` for `4.0.0.0..=4.0.0.2`), plus a synthetic fixture at v4.0.0.2 — there is no vanilla sample to regression-test against.

## Completeness Checks
- [ ] **SIBLING**: other `until="4.0.0.2"` field groups in the same block family checked
- [ ] **TESTS**: A synthetic v4.0.0.2 fixture pins this specific fix
