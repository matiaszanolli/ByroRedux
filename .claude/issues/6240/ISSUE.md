# #6240: ESM-2026-10-21-02: FO76 NPC_ ACBS (20 bytes, FO4 layout) matches no parse arm

**Labels**: medium,bug,esm-plugin,game:fo76

**Source**: docs/audits/AUDIT_ESM_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Record Schema Dispatch · **Location**: `crates/plugin/src/esm/records/actor/npc.rs` (ACBS arms)

- **Record / Sub-record**: `NPC_` / `ACBS`
- **Location**: `crates/plugin/src/esm/records/actor/npc.rs:793-889` (arms gated at 793, 820-822, 852, 867)
- **Status**: NEW. This is the #6120 shape, fixed only for Starfield.
- **Description**: The ACBS arms are tried in this order:

  | Arm | Matches |
  |---|---|
  | Oblivion | Oblivion only |
  | FO4 | `Fallout4 && len ≥ 20` |
  | Starfield | `Starfield && len ≥ 16` |
  | Skyrim | `Skyrim && len ≥ 24` |
  | Generic FNV | `len ≥ 24` |

  FO76's ACBS (`wbDefinitionsFO76.pas:12893-12941`) is the FO4 layout: flags u32, XP offset, level, calc min/max, disposition, template flags, bleedout override, 2 unused bytes. That is 20 bytes, which is shorter than every non-FO4/Starfield arm requires.
- **Evidence**: SeventySix.esm, 6,551 `NPC_` records, `ACBS` size histogram `{20: 6551}`. With `GameKind::Fallout76`, none of the guards above passes.
- **Impact**: Every FO76 NPC has `acbs_flags = 0`, so it is male (female-only outfit ARMAs have no mesh), not essential/protected/unique, and not auto-calc. `level` stays at its constructor default and `template_flags = 0`, so no TPLT inheritance runs and every leveled NPC shell spawns without its template's traits, inventory or model. `essential`/`protected` (#5921) are derived from the zero flags.
- **Related**: #6120 (Starfield, closed), ESM-2026-10-21-03.
- **Suggested Fix**: widen the FO4 arm to `matches!(game, Fallout4 | Fallout76) && len >= 20`, with the xEdit citation and a SeventySix-shaped fixture.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
