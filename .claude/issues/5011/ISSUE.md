# PAR-D4-2026-09-29-01: Two sweeps are weaker than their issues claim: the Oblivion EGM test skips green under REQUIRE and pins nothing, and the BGSM sweeps never check the #4664 drift signals

**Labels**: low,bug,import-pipeline,test-gap,game:oblivion

**Source report**: `docs/audits/AUDIT_PARSERS_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/facegen/tests/parse_real_facegen.rs` (`parse_vanilla_headhuman_egm_oblivion`); `crates/bgsm/tests/parse_all.rs:242`, `crates/bgsm/tests/parse_all.rs:351`
- **Status**: NEW
- **Trigger Input**: n/a (gate weakness).
- **Description**:
  - **Oblivion EGM test** (added by #4665):
    - It resolves `BYROREDUX_OBLIVION_DATA` by hand. A missing directory, or a set-but-wrong override, gives `eprintln!("skipping…"); return;` with no `require_game_data`, so it is green in the strict lane.
    - It asserts only `num_vertices > 0` and `morphs > 0`.
    - Its output still says "record these in the EXPECT table". This run measured 1,736 verts and 80 morphs.
    - It carries none of the #4653 int16 checks that the FNV/FO3 test has: peak |raw| near 32767 and zero non-finite values.
  - **BGSM sweeps:**
    - Both call `parse()` and count `Ok` only.
    - Vanilla is measured zero-noise (0/36,888 files with leftover bytes; only v2 and v22 in use), and #4664's suggested fix asked for both checks in the FO4/FO76 sweep.
    - A gating regression that reads the wrong fields but consumes every byte, or leaves bytes over without erroring, stays green.
  - **Other (format × game) gaps:**
    - FO3 BSA is open-and-list only, with no extract sweep.
    - Skyrim LE BSA is covered only through hkx.
    - FO76 DX10 is exercised by one texture.
    - The FO4 DLC CSGs have no test.
- **Evidence**: the test body; the strict-lane log line `[Oblivion] headhuman.egm: 1736 verts, 80 morphs (record these in the EXPECT table)`.
- **Impact**: an Oblivion EGM decode regression, or a BGSM version-gating regression, can pass the nightly `parsers` lane.
- **Related**: #4660, #4665, #4664, #4653
- **Suggested Fix**:
  - Route the Oblivion test through a `Game` variant with `require_game_data`, and pin 1,736 verts / 80 morphs plus the int16 range and non-finite checks.
  - Assert `unconsumed_bytes == 0 && version_over_ceiling.is_none()` in both BGSM sweeps, via `parse_bgsm_diag` and `parse_bgem_diag`.

**Validated at HEAD 9fcfdc3fc**: `parse_vanilla_headhuman_egm_oblivion` in `crates/facegen/tests/parse_real_facegen.rs` still does `eprintln!("skipping…"); return;` without `require_game_data`, asserts only `> 0` counts and prints the "record these in the EXPECT table" line; `crates/bgsm/tests/parse_all.rs` has no `_diag` / unconsumed / version-ceiling assertion.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
