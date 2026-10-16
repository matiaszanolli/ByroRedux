# #5902: PAR-D4-2026-10-15-02: the FO3 and FNV menu-XML corpora are never parsed by a gate, although the include budgets are calibrated on them

- **Labels**: low,bug,ui,test-gap,game:fo3,game:fnv
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**:
  - `crates/menuxml/tests/fo3_corpus.rs:74-90` and `crates/menuxml/tests/fnv_corpus.rs:83-101` list `*.xml` with only a `>= 90` / `>= 110` floor. FO3 parses one document, `hud_main_menu.xml`, through `fo3_hud_renders`. FNV parses none.
  - Budget calibrations that cite these corpora: `crates/menuxml/src/parse.rs:595-607` ("heaviest vanilla document splices 38,378 bytes", "largest vanilla fragment (12,582 B, FNV)") and `:632` ("largest vanilla document has 205 tiles").
- **Status**: NEW. The Oblivion corpus got exact parse pins in ba252c482 (#5329). The baseline gate matrix already noted "tile totals printed, not pinned", and that is now true only for FO3/FNV.
- **Trigger Input**: n/a (gate gap). Regression class: a scanner or include change that drops, or newly budget-truncates, FO3/FNV menu content. Truncation is a `log::warn!`, never an error, so only a pinned total would show it.
- **Description**:
  - The `MAX_INCLUDE_*` / `MAX_DOCUMENT_TILES` constants are justified by a 2026-10-05 three-corpus census. Two of those three corpora have no committed check.
  - A future change could push a vanilla FNV document into a budget, and the strict lane would stay green.
- **Evidence**: probe `menuparse`, real `parse_document` with an archive-backed source.

  | Corpus | XML | Tiles un-spliced | Tiles spliced | Largest document |
  |---|---|---|---|---|
  | FO3 | 99 | 736 | 1,445 | `inventory_menu.xml`, 150 tiles |
  | FNV | 121 | 1,060 | 2,206 | `inventory_menu.xml`, 195 tiles |

  - 0 budget, cycle or depth warnings on either corpus.
  - The XML-count floors are loose: FO3 is `>= 90` against 99 measured, FNV `>= 110` against 121.
- **Impact**: lost regression coverage for the FO3/FNV HUD driver's whole document set. There is no current failure.
- **Related**: #5329, #5314, #4665, PAR-D4-2026-10-12-01.
- **Suggested Fix**: mirror `vanilla_corpus_parses` in both files: parse every XML un-spliced and spliced, and pin the totals above plus the exact XML counts.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
