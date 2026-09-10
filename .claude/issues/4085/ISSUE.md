# #4085 — ESM-2026-09-09-D8-01

`Starfield.esm` parse cost is now measured — 5.27 s and 3.4 GB peak RSS, closing the one gap the 2026-08-30 run left open

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4085 --json state`).

---

- **Severity**: LOW (INFO-grade; recorded as a durable baseline, not a defect)
- **Dimension**: Real-Data Validation
- **Record / Sub-record**: —
- **Location**: `crates/plugin/src/esm/records/mod.rs::parse_esm_with_load_order`
- **Status**: NEW (completes ESM-2026-08-30-D7-02 / #3718's table)
- **Description**: The 2026-08-30 report's cost table reads *"Starfield.esm — not run: no
  safe headroom on this host"* and extrapolated the FO76 ratio to "near 4 GB". Measured
  under a hard `ulimit -v 12000000` cap: **5.27 s parse, 3 516 600 KiB (3.35 GiB) peak
  RSS** on a 1.36 GiB file. The extrapolation was slightly pessimistic; the ~2.5× file →
  RSS multiplier is stable across the lineage.
- **Evidence** (release build, `/usr/bin/time -f %M`, one process per row; `parse` is the
  probe's own `parse_esm` timer, `elapsed` includes file read):

  | master | file | read | parse | peak RSS | elapsed | index ≈ RSS − file | Δ parse vs 2026-08-30 |
  |---|---|---|---|---|---|---|---|
  | `Oblivion.esm`   | 265 MB  | 0.12 s | **1.79 s** | 1 466 MB | 2.09 s | ~1.20 GB | 1.41 → 1.79 s |
  | `Fallout3.esm`   | 275 MB  | 0.12 s | **1.38 s** | 1 060 MB | 1.62 s | ~0.78 GB | 1.23 → 1.38 s |
  | `FalloutNV.esm`  | 234 MB  | 0.10 s | **1.27 s** |   867 MB | 1.48 s | ~0.63 GB | 1.17 → 1.27 s |
  | `Skyrim.esm`     | 238 MB  | 0.10 s | **1.40 s** |   985 MB | 1.63 s | ~0.75 GB | 1.27 → 1.40 s |
  | `Fallout4.esm`   | 315 MB  | 0.13 s | **1.96 s** | 1 467 MB | 2.28 s | ~1.15 GB | 1.69 → 1.96 s |
  | `SeventySix.esm` | 880 MB  | 0.35 s | **4.68 s** | 3 566 MB | 5.40 s | ~2.68 GB | 3.41 → 4.68 s |
  | **`Starfield.esm`** | **1 390 MB** | 0.83 s | **5.27 s** | **3 434 MB** | 6.42 s | ~2.05 GB | **not previously run** |

- **Impact**: two things worth recording. (a) Every master got **8–37 % slower** in the
  ten days since the last measurement (Oblivion +27 %, FO4 +16 %, FO76 +37 %) — the
  expected cost of the ~14 decode fixes that landed in that window, but it is on the
  critical path of the first cell load and, per ESM-2026-08-30-D7-03, still has no owner.
  A vanilla FO4 order (base + 7 DLC) is now ~3.0 s of single-threaded ESM parsing before
  the first cell can load. (b) Starfield now has a number, so `docs/engine/memory-budget.md`'s
  new "ESM Index (CPU-side)" section (added by `f2a91b2b`) can be completed rather than
  carrying an extrapolation.
- **Related**: #3718, ESM-2026-08-30-D7-02, ESM-2026-08-30-D7-03; Dim 7's D7-08.
- **Suggested Fix**: add the Starfield row to `docs/engine/memory-budget.md`'s ESM-index
  table with the measured 3.35 GiB, and re-run this probe whenever the crate's decoder
  count moves — the +37 % on FO76 in ten days is the kind of drift that only shows up if
  someone is watching.

---
---

## Record Coverage Matrix

Top-level GRUP routing over every installed vanilla master. "Routed" = the record sits
under a top-level GRUP label the dispatcher has an arm for. **119 labels** extracted
programmatically from the `match &label` block in
[`crates/plugin/src/esm/records/mod.rs`](crates/plugin/src/esm/records/mod.rs); measured by
an independent Python walker that attributes each record to its enclosing `group_type == 0`
label. This table is the durable artifact — keep it even when there are no findings.

| game | records | routed | % | unrouted | labels | top unrouted labels | Δ vs 2026-08-30 |
|---|---|---|---|---|---|---|---|
| Oblivion  | 1 167 016 | 1 166 962 | **100.0** | **54** | **2** | SBSP 33, SKIL 21 | **360 → 54; 3 → 2 labels** |
| FO3       |   718 951 |   718 951 | **100.0** | 0 | 0 | — | unchanged |
| FNV       |   465 016 |   465 016 | **100.0** | 0 | 0 | — | unchanged |
| Skyrim SE |   869 687 |   858 332 | 98.7 | 11 355 | 32 | DLBR 3061, SNDR 2453, DLVW 1126, KYWD 825, RELA 642 | 11 379 → 11 355 |
| FO4       | 1 549 276 | 1 524 756 | 98.4 | 24 520 | 46 | RFGP 6116, SNDR 5475, LAYR 3832, KYWD 3539, TRNS 949 | unchanged |
| FO76      | 5 635 950 | 5 528 732 | 98.1 | 107 218 | 87 | RFGP 29599, SNDR 15475, LAYR 11540, KYWD 10544, LCRT 7051 | 89 → 87 labels |
| Starfield | 3 829 246 | 3 682 200 | 96.2 | 147 046 | 93 | RFGP 80584, LMSW 12966, LAYR 6348, AVMD 6154, LCTN 6017 | 95 → 93 labels |

**No routing regression, and one real improvement**: Oblivion's unrouted count fell by 306
records because `LVSP` now dispatches (#3617, `6653b453`), leaving only `SBSP` (33) and
`SKIL` (21) — both recorded as deliberate non-goals under #3619. `PGRD` (#3598) is handled
inside the cell tier. The remaining modern gaps are the same four families the last audit
named: keyword/relationship metadata (`KYWD` `AACT` `RELA`), the Creation-era audio graph
that replaced `SOUN` (`SNDR` `SOPM` `SNCT` `MUST`), the Story Manager, and reference groups
(`RFGP`).

### Sub-record coverage (a code never matched anywhere in the crate)

Lower bound — a code handled for one record type counts as handled for all. 434 distinct
4-char codes appear in `b"…"` literals across `crates/plugin/src`.

| master | sub-record occurrences | never-matched | share | Δ share | distinct (rec, sub) pairs |
|---|---|---|---|---|---|
| `Oblivion.esm`  | 4 574 981 | 200 693 | **4.39 %** | 4.98 % → 4.39 % | 53 (was 64) |
| `Fallout3.esm`  | 2 634 084 | 105 005 | **3.99 %** | 4.20 % → 3.99 % | 214 (was 217) |
| `FalloutNV.esm` | 2 321 456 | 108 912 | **4.69 %** | 4.87 % → 4.69 % | 240 (was 243) |
| `Skyrim.esm`    | 4 131 332 | 412 459 | **9.98 %** | 10.03 % → 9.98 % | 301 (was 306) |
| `Fallout4.esm`  | 6 742 917 | 887 454 | **13.16 %** | 13.18 % → 13.16 % | 495 (was 501) |

Every master improved; none regressed.

### Per-record-type unknown-sub-record share — the schema-split signal

| record | OBL | FO3 | FNV | SSE | FO4 |
|---|---|---|---|---|---|
| `RACE` |  2.3 % |  7.0 % |  7.0 % | **90.7 %** | **92.2 %** |
| `NPC_` |  7.8 % | 11.3 % | 10.2 % | **57.4 %** | 18.8 % |
| `ARMA` |   —    | 11.9 % |  9.1 % | 17.8 % | **54.6 %** |
| `FACT` |   —    |   —    |   —    | **41.3 %** | **41.5 %** |
| `WEAP` |   —    | 16.7 % | 19.6 % | 21.2 % | **41.2 %** |
| `SOUN` |   —    |   —    |   —    | 23.7 % | 37.3 % |
| `ARMO` | 25.8 % | 34.3 % | 27.2 % | 16.5 % | 32.1 % |
| `LVLI` |   —    | 12.3 % |  7.5 % |  8.7 % | 21.7 % |
| `TREE` |   —    |   —    |   —    | 15.1 % | 16.7 % |
| `PERK` |   —    |  2.2 % |  4.3 % |  8.7 % | 14.3 % |
| `WTHR` |   —    | 16.5 % | 22.0 % |  8.2 % | 13.5 % |
| `CELL` |   —    |  0.3 % |  0.0 % | 19.9 % | 13.4 % |
| `REFR` |  6.0 % |  3.7 % |  3.9 % |  2.6 % | 14.5 % |
| `INFO` |  0.0 % |   —    |  0.5 % |  0.2 % | **10.8 %** |

`SOUN` improved sharply on Skyrim (39.4 % → 23.7 %) after #3775 / #3914; `ARMA` improved on
Skyrim (18.8 % → 17.8 %) and FO4 (56.3 % → 54.6 %). `RACE` / `NPC_` remain the face/tint
model — `/audit-skyrim` and `/audit-fo4` Dim 1 territory, not the parser's. `INFO` at 10.8 %
on FO4 is D4-01's `TRDA` (74 996 occurrences) plus its `NAM3`/`NAM4`/`NAM9`/`TIQS`/`INCC`
siblings.

### Header detection & walk integrity

Verified against real bytes on **170 installed plugins**, not only the seven vanilla
masters. Every one classifies correctly.

| master | HEDR | rec ver | variant | `GameKind` | walked to EOF | walker errors |
|---|---|---|---|---|---|---|
| `Oblivion.esm`   | 1.00 | 0 | Oblivion | Oblivion | yes | 0 |
| `Fallout3.esm`   | 0.94 | 2 | Tes5Plus | Fallout3NV | yes | 0 |
| `FalloutNV.esm`  | 1.34 | 2 | Tes5Plus | Fallout3NV | yes | **0** (was 1) |
| `Skyrim.esm`     | 1.71 | 44 | Tes5Plus | Skyrim | yes | 0 |
| `Fallout4.esm`   | 1.00 | 131 | Tes5Plus | Fallout4 | yes | 0 |
| `SeventySix.esm` | **266.0** | 209 | Tes5Plus | Fallout76 | yes | 0 |
| `Starfield.esm`  | 0.96 | 581 | Tes5Plus | Starfield | yes | 0 |

FNV's error column is now zero — #3720's Adler-32 recovery confirmed working on the real
`LAND 0x00150FC0`. Coverage the seven-master sample never reached: 40+ FO4 mod `.esp`/`.esl`
files stamp HEDR `0.95` and are correctly rescued by the `record_version >= 100`
discriminator (all ship `131`); **no** FO3/FNV plugin anywhere on disk has
`record_version >= 100` (all are `2` or `15`), so the FO4 arm cannot steal an FO3 record;
Oblivion DLC `.esp`s ship HEDR `0.8` but are routed by header size before `from_header` sees
the float; Skyrim CC `.esl`s ship `1.70`, inside the `1.6..=1.8` band.

### Parse cost + peak RSS (release build, one master per process)

| master | file | read | parse | peak RSS | index ≈ RSS − file | Δ parse vs 2026-08-30 |
|---|---|---|---|---|---|---|
| `Oblivion.esm`   | 265 MB  | 0.12 s | **1.79 s** | 1 466 MB | ~1.20 GB | 1.41 → 1.79 s |
| `Fallout3.esm`   | 275 MB  | 0.12 s | **1.38 s** | 1 060 MB | ~0.78 GB | 1.23 → 1.38 s |
| `FalloutNV.esm`  | 234 MB  | 0.10 s | **1.27 s** |   867 MB | ~0.63 GB | 1.17 → 1.27 s |
| `Skyrim.esm`     | 238 MB  | 0.10 s | **1.40 s** |   985 MB | ~0.75 GB | 1.27 → 1.40 s |
| `Fallout4.esm`   | 315 MB  | 0.13 s | **1.96 s** | 1 467 MB | ~1.15 GB | 1.69 → 1.96 s |
| `SeventySix.esm` | 880 MB  | 0.35 s | **4.68 s** | 3 566 MB | ~2.68 GB | 3.41 → 4.68 s |
| `Starfield.esm`  | 1 390 MB | 0.83 s | **5.27 s** | 3 434 MB | ~2.05 GB | **first measurement** |

Every master got 8–37 % slower in the ten days since the last measurement — the expected
cost of the ~14 decode fixes that landed, but see D8-01: it is on the critical path of the
first cell load, and #3729 closed the *ownership* question without anyone watching the
number.

---

## Prior-Audit Findings — Status at HEAD

Every finding from `docs/audits/AUDIT_ESM_2026-08-30.md` (the last full sweep) and
`AUDIT_ESM_2026-09-04.md` (the water-scoped run) was re-verified **against the code**, not
against issue state alone. **Fourteen of fifteen landed correctly; none regressed.**

| prior finding | issue | commit | status at HEAD |
|---|---|---|---|
| 08-30 D3-01 `parse_armo`/`parse_arma`/`parse_race` remap | #3714 | `1ee804c2` | **Fixed** — all three take `remap`; allowlist extended |
| 08-30 D2-01 Skyrim `BOOK.DATA` 16-byte | #3716 | `aa5ef326` | **Fixed** — dedicated 16-byte arm; 10-byte arm length-gated |
| 08-30 D3-02 remap allowlist / 11 sites | #3715 | `29f68e1d` | **Fixed for the named set** — but the allowlist shape survives; see D3-02, D7-02, D7-03 |
| 08-30 D7-02 `EsmIndex` RAM absent from memory-budget | #3718 | `f2a91b2b` | **Fixed** — `## ESM Index (CPU-side)` section added; residue in D7-08 |
| 08-30 D8-01 FNV `LAND` Adler-32 | #3720 | `ee5e8ea6` | **Fixed and confirmed on real data** — FNV walker errors 1 → 0 |
| 08-30 D7-01 `merge_from` `game` last-write-wins | #3403 | `ba167c57` | **Fixed** — `other.total() > 0` guard present with two directional tests |
| 08-30 D1-01 nested `sub_end` parent clamp | #3721 | `2ec58c43` | **Fixed at 14 of 15 sites** — one raw call site missed; see D1-01 |
| 08-30 D1-02 unknown-GRUP telemetry | #3722 | `5ddeb3c1` | **Fixed** — deduplicated per label into `skipped_unconsumed_groups` |
| 08-30 D2-02 Skyrim `AMMO.DATA` 20 bytes | #3723 | `399c2043` | **Fixed** |
| 08-30 D2-03 item `DATA` length validation | #3724 | `36c84327` | **Fixed** — every arm gated on its measured width with a paired fall-through |
| 08-30 D3-03 `parse_esm_with_load_order` doc rot | #3725 | `f2a91b2b` | **Fixed** |
| 08-30 D4-01 `from_header` stale "latent" note | #3727 | `f2a91b2b` | **Fixed** |
| 08-30 D5-01 REFR group membership discarded | #3728 | `2c86cacd` | **Landed but defective** — field always `6`; see D5-01 |
| 08-30 D7-03 ESM parse cost has no owner | #3729 | — | **Closed as an ownership decision**; the number itself has since drifted +8–37 %, see D8-01 |
| 08-30 D8-02 `record_count` per-game semantics | #3730 | `d52d2a91` | **Fixed** (documented, not asserted on) |
| 09-04 D5-01 XCLW sentinel walker test gap | #3827 | `c2796db3` | **Fixed** — walker-level sentinel tests exist with the exact asserts |
| 09-04 D5-02 WRLD whole-record merge | #2370 | — | Unchanged, deliberate, documented in-code. Not re-filed. |
---

