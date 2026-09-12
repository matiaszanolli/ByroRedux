# TD1-002: `storage_util.rs` — a 384-line/17-arm dispatcher and a 251-line declarative-call vec that should be a table

Labels: low,tech-debt,bug

**Description**: `adapt_storage_util_global_list` (confirmed 384 lines by brace-depth) dispatches 17 `StorageUtilListCall` verbs, each carrying real distinct logic — a genuine "one function per verb" case, not a table candidate. `papyrus_storage_util_declarations` (251 lines) is the opposite shape: a flat `vec![...]` of ~35 near-identical declaration calls — a lookup-table candidate, and the file's own 2026-09-09 commit already performed exactly this conversion on a sibling 106-arm match elsewhere in the same file.

**Evidence**:
`crates/sdk/src/compatibility/storage_util.rs:1630-2013` (`adapt_storage_util_global_list`), `:420-670` (`papyrus_storage_util_declarations`).

**Impact**: No runtime impact — pure maintainability, same class the file's own recent commit already fixed for a sibling match.

**Related**: None named.

**Suggested Fix**: Extract each `adapt_storage_util_global_list` arm body into a named helper (`list_op_add`, `list_op_sort`, ...) behind a thin ~30-line dispatcher; replace `papyrus_storage_util_declarations`'s vec literal with a `const` tuple table fed through a small loop.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
