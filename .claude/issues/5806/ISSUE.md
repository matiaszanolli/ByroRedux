# TD1-2026-10-15-08: `crates/scripting/src/lib.rs` now has two jobs — 64 `pub use` lines (27 on 10-12) and a 241-line `register` (72 on 10-12)

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-08`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/lib.rs:1-550`, `register` at L310
- **Status**: NEW (the 09-05 report dismissed it as "a facade with one non-re-export item"; that premise no longer holds)
- **Age**: M47.2 commits since 10-12 (see finding 03)
- **Effort**: small
- **Description**: rule "`mod.rs` / `lib.rs` with >20 `pub use` (two jobs)". It was 72 mods + 27 re-exports + a 72-line `register`; now `register` is 241 lines, 37 of them the same three-line "insert default if absent" block.
- **Evidence**: `grep -cE '^\s*pub use' lib.rs` → 64 (27 at 5d31fd637); `fnscan.py`: `register 241 / was 72`.
- **Impact**: the crate's public surface and its world setup edit the same file; every native touches both.
- **Related**: finding 03.
- **Suggested Fix**: move `register` to `crates/scripting/src/registration.rs` (`lib.rs` re-exports it), add `World::init_resource::<R: Default>()` in `crates/core/src/ecs/world.rs` and replace the blocks. The other `pub use` hubs are unchanged and fine (`core/ecs/components/mod.rs` 44 lines of re-exports in 114 lines; `plugin/esm/records/mod.rs` 24).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
