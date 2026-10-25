# TD8-2026-10-25-01: Regression of #6193: `--all-targets` re-accrued 11 lints in 3 files, one deny-level — the `||  true` is also a vacuous assertion

Labels: low, test-gap, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6394
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW (non-blocking CI step) · **Dimension**: 8 (overlaps 9) · **Effort**: small · **Status**: Regression of #6193 (CLOSED, bbda0c975)
- **Location / sites** (all test/example targets; `-D warnings` gate does not see them):
  - `crates/plugin/tests/parse_real_esm.rs:6467` — **error** `clippy::overly_complex_bool_expr`: `.all(|race| !race.body_part_textures.is_empty() || true)`. Added by 9d1a262ed (#6090, 2026-10-25). The assertion cannot fail, so the "Skyrim+ authors none — the parser's own gate, pinned" comment is a lie; and the deny-level error aborts that test target's lint (`byroredux-plugin` test "parse_real_esm" "could not compile").
  - `crates/plugin/src/esm/records/actor/tests.rs:56,59,1478,2673-2677` — 8x `needless_borrows_for_generic_args` (`sub(b"MODT", &[0u8; 12])` etc.; lines from 4a7963156 #6297 10-22, 1a67c9f30 10-24, 4cfcee4b9 10-24).
  - `crates/scripting/src/condition/tests.rs:2480` — `clone_on_copy` on `Condition` (1a67c9f30, 10-24).
  - `byroredux/src/asset_provider/texture.rs:1272` — `type_complexity` on the `[(&str, fn() -> PathBuf, &str); 4]` table (3e4bfab51 #6184, 10-22).
- **Fix**: drop the `&` on the byte-array args and the `.clone()`; name the tuple (`type GameArchiveCase = …`). For 6467 either assert something real (`race.body_part_textures` is empty for Skyrim+ races per the comment: `.all(|race| race.body_part_textures.is_empty())`, guarded to non-Fallout races) or delete the block. Re-run `--all-targets --keep-going` to zero.
- **Merged from Dim 9 (TD9-2026-10-25-02, `test-gap`)**: the `|| true` at `parse_real_esm.rs:6467` is also a vacuous assertion — the test parses only `Fallout3.esm`, so no Skyrim+ race is ever visited and the "Skyrim+ authors none" gate is not exercised. Fix both in one commit.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD8-2026-10-25-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
