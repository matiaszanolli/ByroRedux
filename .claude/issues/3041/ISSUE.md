# FNV-2026-08-16-D5-01: FNV parse-rate regression test never touches the DLC archives

**Issue**: #3041
**Severity**: LOW
**Dimension**: 5 — parse-rate coverage
**Labels**: `low,nif-parser,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_FNV_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 5 — parse-rate regression coverage).

**Location**: `crates/nif/tests/parse_real_nifs.rs`:41-69 (`run_game`), :71-75

## Description

`run_game` opens exactly one archive — `game.mesh_archive()`, i.e. `Fallout - Meshes.bsa` — so the checked-in FNV parse-rate gate covers 14,881 NIFs and **none of the DLC/pack content**. FNV ships eight further mesh-bearing archives.

## Evidence

Manual sweep with `nif_stats` this audit — **all currently clean**, which is why this is LOW and not a live defect:

```
DeadMoney - Main.bsa        950 NIFs   0 truncated  0 recovered
HonestHearts - Main.bsa    1028        0            0
OldWorldBlues - Main.bsa   1621        0            0
LonesomeRoad - Main.bsa    2045        0            0
…
```

## Impact

The regression gate that certifies "FNV NIF parse rate 100% clean" is measuring 14,881 of a substantially larger corpus. A parser change that breaks only DLC content — plausible, since DLC ships later-authored assets — would not turn it red.

No live defect today; this is coverage, not correctness.

## Suggested Fix

Extend `run_game` to iterate every mesh-bearing archive for the game rather than only `mesh_archive()`. The `<stem>N.bsa` sibling auto-load already exists in `asset_provider/archive.rs` and is the natural model.

Keep the per-archive breakdown in the output so a regression names the archive.

## Related

- The ROADMAP-tracked "FNV NIF parse rate 100% clean/recoverable" guard, whose scope this narrows

## Completeness Checks
- [ ] **ALL-ARCHIVES**: Every mesh-bearing archive for each game is walked, not just the base
- [ ] **SIBLING**: The same widening applied to the other games' gates, not only FNV
- [ ] **ATTRIBUTED**: A failure names which archive regressed
- [ ] **TESTS**: The widened gate passes at HEAD (it should — the sweep found all clean)

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3041 --json state` when live state is needed.*