## Verified Clean (no findings)

Recorded with the concrete evidence, so the next auditor does not re-derive them.

**Dimension 1 — header & GRUP walk**
- `EsmVariant::detect` guards `data.len() >= 24` before the `data[20..24]` slice; the only
  non-`detect` construction paths are `with_variant` and test fixtures.
- `GameKind::from_header`'s bands classify 170/170 installed plugins correctly (table above).
  The #439 FO3↔FO4 inversion is absent.
- `FLAG_COMPRESSED` (#3399): `ensure!(data_size >= 4)` precedes the subtraction; the
  `min(64 MiB, max(64 KiB, compressed_len × 512))` ceiling is checked *before*
  `Vec::with_capacity`; the decoder is held to `take(declared + 1)` with a post-check. Still
  in lockstep with `byroredux_bsa::safety::inflate_bounded`.
- #3720's raw-DEFLATE retry is conservative: accepted **only** on an exact declared-length
  match, `ensure!`s `compressed.len() >= 2` before slicing, re-applies the same ceiling, and
  logs once at `warn` naming the FormID. It cannot turn a bad stream into a silent success.
- Nesting bound: `MAX_GRUP_NESTING_DEPTH = 64`, 14 production `bounded_group_content_end`
  call sites, every one passing both `depth` and its own `end`.

**Dimension 2 — sub-record byte accounting**
- #3724's length guards are complete across `parse_weap`/`parse_armo`/`parse_ammo`/
  `parse_book`/`CRDT`/`DNAM`/`ENIT`: each `GameKind` arm is gated on its measured on-disk
  width with a paired empty fall-through, so the narrowing-pair hazard is closed *by
  construction* rather than by a fallible runtime check.
- #3614's `TCLF`/`NAME`/`CTDT` decode reproduces on real bytes: my independent census
  matches the commit's Oblivion numbers exactly (`NAME` 1 342, `TCLF` 4 141, `CTDT` 72,
  `TRDT` 23 877). The ungated `NAME`/`TCLF` arms are safe across games — 4 bytes in every
  occurrence on FO3 (146 / 4 948) and FNV (737 / 1 008); Skyrim and FO4 author neither. The
  "CTDT is a 20-byte CTDA prefix" premise holds on 72/72.
