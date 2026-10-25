# TD1-2026-10-25-06: `crates/scripting/examples/mq101_conformance.rs` is a 2157-prod-LOC single-file example whose only filed issue (#5586) is already fixed

Labels: low, scripting, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6360
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 1 · **Effort**: small
- **Location**: whole file; example (not shipped, needs a Skyrim install). `main()` L2133, `run()` L397-441.
- **Premise check**: #5586 (OPEN) says `fn run()` is 1454 lines, depth 15. It is **45 lines** now; the body was decomposed into `check_*` helpers by `d7c47d741` (2026-10-15). Largest fn in the file is `check_scene_runtime` 165 lines. So #5586's premise is gone and it should be closed; the residual is purely file size (never filed; the 10-21 report listed it as "unchanged at 2157").
- **Split axis (responsibility)**: cargo builds `examples/<name>/main.rs`, and `examples/common/` already exists in this crate: `mq101_conformance/{main.rs (run, main, Checks, data_dir), ast_probe.rs (L119-340: expression/statement predicates, `mq101_player_callback_contract`), scenes.rs (L522-1285: SceneSet/SceneInventory, check_scene_*, analyze_scene), packages.rs (L928-1160: analyze_package_action, execution-kind classifiers), scripts.rs (L1286-1700: ScriptCorpus, check_scripts/fragments/imad), fragments.rs (L1700-1985), report.rs (print_inventory)}`. No `include_str!` reader (grep clean). The hygiene guard only keys on `tmp_`/"throwaway" names.


#### Dimension 2 — Duplication (12)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD1-2026-10-25-06`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
