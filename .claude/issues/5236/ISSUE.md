# #5236: RT-2026-10-03-02: Oblivion GildedCarafe TSV — newest regen blocks are contract-derived one-row edits buried under an older block that contradicts them; entities_total lags the live capture by one

**Labels**: documentation, low, tech-debt, game:oblivion, doc-rot · **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5236

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-03.md` RT-2 · **Severity**: LOW · **Dimension**: Baseline integrity · **Game / Cell**: oblivion / ICMarketDistrictTheGildedCarafe

**Location**: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`: header lines 1–24 (09-30 #5125 block), 38–44 (10-03 #5189 block), 45–58 (09-29 #5123 block); rows `light_count_directional`, `entities_total`

| | Baseline | Current (HEAD `2c36c29d8`, live capture) |
|---|---|---|
| light_count_directional | 0 | 0 |
| entities_total | 929 | 928 |

## Description
The runtime skill's integrity rule is that each TSV's newest `# regenerated:` block explains the rows and that every row comes from one capture. This file breaks both parts:
- **Header order.** Line 1 is still the 2026-09-30 #5125 block. Its text says "light_count_directional stays 1 (RT-2, #5123, fixed)", but the row says 0.
- **Buried blocks.** The two blocks that actually set the row are #5189 (2026-10-03, 1→0) and #5123 (2026-09-29, 2→1). They are inserted at lines 38 and 45, inside the 09-16 narrative, so a reader taking `head` sees the wrong newest block.
- **No live capture behind the row.** Both edits are self-described "CONTRACT-DERIVED regen, not live-captured", so `light_count_directional` came from no capture.
- **Stale entity count.** Today's live run confirms the directional row at 0. It also shows the one fewer spawned light entity (`entities_total` 929→928) that the one-row edit left behind. The −1 is consistent with #5189's zero-spawn, but that is inferred, not bisected.

## Evidence
`grep -n '^# regenerated' .claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` lists 2026-09-30 (l.1), 2026-09-16 (l.25), **2026-10-03 (l.38)**, **2026-09-29 (l.45)**. The 2026-10-03 capture's `light.dump` shows 8 `kind=Point`, 0 `kind=Directional`, and `bench: entities=928`.

## Impact
None on the gate today (−0.1 % is in the ±2 % band; directional is exact). The risk is that the next reader attributes the directional row from the wrong block, and the file sets a precedent of editing gate rows without a capture.

## Related
#5189, #5123 (closed); #5125 (09-30 regen)

## Suggested Fix
Live-regen the Oblivion TSV (`/audit-runtime --game oblivion --regen`). Put a single newest block at line 1 that cites #5123/#5189 for the directional row and #5189 for the −1 entity. Correct the 09-30 block's "stays 1" sentence, or move it below.

## Completeness Checks
- [ ] **SIBLING**: The other baseline TSVs (and their newest `# regenerated:` block) checked for the same pattern
- [ ] **TESTS**: `cargo test -p byroredux --bin byroredux bench::` (runtime baseline schema tests) stays green after the edit
