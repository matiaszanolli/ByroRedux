# TD2-2026-10-15-15: `ExtensionComponentStore::apply_batch` repeats the schema/field/type lookup and row-budget staging in its `Set` and `IncrementI64` arms

Labels: low,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-15`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/sdk/src/component.rs:490-614`
- **Status**: NEW (the sandbox quota path; crate not named by any prior report)
- **Age**: `84e899dbe` / `316fb2028` (2026-08-31); untouched since 2026-09-01
- **Effort**: trivial
- **Description**: 35 identical lines in both arms: `schemas.get(&schema_key).ok_or(UnknownSchema)`, `fields.iter().find(..).ok_or(UnknownField)`, the `TypeMismatch` build, then `row_key`, `new_rows.insert(...)` + `existing_rows + new_rows.len() > max_rows_per_principal` -> `RowBudgetExceeded`, and the `staged.entry(..).or_insert_with(..)` clone. A third command variant (the JContainers work adds collection ops) would copy it again, and the row-budget arm is a security quota.
- **Evidence**: clone scan 8 windows `:496-562` / `:542-588`.
- **Impact**: a divergence here is a quota bypass in sandboxed-extension state, so the cost of a one-sided fix is higher than the line count suggests.
- **Related**: `docs/engine/sandboxed-linked-mods.md`.
- **Suggested Fix**: `fn field_declaration(&self, principal, schema, field) -> Result<&ComponentFieldDeclaration>` and `fn stage_row<'a>(&'a self, staged, new_rows, existing_rows, principal, schema, entity) -> Result<&'a mut Row>` on the store; both arms keep only their type check and value write.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
