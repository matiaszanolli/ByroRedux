# #5481: UI-D1-2026-10-08-02: shim hygiene: a log literal with 10 embedded spaces, a dead binding, `rustfmt` red, and a split `use` block

**Labels**: low,ui,bug,tech-debt
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5481

**Source**: `docs/audits/AUDIT_UI_2026-10-08.md` — `UI-D1-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Profile & Bridge
- **Profile**: n/a
- **Location**: `crates/ui/src/prepare.rs:61-62`, `:79`, `:81`, `:31-33` vs `:157-158`, `:486-488`
- **Status**: NEW
- **Description**:
  - **Log literal.** The `log::info!` at `:79` reads `"… with neither          MOVE nor HAS_CHARACTER …"`. A line
    continuation without `\` left 10 spaces inside the string.
  - **Dead binding.** `compressed` is computed and then discarded with `let _ = compressed;`.
  - **rustfmt.** `rustfmt --check` reports 3 hunks, including `( decompress_zlib_after_header…`. CI does not run fmt.
  - **Split imports.** The new function and its helpers were inserted between the file's two `use` groups, so
    `use crate::navigator…` / `use crate::{ScaleformHostCatalog, …}` now sit at `:157-158`.
  - **Redundant test call.** `compressed_container_is_re_emitted_uncompressed` calls `normalize_scaleform_dialect(&cws)`
    a second time just to assert `is_some()`.
- **Impact**: cosmetic. The log line is malformed.
- **Related**: #5029 (the same splice class)
- **Suggested Fix**: fix the literal with `\` continuation, drop `compressed`, move the `use` lines back to the top,
  and run `cargo fmt -p byroredux-ui`.

**Cross-report note**: the `prepare.rs:79` log literal is also one of the 48 sites in TD3-2026-10-08-01 (`AUDIT_TECH_DEBT_2026-10-08.md`); whichever lands first fixes it for both.

## Completeness Checks
- [ ] **SIBLING**: Rest of `prepare.rs` / crate checked with `cargo fmt -p byroredux-ui --check`
