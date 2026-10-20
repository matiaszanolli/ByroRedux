# TD7-2026-10-19-01: ObScript ledger accumulators use unchecked `+=` where the sibling `MiscStats` saturates

Labels: low,scripting,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants (silent over/underflow)
- **Location**: `crates/scripting/src/obscript_more/state.rs:37` (`l.crime_gold += num(args, 0) as i32`), `crates/scripting/src/obscript_ledger.rs:131` (`*self.disposition_mods.entry(actor).or_default() += delta`, fed by `obscript_more/state.rs:113` `... as i32`)
- **Age**: `6e9d55aa9` (2026-10-18)
- **Effort**: trivial
- **Description**: `as i32` of the script's f32 argument saturates, so two `ModCrimeGold 2147483647` statements overflow; `profile.dev` has overflow checks (panic inside a scheduler system on the default `cargo run`), `profile.release` wraps to a negative bounty. The neighbouring `ModMiscStat` path was written correctly (`crates/scripting/src/misc_stats.rs:51`, `saturating_add`). Documented-use values (hundreds) are far below the limit, hence LOW, but both sit on mod-supplied bytecode (ObScript quest scripts).
- **Suggested Fix**: `saturating_add` in both (same one-liner as `MiscStats::increment`); a two-line test per site.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
