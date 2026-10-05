# #5340 — CHAR-2026-10-05-D1-01: NpcHealthCurve applies fAVDNPCHealthEnduranceOffset to the level term as well — no level-offset setting exists, so the binding is unsourced

- **Labels**: low,character,game:fnv,game:fo3,bug
- **Filed from**: `docs/audits/AUDIT_CHARACTER_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5340

- **Severity**: LOW. It is value-identical on every vanilla FO3/FNV load order, and diverges only when a plugin authors `fAVDNPCHealthEnduranceOffset`.
- **Dimension**: Ruleset Seam (profile row sourcing)
- **Game**: fo3, fnv
- **Location**: `crates/core/src/character/profile.rs`:
  - `NpcHealthCurve::evaluate`: `self.level_multiplier * (level + self.offset)`.
  - `NpcHealthCurve::with_gmst`: `offset: gmst("fAVDNPCHealthEnduranceOffset")`.
  - the `npc_health_curve_overlays_authored_gmsts` pin.
  - The same formula is in `docs/engine/charal-fnv-fo3-ruleset.md`'s Health (NPCs) paragraph: "`fAVDNPCHealthLevelMult·(Level + fAVDNPCHealthEnduranceOffset)`".
- **Status**: NEW (`c71eeed80`).
- **Source**:
  - (1) `strings` over `Fallout3.exe` and `FalloutNV.exe`: the only NPC Health settings are `fAVDNPCHealthEnduranceMult`, `fAVDNPCHealthEnduranceOffset` and `fAVDNPCHealthLevelMult`. There is no level offset.
  - (2) Both masters author the player-side `fAVDHealthEnduranceOffset` = **0.0** (verified this run). Yet the sourced player formulas still anchor the level term. FNV is `100 + 20·END + 5·(Level−1)` per the FO4 capture's "Cross-game Health" table, which also notes "FNV also re-anchors the level term to `(Level − 1)`". With the endurance offset at 0, the player's level anchor cannot come from it.
  - (3) The commit message itself says "Applied per-term it composes to the −10 constant", which is a fit, not a citation.
- **Description**: The vanilla constant (−10 = −5 END + −5 level) is right. The model, though, binds the level term's −1 to a setting named for Endurance. That is the "plausible decomposition" the no-guessing rule warns against: a mod that retunes `fAVDNPCHealthEnduranceOffset` (for example to −2) would move every NPC's Health by `5·Δ` twice instead of once. The overlay test pins exactly that behaviour.
- **Impact**: None on vanilla data; no shipped FO3/FNV plugin authors the setting. Under a mod that does, every auto-calc NPC's Health is off by `level_multiplier·Δ`.
- **Suggested Fix**: Give the level term its own engine constant, for example a `level_anchor: f32 = -1.0` field documented as engine-hardcoded and mirroring the player formula's `(Level−1)`, that no GMST overlays. Alternatively, keep the binding but mark it UNSOURCED in the doc and the capture until an exe disassembly settles it.

_Source: `AUDIT_CHARACTER_2026-10-05.md` (CHAR-2026-10-05-D1-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
