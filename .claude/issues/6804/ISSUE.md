# ESM-D7-04: `memory-budget.md` EsmIndex table is 6-11% low on RSS after this window's decoders

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6804
Labels: low, esm-plugin, documentation, doc-rot
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Location**: `docs/engine/memory-budget.md:68-79` (2026-09-09 table)
- **Status**: NEW (the page's own rule: "re-run whenever the crate's decoder count moves")
- **Evidence**: `target/release/examples/esm_dim8_bench` (built 10:21 today, HEAD), `/usr/bin/time -f %M`, one run each, host busy with other agents so times are noisy, RSS is the usable column. Doc units are MiB (Oblivion.esm is 277,504,985 B = 264.7 MiB = the "265 MB" row).

  | Master | doc RSS | now RSS | delta | doc parse | now parse (loaded host) |
  |---|---|---|---|---|---|
  | Oblivion | 1 466 | 1 608 | +9.7% | 1.79 s | 2.00 s |
  | Fallout3 | 1 060 | 1 126 | +6.2% | 1.38 s | 1.49 s |
  | FalloutNV | 867 | 915 | +5.6% | 1.27 s | 1.33 s |
  | Fallout4 | 1 467 | 1 631 | +11.2% | 1.96 s | 2.23 s |

  Skyrim, SeventySix, Starfield not run (Skyrim path not at the test_paths default on this box; the other two exceed safe headroom with 8 GB free).
- **Growth analysis**: the three maps added since baseline (`volumetric_lighting`, `god_rays`, `music_tracks`) hold 0 records on FNV and a few hundred at most elsewhere; they do not explain the delta. The growth is decoder fidelity (PERK entries, EXPL/PROJ, REGN polygons/data, BPTD parts, CRVA, story-manager fields) and is inside the "8-37% drift per ~14 decode fixes" the page already documents. No unbounded growth: `SharedParseCache` still hands one resident `Arc` per plugin set. The only second copies of index data are `PerkEntryTable` (`crates/scripting/src/perk_entry_points.rs:162-210`, clones each perk's `condition_tabs` plus a `rank_limits` map over all perks) and `WeaponSkillTable` (one `(u32,u32)` per skill weapon); both are small next to the 0.6-1.2 GB index.
- **Suggested fix**: re-run the seven masters once and refresh the table; add the date.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
