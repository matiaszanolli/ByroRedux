# #5465: SF-2026-10-08-D3-01: `MaterialIndex` assigns `PersistentID` columns by `BTreeMap` alphabetical field order (Dir < Ext < File), while its docs call the read "positional"

**Labels**: low,import-pipeline,tech-debt,bug,game:starfield,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5465

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-08.md` — `SF-2026-10-08-D3-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: CDB Material Database
- **Location**: `crates/sfmaterial/src/index.rs:582-604` (the `Objects` stream closure); docs at `:13-14` and `:165-168`
- **Status**: NEW
- **Description**: The closure collects the `PersistentID` struct's three `U32` columns from `pid.fields.values()`. `ObjectInstance.fields` is a `BTreeMap<String, Value>` (`value.rs:55`), so iteration order is alphabetical: `Dir`, `Ext`, `File`. The destructure `[stem, dir, _ext]` therefore reads `Dir` → stem and `Ext` → dir, and binds the `"mat"` constant column `File` as `_ext`.
  - That is correct, but only because the three field names happen to sort that way.
  - On disk the positional order is `Dir`(stem)@0, `File`("mat")@4, `Ext`(dir)@8, which the fixture spells out at `index.rs:931-936`. A truly positional read with the same destructure would key every object on the `"mat"` constant.
  - The module doc ("the columns are positional") and `material_key`'s doc ("this is positional") describe the opposite of what the code relies on.
- **Evidence**: As above. Real-data lookups pass today (`material_index_resolves_a_vanilla_material`, run green this audit).
- **Impact**: None today. A refactor that follows the docs, or switches `fields` to an insertion-ordered map, breaks every CDB lookup. The synthetic fixture would catch it, so this is fragility plus a doc contradiction, not a live bug.
- **Related**: #3398; the `.Dir` = stem / `.Ext` = dir label rotation recorded in the 08-29 spike.
- **Suggested Fix**: Read the columns by name (`fields.get("Dir")` → stem, `fields.get("Ext")` → dir) and correct both comments.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `fields.values()` positional reads over `ObjectInstance.fields` in `crates/sfmaterial`)
- [ ] **TESTS**: A regression test pins this specific fix