- `TRDT` width drift is handled: FO3 ships `{16: 2230, 20: 1589, 24: 25644}`, FNV/Skyrim only
  24, Oblivion only 16. `sub.data[0]` is behind `!is_empty()` and `sub.data[12]` behind
  `len() >= 13`, so all four widths decode without a stray-byte read.
- `gras.rs` (#3807, new this cycle) is the best-cited decoder in the crate: layout
  cross-referenced against OpenMW *components/esm4/loadgras.cpp* **and** verified against all
  168 installed records, with per-field measured ranges and an explicit warning that the four
  padding offsets hold uninitialised memory (values enumerated) and must stay skipped.
- No unguarded raw slicing in any newly-added arm read this pass.

**Dimension 3 — FormID remap**
- `GlobalSlot::compose` masks `0x0FFF` on **both** the ESL sub-index and the object id.
- `FormIdRemap::remap`'s four arms are intact and in the right order, including the
  `raw == 0` NULL short-circuit and the standalone-no-masters **pass-through** — the #1308
  rationale comment is present and it has **not** been "fixed" into a clamp.
- `CTDA` parameters go through `push_ctda` → `remap_condition_form_ids`; #3614 extended the
  same path to the legacy `CTDT` tag.
- ~30 embedded-FormID sites in `crates/plugin/src/esm/records/misc/quest.rs` are remapped without exception; `parse_otft`,
  `parse_eczn`, `parse_acti`, `parse_term`, `parse_scen`, `parse_scol`, `parse_pkin`,
  `parse_movs`, `parse_flst`, `parse_tree`, `parse_armo`, `parse_arma`, `parse_race`,
  `parse_weap`, `parse_ammo`, `parse_cobj` likewise.

**Dimension 4 — schema dispatch**
- `ACRE` is still ungated by game (`cell/walkers.rs:779`); #3755's hazard stays closed.
- The FO4/Starfield `QUST`-nested dialogue tree is walked (`extract_quest_dialogue_scene_tree`
  with `QUST`/`DIAL`/`INFO`/`SCEN` arms) — ESM-D4-02 from 2026-08-13 is fixed.
- `merge_from`'s `other.total() > 0` guard for `game` is present, separate from the
  `character_rules` guard, with tests in both directions. A programmatic field ↔
  `categories()` diff (99 fields / 94 `HashMap`s / 100 category rows) found the 5 uncounted
  fields are metadata and **all** explicitly merged; the only maps added since 2026-08-30
  (`leveled_spells`, `grasses`) both have rows. No repeat of the 41-of-92 defect.
- The FourCC → `FormType` table conversion (`e142e3d4`) is lossless: 0 dropped, 0 changed,
  1 added, sorted and unique.

**Dimension 5 — CELL/WRLD** *(all four child sub-groups reached; VWD content is not lost,
only mislabelled — see D5-01)*
- Lighting-template inheritance is genuinely **per-field**, with "absent" and "authored
  zero" distinguishable.
- The `XCLW` tri-state holds at the walker level, and the sentinel tests #3827 promised
  exist at HEAD with the exact asserts.
- Worldspace `PNAM` inheritance is per-category and cycle-guarded.
- `LAND` stride arithmetic is correct (`VHGT` is 1 096 B in all five masters checked).
- `NAVM` row decoders are genuinely shared, the `NVNM` cursor is bounds-checked,
  `NVNM_MAX_DIVISOR` is enforced, and #3404's strictness is present.

**Dimension 6 — localized strings**
- **The dimension's headline prior finding, ESM-D6-01 (`.STRINGS` discovery is loose-file
  only), is FIXED**: `StringTableSet::load_with_archive` + `ArchiveStringSource` read tables
  out of BSA/BA2, and the archive-discovery predicate finds the right archive on all four
  modern installs. Skyrim and its four DLC masters resolve correctly. (D6-01 in this report
  is the *language-token* half, which is separate and still live.)
