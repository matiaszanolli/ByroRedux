# CHAR-2026-10-27-L-02: The FO4 and Skyrim `BarterModel` compiled-in curve ends (2.5/1.5, 2.4/1.4) have no capture line

Labels: low, character, game:fo4, game:skyrim, documentation, doc-rot
Source: docs/audits/AUDIT_CHARACTER_2026-10-27.md

**Source**: `docs/audits/AUDIT_CHARACTER_2026-10-27.md` finding `CHAR-2026-10-27-L-02` (LOW)
**Location (re-mapped to current tree)**: `crates/core/src/character/profile.rs` (SKYRIM `barter_max: 2.4, barter_min: 1.4` ~:420-426; FALLOUT4 `2.5 / 1.5` ~:452-461); captures `docs/engine/charal-fo4-ruleset.md` (~:226-247) and `docs/engine/charal-skyrim-ruleset.md` (~:97-116)

**Game**: FO4, Skyrim. Doc/sourcing gap (values are correct). Adjacent to closed #6283 (which corrected only the floor/ceiling sentence).

## Source
This audit read both executables' setting tables (exe_ini_default_tables method): Fallout4.exe `fBarterMax` 2.5, `fBarterMin` 1.5, `fBarterBuyMax` 1.05, `fBarterSellMax` 0.95; SkyrimSE.exe `fBarterMax` 2.4, `fBarterMin` 1.4, `fBarterBuyMin` 1.05, `fBarterSellMax` 0.95. The masters author FO4 3.5/2.0/1.2/0.8 and Skyrim 3.3/2.0 (byte-scan).

## Description
`BarterModel`'s doc defines its fields as "the executable defaults the masters' GMSTs override". The captures state the master values; they name the executable fallback only for the floor/ceiling pair. Neither capture gives the `fBarterMax`/`fBarterMin` fallbacks, and no code comment cites them (literals arrived in `61ad95cf3` with no citation). A reader comparing code to capture sees 2.5 vs 3.5 and 2.4 vs 3.3 - the same shape as the #6424 "wrong fallback" class - with no line to settle it.

## Evidence
`profile.rs` literals above vs the capture text at `charal-fo4-ruleset.md:242-247` (master values + exe floor/ceiling only) and `charal-skyrim-ruleset.md:100-103` (master 3.3/2.0 only).

## Impact
None at runtime (vanilla masters overlay all four). The audit trail is incomplete, so a future "fix" toward the master values would silently change the no-overlay fallback.

## Related
#6283, #5928, #6424.

## Suggested Fix
Add one sentence to each capture's barter section giving the executable's `fBarterMax`/`fBarterMin` fallbacks and the read method; add a `// Fallout4.exe / SkyrimSE.exe setting table` comment on the two literal pairs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems

