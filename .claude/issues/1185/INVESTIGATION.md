# #1185 Investigation

**Resolution**: tracker close (no code change). Fix already landed on 2026-05-21 across two commits.

## Findings

### Primary fix — already shipped
- **`2b79a2ac`** (docs: re-anchor stale path / line refs in audit skill files (#1229, #1200, #1185))
  - Updated `CLAUDE.md:309`: "Verified against the 30 Starfield texture archives shipped post-Shattered-Space (was 22 as of Session 7; re-checked 2026-05-21, #1185)"
  - Updated `docs/engine/archives.md:392`: matching text

### Coupling fix — also shipped
- **`a9d1dca5`** (Fix #1184: add Starfield BA2 corpus-wide sweep regression test)
  - Added `starfield_full_corpus_ba2_sweep` test at `crates/bsa/tests/ba2_real.rs:439`
  - Discovers archives at runtime — future Bethesda content drops surface as test diff, not silent doc rot
  - This is the in-tree corpus sweep the issue explicitly suggested as the ideal coupling

### Live disk verification
- `ls /mnt/data/SteamLibrary/steamapps/common/Starfield/Data | grep -iE "Textures.*\.ba2" | wc -l` → **30**
- Matches the documented count.

## SIBLING checks (per the issue's completeness list)

### FO4 BA2 count ("53 vanilla FO4 BA2s")
- 21 `Fallout4 - *.ba2` (base game) + 32 `DLC*.ba2` archives (Coast/Nuka/Robot/3 Workshop + 16 UltraHighResolution) = 53
- Math holds. No fix needed.

### NIF parse-rate snapshots
- `CLAUDE.md:286-291` carries the snapshot with explicit "informational only and allowed to drift one sweep behind" caveat
- Sweep date stamped (2026-04-27)
- ROADMAP.md designated authoritative source per #1218 / `0d3c3011`
- Drift-budget acknowledged in-doc; no silent regression risk

## Pattern observation

Same hygiene pattern as #1229 (closed earlier today): doc fix lands, tracker doesn't auto-close. The audit-publish flow could optionally check `git log --grep` for the issue number before filing, but that adds friction for the common case. Filing → close gap stays as manual closeout per `feedback_fix_issue_flow.md`.