- The lstring branch is flag-driven (TES4 `0x80` via `LocalizedPluginGuard`), never a
  looks-like-a-small-integer heuristic; per-kind parse form is extension-driven, not
  content-sniffed; header/bounds hardening is complete with no panic path.

**Dimension 7 — handoff & Redux-native tier**
- The live resolver is `resolve_entity_by_global_form_id`, not `World::find_by_form_id`.
- `EsmCellIndex::merge_from` covers all 13 fields.
- `equip.rs`'s biped-slot constants carry `wbDefinitions*.pas` citations (the one exception
  is D7-04).
- `manifest`/`record`/`datastore`/`resolver` and `crates/plugin/src/legacy/mod.rs` compile clean against the
  current `Record` shape; unused-but-documented is by design and not reported.

---

## Disproved Candidates (recorded so they are not re-litigated)

1. **`parse_regn` never remaps `out.weather_form`.** It does — in a deliberate **post-pass
   at the end of the function** (`misc/world.rs:1017-1041`), covering `weather_form`,
   region-data map forms, per-row weather/global forms, music, incidental and sound forms.
   Any future automated remap scan must be function-scoped or it will re-file this.
2. **`decode_nvnm` / `decode_nvdp_row` leave `door_form_id` raw.** Same post-pass idiom —
   `parse_navm` remaps `door_form_id`, `mesh_form`, `worldspace_form` and `cell_form` at
   `misc/world.rs:408-415`.
