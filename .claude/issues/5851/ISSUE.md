# TD8-2026-10-15-03: The CI `cargo machete` gate cannot see `[dev-dependencies]`; four unused dev-deps sit behind it

Labels: low,dependencies,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD8-2026-10-15-03`).

- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (unused-dependency gate)
- **Location**: `crates/papyrus/Cargo.toml:11` (`env_logger`), `crates/bgsm/Cargo.toml:12` (`env_logger`), `crates/sfmaterial/Cargo.toml:13` (`env_logger`), `crates/nif/Cargo.toml:19-23` (`byroredux-facegen`), `.github/workflows/ci.yml:80` (`cargo machete`)
- **Status**: NEW
- **Age**: `byroredux-facegen` dev-dep from `4b6108636` (#2599) — its only use, the `facegen_half_to_f32_copy_matches_canonical_bit_for_bit` pin, was removed with the facegen half decode (#4653; `crates/nif/src/import/mesh/decode_half_float_tests.rs:50-53` is now a comment), but the Cargo.toml comment still says "This dep exists solely so `decode_half_float_tests` can pin the two impls". `env_logger` dev-deps date from `edb0525e9` (#490, bgsm) and `c99d78035` (#762, sfmaterial); papyrus likewise has no use.
- **Effort**: trivial
- **Description**: plain `cargo machete` (what CI runs) reports "didn't find any unused dependencies". `cargo machete --with-metadata` reports `env_logger` x3 and `byroredux-facegen` (and a false positive on `cxx-build`, used by `crates/cxx-bridge/build.rs`). Verified with a scratch workspace: plain machete flags an unused `[dependencies]` entry written as `{ workspace = true }` but does NOT flag the same entry under `[dev-dependencies]`; `--with-metadata` flags both. `grep -rn env_logger crates/{papyrus,bgsm,sfmaterial}` finds no use outside Cargo.toml; `grep -rn byroredux_facegen crates/nif` finds nothing. Re-running `cargo machete` (plain) is exactly the blind spot the job comment (`ci.yml:71-74`) claims not to have ("does not produce false negatives for the plain declared-but-never-referenced case").
- **Evidence**: `cargo machete --with-metadata` output; the greps above.
- **Impact**: dev-deps compile into every test build (facegen into the nif tests) and the gate cannot catch the next one. The workspace.dependencies table itself is clean (every entry has a member).
- **Related**: #3890 (introduced the gate), closed #2426-#2431 sweep.
- **Suggested Fix**: delete the four lines (and the stale comment block in nif/Cargo.toml); switch CI to `cargo machete --with-metadata` and add `[package.metadata.cargo-machete] ignored = ["cxx-build"]` to `crates/cxx-bridge/Cargo.toml` (note `--with-metadata` may touch Cargo.lock — pair it with `--locked` or a `git diff --exit-code Cargo.lock`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
