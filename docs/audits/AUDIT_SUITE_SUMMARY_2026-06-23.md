# Audit Suite Summary — comprehensive — 2026-06-23

Full `--preset comprehensive` sweep: every subsystem audit, every per-game audit,
the regression pass, and the runtime telemetry diff. 21 audits, run against HEAD
`2d4c350d` (plus the uncommitted M47.2 `crates/scripting` work in the tree).

This sweep is the first to include the two **new** audits authored today —
`/audit-scripting` (crates/scripting + crates/pex + crates/papyrus) and
`/audit-save` (crates/save) — which together account for 26 of the 66 findings,
confirming the coverage gap they were created to close.

## ✅ No CRITICAL findings.

7 HIGH findings cluster in exactly two places: the brand-new scripting domain
(4) and the save/load subsystem (1) — both previously unaudited — plus one ECS
HIGH that is actually a broken test build in the in-progress M47.2 work, and one
runtime HIGH (a Skyrim warm-up FPS collapse).

## Results

| Audit | Findings | CRIT | HIGH | MED | LOW | Report |
|-------|---------:|-----:|-----:|----:|----:|--------|
| renderer | 1 | 0 | 0 | 0 | 1 | AUDIT_RENDERER_2026-06-23.md |
| ecs | 1 | 0 | 1 | 0 | 0 | AUDIT_ECS_2026-06-23.md |
| safety | 2 | 0 | 0 | 1 | 1 | AUDIT_SAFETY_2026-06-23.md |
| concurrency | 1 | 0 | 0 | 0 | 1 | AUDIT_CONCURRENCY_2026-06-23.md |
| performance | 1 | 0 | 0 | 0 | 1 | AUDIT_PERFORMANCE_2026-06-23.md |
| nif | 1 | 0 | 0 | 1 | 0 | AUDIT_NIF_2026-06-23.md |
| nifal | 0 | 0 | 0 | 0 | 0 | AUDIT_NIFAL_2026-06-23.md |
| audio | 1 | 0 | 0 | 0 | 1 | AUDIT_AUDIO_2026-06-23.md |
| speedtree | 4 | 0 | 0 | 0 | 4 | AUDIT_SPEEDTREE_2026-06-23.md |
| **scripting** ⭐ | 16 | 0 | 4 | 5 | 7 | AUDIT_SCRIPTING_2026-06-23.md |
| **save** ⭐ | 10 | 0 | 1 | 6 | 3 | AUDIT_SAVE_2026-06-23.md |
| legacy-compat | 3 | 0 | 0 | 2 | 1 | AUDIT_LEGACY_COMPAT_2026-06-23.md |
| tech-debt | 9 | 0 | 0 | 1 | 8 | AUDIT_TECH_DEBT_2026-06-23.md |
| fnv | 4 | 0 | 0 | 1 | 3 | AUDIT_FNV_2026-06-23.md |
| fo3 | 4 | 0 | 0 | 1 | 3 | AUDIT_FO3_2026-06-23.md |
| skyrim | 0 | 0 | 0 | 0 | 0 | AUDIT_SKYRIM_2026-06-23.md |
| oblivion | 0 | 0 | 0 | 0 | 0 | AUDIT_OBLIVION_2026-06-23.md |
| fo4 | 4 | 0 | 0 | 0 | 4 | AUDIT_FO4_2026-06-23.md |
| starfield | 1 | 0 | 0 | 0 | 1 | AUDIT_STARFIELD_2026-06-23.md |
| regression | 0 | 0 | 0 | 0 | 0 | AUDIT_REGRESSION_2026-06-23.md |
| runtime | 3 | 0 | 1 | 1 | 1 | AUDIT_RUNTIME_2026-06-23.md |
| **Total** | **66** | **0** | **7** | **19** | **40** | |

⭐ = newly-created audit, first run.

## HIGH findings (read these first)

1. **scripting — 4 HIGH** (`AUDIT_SCRIPTING`). Concentrated in the `.pex`
   decompiler (untrusted-bytecode path, the highest-bug-density surface) and the
   recognizer-chain `decline-on-unmodeled` invariant. See the report for the
   specific dimensions; a wrong recognizer lowering is silent, all-game
   game-logic corruption with no fallback (the scripting analogue of NIFAL's
   no-fabrication rule).
2. **save — 1 HIGH** (`AUDIT_SAVE`). The standout: two parallel completeness
   lists that can silently drift — `build_save_registry` decides what is *saved*,
   a separate hardcoded `MUTABLE_DELTA_COLUMNS` decides what is *replayed* on a
   live load. A column in one but not the other is written to disk yet never
   overlaid → silent progress loss. (Plus a HIGH-adjacent `form_id_column()`
   mis-key trap among the MEDIUMs.)
3. **ecs — 1 HIGH** (`AUDIT_ECS`). **This is in your uncommitted M47.2 work, not
   committed code**: `crates/scripting/src/fragment.rs` declares `mod tests;`
   with no backing `fragment/tests.rs`, so the crate compiles as a library but
   `cargo test` fails with E0583, blocking the whole-workspace test gate. Quick
   fix before you commit the fragment-dispatch work.
