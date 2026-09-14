# #4372 — TD6-005: `SetInChargen` and `SetHudCartMode` write state nothing reads, but `Effect::is_placeholder` doesn't flag them

**Labels**: low, scripting, save-load, game:skyrim, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4372

- **Severity**: LOW · **Dimension**: 6
- **Location**: `crates/scripting/src/translate/effects.rs:338-340` (+ the `:2799` assertion pinning `!SetHudCartMode.is_placeholder()`), `crates/scripting/src/cinematic.rs:185-202`, `crates/scripting/src/player_control.rs:59` · **Status**: NEW (sibling of closed #4328) · **Age**: `d0dac91b1` (09-13), `5162829a3` (09-14) · **Effort**: small · **Kind**: tech-debt
- **Finding**: `disable_saving` / `disable_waiting` / `show_controls_disabled_message` and `hud_cart_mode` have no production reader; `cinematic.rs`'s own doc says saving is not actually blocked. The code path is reachable through the MQ101 `--new-game` route (no smoke test). Coverage harnesses report these fragments as fully claimed, and a player can quicksave inside Bethesda's save-disabled chargen block.
- **Suggested Fix**: Either tag both as placeholders (trivial) or gate quicksave/wait on the flags (small).

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
