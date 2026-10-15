# OBL-D1-2026-10-14-03: nothing checks that a sizeless parse lands on the footer

**Labels**: low,game:oblivion,legacy-compat,nif-parser,test-gap,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D1-2026-10-14-03`. Re-verified against current code before filing.

- **Severity**: LOW (test gap)
- **Dimension**: 1
- **Location**: `crates/nif/src/lib.rs` `dispatch_blocks` / `finalize_scene` (no footer read; `root_index` is the first NiNode-family block); `crates/nif/tests/` (no EOF-exactness gate)
- **Status**: NEW
- **Description**: with no `block_sizes` table (every Oblivion file) a wrong *last* block is invisible: a successor would fail, but the last has none, and `parse_nif` never reads the footer or compares the cursor with the file length. The corpus gates (truncation, per-block histogram, drift detector over fixed-size types) cannot see an under-read of a file's final block, such as a missing trailing field on `NiTriStripsData` or `NiSkinPartition`.
- **Evidence**: throwaway scan outside the repo over all 9 612 Oblivion NIFs (6 pre-5.0.0.1 inline-name files skipped): 9 598 of 9 606 end exactly at `4 + 4*num_roots` bytes before EOF. The other 8 are Tamriel terrain-LOD files (`meshes\landscape\lod\60.*`) whose footer is correct right after the last block and then followed by 1.4–235 KB of stale bytes (a longer file overwritten by a shorter one, not a parser under-read). `root_index` is block 0 in all 9 612.
- **Impact**: none measured; a blind spot, not a defect. A future last-block under-read would be silent.
- **Related**: #3926, #3711.
- **Suggested Fix**: add an `--ignored` corpus assertion that, for sizeless files, the cursor after the last block satisfies `remaining == 4 + 4*num_roots`, with the 8 known stale-tail LOD files as an explicit allowlist.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