3. **The `6 | 8 | 9` CELL-children arm loses type-10 (Visible Distant) content.** It does
   not: `parse_refr_group_inner` recurses into *any* nested group, so Oblivion's 143 127 VWD
   REFRs under `6→10` groups are all walked. Only their *label* is wrong (D5-01).
4. **FO4 INFO records are never parsed because the tree is nested under `QUST`.** Stale —
   ESM-D4-02 (2026-08-13) was fixed.
5. **`StringTableSet::resolve`'s type-agnostic first-match order can return the wrong
   string.** The 2026-08-30 run disproved this via "zero id overlap"; that *premise* is
   actually false on FO4 (2 overlapping ids were measured this run) but the **conclusion
   still holds** — both ids are used only as `MESG/FULL`, which `.STRINGS`-first resolves
   correctly. Stronger: the type-agnostic order is **load-bearing**, not merely harmless —
   `LSCR/DESC`, `QUST/NNAM` and `MESG/ITXT` all live in `.STRINGS`, so a
   per-sub-record-type table choice would get them *wrong*.
6. **Starfield masters set no ESL flag, so the 12-bit light space may be mis-detected.** No
   installed Starfield plugin sets `0x0200`, so nothing exercises that path and there is no
   observable defect. Whether Starfield relocates the small-master flag is a genuine open
   question — but I have **no cited ground truth** for it and will not guess one. Recorded
   only so the dead end is not re-walked.

