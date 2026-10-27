# CHAR-2026-10-26-L-09: `PoolRegenConfig` fills the unused TES lanes with AVIF id `0` instead of `Option`

- **Severity**: LOW
- **Dimension**: Progression & Pools
- **Game**: fo3, fnv
- **Location**: `byroredux/src/npc_spawn.rs:348-356`; consumed in `regen.rs` (`avs.get(config.fatigue_avif)`, `avs.get(config.magicka_avif)`)
- **Status**: NEW
- **Description**: The inert lanes are disabled only by "no actor carries AV 0". An actor ever holding an ActorValue keyed 0 (a failed upstream `resolve`, a fixture) would arm the Fatigue lane (+10/s). The AP lane correctly uses `Option<ActionPointsRegen>`. The code comment discloses the sentinel.
- **Suggested Fix**: Make the three TES ids `Option<u32>`.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-09`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

