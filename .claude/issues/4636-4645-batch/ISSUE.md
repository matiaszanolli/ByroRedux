# Batch #4636–#4645 (NIFAL-D8/D9 residuals + ESM audit 2026-09-21)

Fetched 2026-09-22. All 10 OPEN.

## #4636 — NIFAL-D8: BGSM merge NIF-first texture precedence unsourced + dead-path (LOW, FO4, binary crate)
- `byroredux/src/asset_provider/material/merge.rs:139-158` — `fill` keeps NIF slot when non-None; unsourced claim "NIF fields take precedence".
- Hard sub-case: 3 FO4 1st-person meshes bind NIF slot `femalebody_msn.dds` (absent from all BA2s); BGSM names `FemaleBody_n.DDS` (present) — shape gets NO normal map at all.
- Fix: source the precedence rule + cite in merge doc; add spawn-time fallback to BGSM chain path when NIF slot fails to resolve at texture-load.
- Tests: pin precedence rule + dead-path fallback test.

## #4637 — NIFAL-D9: #4523 dark-role census false-FAILs when Oblivion data absent (LOW, test-gap, nif crate)
- `crates/nif/tests/translation_completeness.rs:889-975` — unconditionally asserts `total_dark == 8` when ≥1 game resolves; all 8 hits are Oblivion-only. FO4-only run fails.
- Fix: assert ==8 only when Oblivion resolved; per-game zero asserts for other resolved games; SKIP line when Oblivion absent (mirror `probed == 0` pattern).

## #4638 — ESM-D2-01: Oblivion legacy leveled-list encodings (MED, oblivion, plugin crate)
- `crates/plugin/src/esm/records/container.rs` — (a) 8-byte LVLO rows (no Count) dropped: 94 rows / 14 lists empty. (b) LVLD high bit 0x80 = "Calculate from all levels" flag, not chance-none; 11 lists have LVLD=128 → chance_none=128 → `equip.rs:788` (>=100 always-empty) collapses them.
- Fix: accept LVLO >= 8 bytes, Count when >= 10 (default 1); mask LVLD & 0x7F, OR 0x01 into flags when bit 7 set. Pin with fixtures (8-byte LVLO + LVLD=128).
- Sibling: consumers npc_spawn.rs, inventory.rs, attach.rs, equip.rs.

## #4639 — ESM-D3-01: Starfield TES4 master flags wrong bit layout (MED, starfield, plugin crate)
- `crates/plugin/src/esm/reader.rs` — `light_master = flags & 0x0200` unconditional. Starfield: small=0x100, medium=0x400, 0x200=Update. Oblivion/FO3/FNV/SSE-LE/FO76: no ESL support at all.
- Consumer: `byroredux/src/cell_loader/load_order.rs:604` allocate_global_slot; Medium (0xFD) half still open from #4384.
- Fix: decode per GameKind. SF: 0x100 small, 0x400 medium (GlobalSlot::Medium = 0xFD + 8-bit idx + 16-bit id), ignore 0x200. SSE+FO4 keep 0x200. Others never set.
- Tests: pin SF 0x100/0x200/0x400 + existing SSE/FO4 0x200.

## #4640 — ESM-D1-03: inflation ceiling rejects 82 vanilla Starfield SFTR records (MED, starfield, plugin crate)
- `reader.rs` MAX_RECORD_INFLATION_RATIO=512; 82 SFTR records up to 786:1 (834 B → 655,482 B) exceed it. Hard Err → whole-plugin EsmIndex::default() (silent drop of all Starfield.esm).
- Fix: re-derive bound so 786:1 clears with margin (64 MiB cap + take(declared+1) remain guards); fix doc/test census numbers (Oblivion compresses 41,789!); pin SFTR shape; consider per-record skip+warn instead of whole-parse Err.
- Sibling: byroredux_bsa::safety::inflate_bounded same check.

## #4641 — ESM-D2-02: TERM fix still drops FO4 terminal text (MED, fo4, plugin crate)
- `crates/plugin/src/esm/records/misc/world.rs` parse_term — no NAM0/WNAM/UNAM arms; single last-wins body_text; ANAM reads 1-byte flag only.
- 1,818 menu items ANAM=8, 1,804 carry text in UNAM; 467 TERMs WNAM, 291 NAM0; 82 TERMs 2-7 conditional BTXT bodies.
- Fix: UNAM per-item display_text (lstring); record-level NAM0/WNAM; body as Vec<(text, conditions)>.

## #4642 — ESM-D2-03: LTEX.GNAM grass array last-wins (MED, plugin crate + bin consumer)
- `crates/plugin/src/esm/cell/support.rs` GNAM → HashMap::insert one grass per LTEX. 151/184 grass-bearing LTEXs author 2-4 grasses.
- Fix: `landscape_grasses: HashMap<u32, Vec<u32>>` authored order; carry per-lane list through `authored_grass_for_splat_layers` (byroredux/src/cell_loader/terrain.rs:347-358); fix docs/engine/plugin-loading.md:251.

## #4643 — ESM-D1-01: FO76 HEDR 266.0 doc rot (LOW, doc-rot)
- reader.rs:164, 189-213 comments; records/tests.rs:2326; docs/engine/esm-records.md:228; .claude/commands/audit-esm/SKILL.md:96,218 — installed masters now ship 279.0 (2026-09-20 patch).
- Fix: record as patch-dependent with dated samples (266.0 @ 2026-08-28, 279.0 @ 2026-09-20); floor >= 60.0 is load-bearing. Update doc + skill lines.

## #4644 — ESM-D1-02: skip arms seek by child's raw declared size (LOW, plugin crate)
- Skip arms call unclamped skip_group: cell/walkers.rs:196,200; cell/wrld.rs:57,61 + 347,351; reader.rs bounded_group_content_end depth-cap arm; records/grup_walker.rs:249.
- Fix: seek to clamped sub_end (seek_to helper) at all sites; clamp depth-cap skip to parent_end; fixture with overrunning child GRUP in a skip arm.

## #4645 — ESM-D4-01: TRDA payload undecoded (LOW, plugin crate)
- `crates/plugin/src/esm/records/misc/dialogue.rs:234-253` — TRDA splits segments but never decodes payload. FO4 20-byte: Emotion FormID[KYWD] @0, Response number u8 @4, Sound FormID @5, unknown(1), Interrupt u16, 2×alias s32. FO76 same. SF1 12-byte: Emotion KYWD, WEM file u32, Emotion Out f32.
- Fix: decode FO4/FO76 response number + remapped emotion KYWD; SF1 emotion KYWD + WEM id; correct "no cited source" comment. Tests: FO4 20-byte + SF1 12-byte incl. emotion FormID remap.
