# #6976: TD4-2026-11-15-01: `audit-oblivion` and `audit-fnv` cite `placement_lod_supported_is_oblivion_only` as a live guard; cc39d7792 deleted it

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6976
- **Labels**: low,game:oblivion,game:fnv,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 4. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: cc39d7792 (#6637, DLOD-GATE-008) replaced `placement_lod_supported_is_oblivion_only`, `combined_and_legacy_lod_capabilities_are_disjoint` and `object_lod_scheme_table`'s scheme rows with `lod_family_table_covers_every_game` (`byroredux/src/cell_loader/lod_support.rs:681`, doc at `:675-680`).
- **Evidence**: `.claude/commands/audit-oblivion/SKILL.md:98` ("`placement_lod_supported_is_oblivion_only`; FO3/FNV ship none ..."), `.claude/commands/audit-fnv/SKILL.md:24` ("`placement_lod_supported` is Oblivion-only (`placement_lod_supported_is_oblivion_only`)"). No `fn placement_lod_supported_is_oblivion_only` at HEAD; the function `placement_lod_supported` (`placement_lod.rs:359`) still exists.
- **Fix**: name `lod_family_table_covers_every_game` in both. The gate's symbol check reports only `docs/engine`; extending it to skills (backticked names of 4+ snake words with no `fn`/`const`/`struct` definition and not in `_audit-validate-symbol-exempt.txt`) found exactly this one real hit among 24 candidates (the rest are ash/std names, struct fields, TSV keys, or file stems), so it is cheap to add.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
