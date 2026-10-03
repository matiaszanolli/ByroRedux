# #5227 — FO3-D4-02: Real-data test helpers carry another helper's doc paragraph; `data_dir` / `game_data_dir` are undocumented

https://github.com/matiaszanolli/ByroRedux/issues/5227

Source: `docs/audits/AUDIT_FO3_2026-10-03.md` (HEAD `be3cd9468`)

- **Severity**: LOW
- **Dimension**: BSA v104 & Real-Data Validation (test-harness doc rot). Owner `/audit-tech-debt`. FO3 reach: `ba2_real.rs` holds FO3's only committed BSA test.
- **Location**:
  - `crates/bsa/tests/ba2_real.rs:28-42`;
  - `crates/nif/tests/common/mod.rs` (~325);
  - `crates/plugin/tests/parse_real_esm.rs` (~34);
  - a sibling with a different paragraph at `crates/papyrus/src/parser/script.rs` (~851, `parse_recovering`).
- **Status**: NEW. It is the same kind of defect as #5029, at different sites.
- **Description**:
  - #3850 inserted `require_game_data` between the "Resolve a `Data/` directory from an env var…" paragraph and the `data_dir` / `game_data_dir` function that paragraph documents.
  - Both `///` blocks attach to `require_game_data`, and `data_dir` has no doc.
  - 4ad847a81 satisfied clippy's `empty_line_after_doc_comments` by deleting the blank line instead of moving the paragraph. That silenced the only lint that flagged the problem.
- **Evidence**: I confirmed it at `ba2_real.rs:28-63`: the resolve paragraph runs straight into the #3850 strict-lane paragraph on `fn require_game_data`, and `fn data_dir` at :63 has no doc. `git show 4ad847a81` shows the same blank-line deletion in all 4 files.
- **Impact**: no runtime effect; the strict-lane helper docs are misleading.
- **Related**: #3850, #4660, #5029, 4ad847a81.
- **Suggested Fix**: move the resolve paragraph onto `data_dir` / `game_data_dir` in the 3 real-data files. Move the #4472 paragraph in `script.rs` onto its test, or drop it.

## Completeness Checks
- [ ] **SIBLING**: All 4 files touched by 4ad847a81's blank-line deletion are fixed (`ba2_real.rs`, `nif/tests/common/mod.rs`, `parse_real_esm.rs`, papyrus `script.rs`)
- [ ] **TESTS**: `cargo clippy --all-targets` stays green (`empty_line_after_doc_comments`)
