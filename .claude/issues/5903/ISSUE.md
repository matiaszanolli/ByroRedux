# #5903: PAR-D4-2026-10-15-03: the (format × game) sweep matrix still has unswept archive generations

- **Labels**: low,bug,import-pipeline,test-gap
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: the (format × game) sweep matrix still has unswept archive generations; #5073's FO3 placeholder sat outside every committed gate

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**:
  - `crates/bsa/tests/ba2_real.rs:743-775` (`fo3_bsa_corpus_opens_and_lists`, which never extracts).
  - `ba2_real.rs:687-740` (FO76: list everything plus one DX10 extract).
  - `bsa_real.rs:358-416` (SE, `Meshes0` only).
  - `bsa_real.rs:419-462` / `ba2_real.rs:776-815` (the `declared_size` tests, which fully extract FNV Meshes, SE Meshes0, FO4 Meshes and Textures1 only).
  - `crates/facegen/tests/parse_real_facegen.rs` (one file per game).
- **Status**: NEW as a matrix. #5011 closed with "the other (format × game) gaps the issue listed are not addressed", and #4665 added list-only FO76/FO3 coverage.
- **Trigger Input**: n/a.
- **Description**: committed coverage per (format × game):

  | Format | Oblivion | FO3 | FNV | Skyrim LE | Skyrim SE | FO4 | FO76 | Starfield |
  |---|---|---|---|---|---|---|---|---|
  | BSA / BA2 full extract | ✓ 17/17 archives | ✗ list only | Meshes only (via `declared_size`) | ✗ none | Meshes0 only | Meshes + Textures1 (DX10) | ✗ list + 1 extract | ✓ full sweep |
  | BGSM / BGEM | — | — | — | — | — | ✓ rate 1.0 (count unpinned, D4-43) | ✓ rate 1.0 (count unpinned) | — |
  | CDB | — | — | — | — | — | — | — | ✓ exact 97 / 1,438,780; index 500,403; DLC 500,385 |
  | HKX clips | — | — | — | 16-clip LE/SE parity | ✓ exact census | ✗ 1 clip (D4-40) | — | — |
  | HKX graphs | — | — | — | — | 2 graphs | 3 graphs | — | — |
  | FaceGen | 1 EGM | 1 each EGM/EGT/TRI | 1 each | — | — | — | — | — |
  | MenuXml XML | ✓ exact | ✗ (D4-41) | ✗ (D4-41) | — | — | — | — | — |
  | Menu DDS | ✓ exact | ✓ exact | ✓ exact | — | — | — | — | — |
  | CSG | — | — | — | — | — | ✓ exact | — | — |

  The FO3 and FNV empty-placeholder entries behind #5073 were found only by the orchestrator's uncommitted 2026-10-12 sweep. The committed Oblivion sweep caught Oblivion's 35, while FO3's archives are never extracted by any test.
- **Evidence**: probe `fgsweep` shows the FaceGen sweep is cheap and green today, though uncommitted.

  | Game | EGM | EGT | TRI |
  |---|---|---|---|
  | Oblivion | 154 | 20 | 86 |
  | FO3 | 250 | 36 | 49 |
  | FNV | 275 | 36 | 49 |

  0 parse failures in all of them (679 EGM in total), under seconds of runtime.
- **Impact**: reader regressions in the v104 zlib path (FO3/FNV non-mesh and LE), in v105 outside Meshes0, in FO4 DX10 beyond Textures1, in FO76 extract, and on any EGM other than `headhuman` would land green.
- **Related**: #5011, #4665, #5073, #5660.
- **Suggested Fix**:
  - Generalise the Oblivion brute-force test into a per-title loop over every vanilla archive (FO3, FNV, SE, FO4, FO76; LE behind its variable), with exact file counts.
  - Add a FaceGen sweep pinned at the counts above.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