4. **runtime — 1 HIGH** (`AUDIT_RUNTIME`). Skyrim `WhiterunDragonsreach`
   bench-window FPS collapses 321→8.7 (reproduced twice), pinned to the ECS
   `atw_scheduler` stage stalling ~140 ms/frame for the first ~28 s, then fully
   recovering to 555+ fps. A CPU-side scheduler warm-up cost unique to the
   heaviest cell (the other four games show systems_ms < 1.2 ms, zero slow
   frames). All visible-symptom HIGH-gate metrics (tex.missing / mesh.cache
   failed / skin overflow) passed on every game; Skyrim's mesh_fail even
   improved 11→9.

## Clean audits (verified, zero findings)

nifal, skyrim, oblivion, regression — all re-verified their invariants/regression
guards hold; skyrim & oblivion burned real game data (22k / 8k NIFs at ~100%
clean). Regression confirmed the recent fix wave (#1590/#1592/#1594/#1606/#1650/
#1651/#1652/#1656/#1658) has not regressed.

## Housekeeping surfaced (not bugs)

- **#1661 is stale-open**: its fix (`821a425b`, zero-based LOD sibling auto-load)
  already landed; the issue should be closed. (from `AUDIT_SKYRIM`)
- **`docs/feature-matrix.md` doc-rot**: the "Save / load (M45)" row says
  "unstarted" and the M47.2 transpiler row reads stale, but M45/M45.1 and the
  M47.2 `.pex` slice all shipped. (from `AUDIT_TECH_DEBT` + `AUDIT_SCRIPTING` +
  `AUDIT_SAVE`)
- Several existing OPEN issues re-confirmed still present (not new): #1539
  (ragdoll body/constraint drop on bone-name miss), #1660 (deleted-REFR
  tombstones), #1659 (BSDismember partition flags), #1560 (no 6-NPC equip-count
  guard), #1219 (benign FNV diagnostic).

## Published — 48 GitHub issues filed (#1696–#1743)

All NEW findings were published via `/audit-publish` (2026-06-23). Of 52 NEW
findings parsed: **48 filed**, 4 consciously withheld —
- feature-matrix doc-rot deduped to one owner (tech-debt #1699/#1703); scripting
  & save copies skipped.
- `SPT-NEW-01` skipped STALE (its "dead code" premise was false — a live
  `spt_dissect` example consumes `detect_variant`).
- `TD8-002` skipped non-actionable (self-classified "likely intentional").
- `ECS-2026-06-23-01` skipped — it's in uncommitted M47.2 WIP, a local fix, not
  a fileable repo bug.

**6 HIGH issues filed:** #1696 (save — save/replay completeness-list drift),
#1698 (runtime — Skyrim scheduler FPS collapse), #1710 / #1712 / #1719 / #1727
(scripting — varargs OOM-prealloc, `.psc` recursion stack-overflow,
quest_stage_gate sibling-drop, OnTriggerEnter never drained → every-frame
re-advance).

| Report | Issues filed |
|--------|--------------|
| scripting | #1710 #1712 #1719 #1727 #1728 #1732 #1734 #1736 #1737 #1738 #1739 #1740 #1741 #1742 #1743 |
| save | #1696 #1697 #1700 #1702 #1706 #1708 #1714 #1716 #1720 |
| tech-debt | #1699 #1703 #1704 |
| runtime | #1698 #1701 #1705 |
| speedtree | #1707 #1711 #1715 |
| legacy-compat | #1722 #1726 #1731 |
| per-game (fnv/fo3/fo4) | #1718 #1723 #1724 #1730 #1733 #1735 |
| starfield | #1717 |
| nif | #1721 |
| audio / concurrency / performance / safety | #1709 #1713 #1725 #1729 |

**Label-gap note for the repo owner:** the M45 save subsystem, the scripting/pex
domain, audio, and physics(PHYSAL) have no dedicated labels — findings were
mapped to the closest existing (`import-pipeline` / `legacy-compat` / `tech-debt`
/ `sync` / `safety`). A `save` and/or `scripting` domain label may be worth
adding if those areas keep accruing issues.

## Next steps (original)

Publish the reports that carry findings (skip the four zero-finding ones —
nifal/skyrim/oblivion/regression):

```
/audit-publish docs/audits/AUDIT_SCRIPTING_2026-06-23.md   # 16 — start here
/audit-publish docs/audits/AUDIT_SAVE_2026-06-23.md        # 10
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-06-23.md   # 9
/audit-publish docs/audits/AUDIT_FNV_2026-06-23.md         # 4
/audit-publish docs/audits/AUDIT_FO3_2026-06-23.md         # 4
/audit-publish docs/audits/AUDIT_FO4_2026-06-23.md         # 4
/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-06-23.md   # 4
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-06-23.md # 3
/audit-publish docs/audits/AUDIT_RUNTIME_2026-06-23.md     # 3
/audit-publish docs/audits/AUDIT_SAFETY_2026-06-23.md      # 2
/audit-publish docs/audits/AUDIT_RENDERER_2026-06-23.md    # 1
/audit-publish docs/audits/AUDIT_ECS_2026-06-23.md         # 1 (or just fix the test-mod locally)
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-06-23.md # 1
/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-06-23.md # 1
/audit-publish docs/audits/AUDIT_NIF_2026-06-23.md         # 1
/audit-publish docs/audits/AUDIT_AUDIO_2026-06-23.md       # 1
/audit-publish docs/audits/AUDIT_STARFIELD_2026-06-23.md   # 1
```
