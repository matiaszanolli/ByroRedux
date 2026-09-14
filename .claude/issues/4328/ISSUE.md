# #4328 SCR-D5-2026-09-14-04: `ShowRaceMenu` / `RequestSave` / `RequestAutoSave` lower to counters nothing reads, and `fragment_coverage` counts them as claimed

**Labels**: low,scripting,test-gap,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs` `prim_show_race_menu` / `prim_request_save` / `prim_request_auto_save`; `crates/scripting/src/cinematic.rs` `show_race_menu` / `request_save`; `crates/scripting/examples/fragment_coverage.rs` (`claimed` tally)
- **Status**: NEW
- **Description**: The arities match (0-arg native globals). Dispatch only bumps `race_menu_shown_count` / `save_requested_count`, which nothing reads. No race menu opens and no save is written; the variant docs say so. However, `fragment_coverage` counts any `Some(effects)` as fully lowered, so these stubs feed the commit's reported 45.9% → 50.0% with no placeholder marking. There are no over-arity decline tests for the two save primitives.
- **Impact**: No state corruption. The costs are misleading coverage numbers and a silent MQ101 divergence (no race choice, no chargen saves) that a reader of the tally cannot see.
- **Related**: SCR-D5-2026-09-14-02
- **Suggested Fix**: Tag placeholder effects (e.g. `Effect::is_placeholder()`), report them as a separate "claimed-by-stub" count, and add over-arity decline tests.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
