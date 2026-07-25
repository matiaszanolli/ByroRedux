# NIF-D1-2026-07-25-01: NiSkinData::parse reads Has Vertex Weights and the inline Skin Partition ref without their full nif.xml version gates

- **GitHub Issue**: #2168
- **Severity**: MEDIUM
- **Dimension**: Stream Position Integrity (cross-referenced under Version Gating, same root cause)
- **Location**: `crates/nif/src/blocks/skin.rs:96-108` (`NiSkinData::parse`); the incomplete gate lives on `crates/nif/src/version.rs:220-226` (`NifVersion::has_skin_data_partition_ref`)
- **Game Affected**: Would affect a `NifVariant::Morrowind`-band file at NIF version `< 4.2.1.0` (Skin Partition ref: `< 4.0.0.2`). No currently supported game ships at this version — Oblivion is the oldest supported title at v20.0.0.4/5.
- **Source Report**: `docs/audits/AUDIT_NIF_2026-07-25.md`
- **Labels applied**: `medium`, `nif-parser`, `nif`, `bug`

## Description

`NiSkinData::parse` reads two fields whose presence nif.xml gates by version, and both gates are incomplete:

1. **`Has Vertex Weights`** (nif.xml line 5072: `since="4.2.1.0" default="true"`) is read unconditionally at `skin.rs:107` (`let has_vertex_weights = stream.read_u8()? != 0;`). No version check exists anywhere nearby — every sibling version-gated field in this same function is properly guarded; this one isn't, despite an inline comment stating the correct gate in prose ("version >= 4.2.1.0") that the code never applies.
2. **`Skin Partition` ref** (nif.xml line 5071: `since="4.0.0.2" until="10.1.0.0"`) is gated by `NifVersion::has_skin_data_partition_ref`, whose body is `self <= Self::V10_1_0_0` — only the `until` bound is checked. The `since="4.0.0.2"` lower bound is missing, so for `version < 4.0.0.2` (i.e. exactly `V4_0_0_0`) the predicate still returns `true` and the parser reads a 4-byte ref that isn't on disk at that version.

Both gaps break the codebase's own established doctrine: every other "old NetImmerse" boundary in this file (`has_object_group_id`, `has_interp_controller_manager_controlled`, `has_quat_transform_trs_valid`) is a correct two-sided (or correctly single-sided, when nif.xml itself has no `since`) range check. This is the one field-level gate in the file that doesn't match its own nif.xml citation.

## Evidence

```rust
// crates/nif/src/blocks/skin.rs:102-107
if stream.version().has_skin_data_partition_ref() {
    let _skin_partition_ref = stream.read_block_ref()?;
}
// has_vertex_weights (version >= 4.2.1.0, always true for Bethesda games)
let has_vertex_weights = stream.read_u8()? != 0;
```

nif.xml cross-check:
```
5071: <field name="Skin Partition" type="Ref" template="NiSkinPartition" since="4.0.0.2" until="10.1.0.0">
5072: <field name="Has Vertex Weights" type="bool" since="4.2.1.0" default="true">
```

```rust
// crates/nif/src/version.rs:220-226
pub fn has_skin_data_partition_ref(self) -> bool {
    self <= Self::V10_1_0_0   // missing `self >= Self::V4_0_0_2 &&`
}
```

No test in the crate exercises a pre-4.2.1.0 `NiSkinData` fixture — this path has zero coverage in either direction.

## Impact

For any NIF at `version < 4.2.1.0` containing an `NiSkinData` block, the parser over-reads by 1 byte (5 bytes if also `< 4.0.0.2`), misaligning every subsequent float read inside that block's `BoneData` array. Versions this old predate the `block_sizes` table (`>= 20.2.0.5`), so there is no per-block recovery anchor for this specific era — the drift would cascade the same way the historical `#1301`/`#1310`/`#1337` v10.x bugs did. Blast radius is effectively zero for every currently supported title: Oblivion ships at 20.0.0.4/20.0.0.5, FO3/FNV at 20.2.0.7, Skyrim+ higher still — all far above 4.2.1.0. `ROADMAP.md:488` explicitly places Morrowind out of scope.

## Related

Same class of bug as the historical `#1301`/`#1310`/`#1329`/`#1337` Oblivion v10.x truncation family (all fixed); this is the one remaining unguarded field in the skinning path, one version-era further back than any of those. Same "out of scope today" class as `#1843`.

## Suggested Fix

Gate `has_vertex_weights` on `stream.version() >= NifVersion::V4_2_1_0` (default `true` when absent, per nif.xml's `default="true"`), and add the missing lower bound to `has_skin_data_partition_ref` (`self >= NifVersion::V4_0_0_2 && self <= Self::V10_1_0_0`). Add a synthetic fixture test at `V4_0_0_2` (has skin partition ref, no has-vertex-weights byte) and one below `V4_2_1_0`, mirroring the existing `old_oblivion_layout_predicates` test pattern in `version.rs`. Given the confirmed-zero blast radius, this is safe to batch with other low-urgency parser hardening rather than treat as urgent.
