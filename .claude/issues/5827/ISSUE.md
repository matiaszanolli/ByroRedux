# TD3-2026-10-15-03: `fragment/vm.rs` module doc ends "Not modelled: structs." although `Value::Struct` and FO4 struct support exist

Labels: low,scripting,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-03`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `crates/scripting/src/fragment/vm.rs:50`
- **Status**: NEW
- **Age**: `20cbfd7b7` (2026-10-13, "Fallout 4 structs and array methods on the Papyrus VM"); the line predates it (`4cfdc525b`).
- **Effort**: trivial
- **Description**: `Value::Struct(Rc<RefCell<Vec<(String, Value)>>>)` is at `:107`, `PropertyValue::Struct` converts at `:214`, and `scripting.md:696-707` documents struct and array-method support (including save serialisation).
- **Evidence**: `sed -n 100,110p` vs `sed -n 50p crates/scripting/src/fragment/vm.rs`.
- **Impact**: the module header is the first thing a reader of a 6 215-line file sees.
- **Related**: #5788 is the open sibling for `obscript_quests.rs`'s "phase 1" header (`:1`, `:14`, `:22`, `:207`) — do not re-file; widen it to the `0x1059` / `0x101f` rows.
- **Suggested Fix**: delete the line (or replace it with what is genuinely unmodelled, e.g. "a struct or array held across `Utility.Wait` — the saved continuation stores scalars only", `scripting.md:926`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
