# #7358: FO3-D4-001: the skill-quoted DDS-header sweep and folder-hash census have no pinned test

Labels: see GitHub (severity, domain, type and game labels)

Filed from `docs/audits/AUDIT_FO3_2026-11-23.md` (finding `FO3-D4-001`).

- **Severity**: LOW
- **Dimension**: BSA v104 and real-data validation
- **Location**: `crates/bsa/tests/bsa_real.rs`, `crates/bsa/src/archive/open.rs`
- **Status**: NEW
- **Description**: The skill tells the auditor to sweep `Fallout - Textures.bsa` to valid DDS headers (12 261 / 12 261) and to run the folder-hash census (0 collisions over 1 810 folders). Neither number nor any such sweep exists in `crates/bsa`. `fo3_fnv_and_skyrimse_bsa_corpus_census` pins only the archive count (16), the entry count (159 155) and that every entry extracts.
- **Evidence**: no FO3 textures DDS or folder-hash pin exists (`rg` over the crates finds none). The only DDS-header census is `crates/menuxml/tests/fo3_corpus.rs` (1 157 interface DDS). `crates/bsa/src/archive/open.rs` computes the folder hash under `#[cfg(debug_assertions)]` only and merely warns on a mismatch, so a release lane never exercises it.
- **Impact**: extraction can succeed while payloads or folder hashes are wrong, and nothing guards it. Low because extract-with-inflate plus the NIF and menuxml lanes cover most payloads.
- **Related**: #5903, #5669.
- **Suggested Fix**: add an `--ignored` `fo3_textures_bsa_dds_header_census` and a hash check that runs in release to `crates/bsa/tests/bsa_real.rs`, or mark both numbers in the skill as manual measurements.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