Two skill/prior-report premises the data refutes, now corrected in-report rather than
re-derived:

- The skill's Dim-5 checklist (and three doc comments in the code) state the CELL child
  group-type legend as *"6 = temporary, 8 = persistent, 9 = visible-distant"*. Bethesda's
  codes are **6 = Cell Children (container), 8 = Persistent, 9 = Temporary, 10 = Visible
  Distant** — measured from the raw GRUP tree of four masters. See D5-01.
- `wrld.rs:277-291`'s comment asserts Skyrim wraps the worldspace-persistent CELL in an
  outer type-6 group. No shipped plugin does; CELL parents are only ever 1/3/5. See D5-02.

---

## Cross-Audit Pointers

- **`/audit-scripting` Dim 7** owns D4-01's consumer
  ([`crates/scripting/src/dialogue.rs`](crates/scripting/src/dialogue.rs)) and D7-01's
  (`byroredux/src/cell_loader/references/attach.rs`). `VMAD` translation was not audited
  here — only that its FormIDs are remapped, which they are.
- **`/audit-fo4` and `/audit-starfield` Dim 1** — D4-01 (`TRDA`) and D6-01 (`_en` string
  tables) are both title-specific and both have visible symptoms those audits would see
  first: missing dialogue lines and placeholder `<lstring 0x…>` names.
