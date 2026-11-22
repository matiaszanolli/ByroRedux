# #7251: INC-2026-11-22-10: new LMC modules sit under catch-all ownership rows

**Labels**: low,tech-debt,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-B-06**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: Tooling (audit ownership map)
- **Location**: `.claude/commands/_audit-owners.md` (no row for `byroredux/src/plugin_activation.rs`, `byroredux/src/commands/plugins.rs`, `crates/plugin/src/{activation,archive_pairing}.rs`, `crates/plugin/src/esm/{raw_record.rs,records/provenance.rs}`, `crates/core/src/form_id.rs`)
- **Changed in**: files added in d497bc3e2, 884627235 and 1623b64f4
- **Status**: NEW
- **Description**: the validator advisory lists `byroredux/src/plugin_activation (387 LOC)` as reached only by the catch-all row. `crates/plugin/` falls to the generic `esm, performance Dim 7` row (`_audit-owners.md:89`), so these untrusted-input readers and the boot/argv rewrite are not routed to audit-tooling Dim 4, audit-save Dim 2 or audit-parsers.
- **Evidence**: routing `crates/plugin/src/activation.rs` and `byroredux/src/plugin_activation.rs` through `.claude/commands/_audit-route.sh` resolves both by general prefix rows, not an LMC-specific owner.
- **Impact**: a later `/audit-incremental` routes them thinly.
- **Related**: IA-B-05.
- **Suggested Fix**: add rows such as `byroredux/src/plugin_activation.rs, commands/plugins.rs | esm, tooling Dim 4; per-game` and `crates/plugin/src/{activation,archive_pairing}.rs, esm/{raw_record,records/provenance}.rs | esm, performance Dim 7`.

**Update at publish (origin/main 0e4491571)**: #7241 added `crates/plugin/src/esm/records/list_merge.rs`, which also lands on the general `crates/plugin/` row; give it the same LMC row as `provenance.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
