# #7248: INC-2026-11-22-06: `diff_fields` compares raw record flags, so a compression-bit difference defeats the ITM/identical verdict (LMC-REC-006)

**Labels**: low,esm-plugin,legacy-compat,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-B-02**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: ESM records / conflict report (audit-esm Dim 7)
- **Location**: `crates/plugin/src/esm/raw_record.rs:163-165`; consumer `byroredux/src/commands/plugins.rs` `chain_verdicts`
- **Changed in**: `crates/plugin/src/esm/raw_record.rs` (884627235)
- **Status**: NEW
- **Description**: `diff_fields` begins with `if winner.flags != other.flags { diffs.push(Flags …) }` over `header.flags` exactly as stored. That word includes `FLAG_COMPRESSED` (0x00040000, `reader.rs:20`), a storage detail of how the saving tool wrote the record, and `read_sub_records` already decompresses before the sub-record bytes are compared. An override byte-identical to the record it replaces but stored compressed by one tool and uncompressed by another yields `Flags { … }`, a non-empty diff, and is therefore reported as a change instead of "identical (ITM)". It also cannot be classified as a revert.
- **Evidence**: `chain_verdicts` treats `diffs.is_empty()` as `Identical`. PRD LMC-REC-006 defines identity over the records' values (xEdit's ITM: "values identical to the values without the edit"), not their on-disk encoding.
- **Impact**: ITMs and reverts can be under-reported for mixed-tool load orders. The output is a console/`--plugin-report` line today. The same predicate will gate the "no-op must not take ownership" rule when `merge` lands.
- **Related**: LMC-REC-006, #6670.
- **Suggested Fix**: compare `flags & !FLAG_COMPRESSED` (and any other pure-encoding bits), and keep the compressed bit out of the printed flag words.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