- **`/audit-skyrim` and `/audit-fo4` Dim 1** — the `RACE` (90.7 % / 92.2 %) and `NPC_`
  (57.4 % Skyrim) unknown-sub-record shares are the face/tint model. The parser tier only
  measures the gap; whether it matters is a per-game call.
- **`/audit-character` Dim 4/5** — `actor_value_derive.rs` and the `RaceRecord` starting-pool
  floats were cross-referenced, not re-audited. `0dcb5cf0`'s two-`TPLT`-chain split checks
  out at the parser boundary (see Dim 8's Verified Clean); the CHARAL-side consumption of
  the values is that audit's.
- **`/audit-physics` and the streaming/save owners** — D5-01 is the placement metadata a
  collision/streaming/save consumer would want, and it is currently wrong. Anyone about to
  wire `PlacedRef.group_type` should read D5-01 first.
- **`/audit-oblivion`** — D5-04's 10 dropped terrain layers are at nine named exterior tiles;
  a visual check there is the confirmation.
- **`/audit-performance`** — #3729 closed the ownership question for ESM parse cost; D8-01
  is the number, and it has moved +8–37 % in ten days.

---

## Not Covered (stated plainly)

- **The `#[ignore]`d real-data test suite was never run** (hard constraint,
  *plugin_ignored_tests_oom*). Everything real-data in this report comes from bounded
  standalone walkers or the single-master `esm_dim8_bench` example.
- **`--sf-smoke` resolve-rate baseline was not re-run.** `byroredux/src/sf_smoke.rs` lives in
  the engine binary and running it means launching `byroredux`, which this run was forbidden
  from doing. The 2026-08-30 baseline (78 handled / 176 FourCCs, 86.1 % of GRUP bytes) is
  therefore **unverified this pass**. My independent Starfield routing figure (96.2 % of
  records, 93 unrouted labels) is consistent with it but measures a different quantity and
  is not a substitute.
- **Multi-plugin end-to-end parse.** D3-01's and D7-01's impact is derived from a byte
  census of the DLC masters' authored FormIDs plus a source trace of the lookup, not from
  running `parse_esm_with_load_order` over a real load order — that is what the ignored
  suite does. The census is direct evidence of the *input*; the end-to-end observation is
  the one link taken on inference rather than measurement, and is marked as such.
- **`NW.esm` and the Starfield DLC masters** were header-checked only, not walked.
- **`crates/plugin/src/esm/records/script_instance.rs` (VMAD internals)** — cross-referenced
  for remap coverage only; the decode itself is `/audit-scripting`'s.

---

## Method & Hygiene

- **Deduplication**: `gh issue list --limit 400 --state all` cached to
  `/tmp/audit/esm/issues.json`; all seven prior `docs/audits/AUDIT_ESM_*.md` reports read,
  with `AUDIT_ESM_2026-08-30.md` (the last full sweep) and `AUDIT_ESM_2026-09-04.md` read in
  full; per-game reports checked for ESM-dimension overlap. Every 2026-08-30 finding was
  re-verified against the code at HEAD before this report claimed anything about it.
- **The pre-2026-06-07 caveat was applied.** Six findings here (D6-01, D6-03, D6-04, D6-05,
  D6-06 and the `AUDIT_ESM_2026-08-13.md`-sourced halves of D7-05/D7-06/D7-07) trace to
  reports predating `/audit-publish`, so absence of a GitHub issue is **not** evidence they
  are still open. Each was verified against the **code and, where applicable, the game
  data** — never against issue absence — and each says so in its Status line.
- **Architecture**: Dimensions 1, 2, 3, 4 and 8 were run by the orchestrator directly.
  Dimensions 5, 6 and 7 ran as sub-agents, each required to write to
  */tmp/audit/esm/dim_N.md*; every scratch file was read back in full by the orchestrator
  and this report was built from the file contents, not from agent summaries
  (*feedback_audit_suite_nested_agent_relay*). The two agent HIGHs (D6-01, D7-01) and the
  agent MEDIUM with the largest blast radius (D5-01) were each **independently re-verified
  by the orchestrator** against the source and, for D5-01 and D6-01, against fresh
  measurements of the raw GRUP tree and the FO4 BA2 name table respectively.
- **No guessing** (*feedback_no_guessing*). Where a decode was missing and I had no
  citation — `TRDA`'s field layout on FO4 (20 B) and Starfield (12 B) — the finding says so
  and splits the fix into a layout-free half and a needs-a-citation half rather than
  proposing offsets.
- Report path: `docs/audits/AUDIT_ESM_2026-09-09.md`. Publish with
  `/audit-publish docs/audits/AUDIT_ESM_2026-09-09.md` — domain label `esm-plugin`, plus
  `dialogue` (D4-01), `scripting` (D7-01), `terrain-exterior` (D5-04), `character` (D7-04),
  `doc-rot` (D5-02, D7-05, D7-08), `tech-debt` (D2-01), `test-gap` (D3-02, D7-09),
  `memory` (D8-01); `game:fo4` (D4-01, D6-01, D6-03), `game:starfield` (D4-01, D6-01,
  D6-03), `game:fo76` (D6-01), `game:oblivion` (D5-02, D5-04), `game:skyrim` (D6-07).
