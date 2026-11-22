# #7189 — TOOL-D5-2026-11-21-02: games-root precedence is four tiers, documented as three, and the missing-root message does not name the fix

Labels: low,tech-debt,bug,doc-rot

- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: a player on a non-developer machine running `--game <key>` without detection
- **Location**: `crates/game-detect/src/profiles.rs` `resolve_games_root` (doc); `byroredux/src/boot/cli.rs` (the `.or_else(defaults.games_root)` and the "resolved data dir does not exist" message); `docs/engine/launcher.md` ("`BYROREDUX_GAMES_ROOT` … read first and treated as authoritative")
- **Status**: NEW
- **Description**: the real order is `--games-root`, then `[defaults].games_root`, then `BYROREDUX_GAMES_ROOT`, then `DEFAULT_GAMES_ROOT` (one developer's path). That contradicts the launcher doc's claim that the env var is authoritative. On a machine with nothing detected, the message names the developer path and the env var, but not `byro-detect --write` or the launcher, which are the actual fix.
- **Suggested Fix**: state the four tiers in the doc comment and `launcher.md`, and append "or run `byro-detect --write`" to the message.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
