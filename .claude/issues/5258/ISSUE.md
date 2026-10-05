# #5258: NIF-D2-2026-10-05-01: the #5119 version-literal guard's production cut silently skips ~1,000 lines of production parser code

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5258
- **Labels**: low,nif-parser,nif,bug,test-gap
- **Source**: `docs/audits/AUDIT_NIF_2026-10-05.md` (NIF-D2-2026-10-05-01)

_From `docs/audits/AUDIT_NIF_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW (test-gap; the code is correct today)
- **Dimension**: Version Gating
- **Game Affected**: all, but mostly the pre-Skyrim controller and texture-effect paths (Oblivion v10.x–20.0, FO3/FNV)
- **Location**: `crates/nif/src/version_literal_tests.rs:37-40` (`production_text`), `:285-297` (`violations_in`)
- **Status**: NEW. #5119 (closed) introduced the guard; no issue covers the cut.
- **Description**: `production_text` keeps everything before the **first** `"\n#[cfg(test)]\nmod "`. That is the right cut for a positive scan, where a needle after the cut fails loudly, as the core `source_scan` doc says. For this *negative* scan (ban a needle), any production code after an early test-module declaration is never scanned, and nothing reports that.
- **Evidence**: a brace-depth walk over `crates/nif/src` finds production items after the first cut in three parser-relevant files:
  - `blocks/controller/mod.rs`: `#[cfg(test)] mod sequence_pre_10_1_0_106_tests;` sits at lines 20-21, so **lines 22-902 are unscanned**. That range is `NiTimeControllerBase` and every controller parser in the file, with 8 live version gates at lines 90, 260, 371, 413, 482, 608, 762 and 771.
  - `blocks/texture.rs`: an inline `mod tests {` at line 433 precedes the `NiTextureEffect` parser (~1150-1227), whose gates sit at 1172, 1191 and 1205, plus the `NiDynamicEffect` `bsver < FALLOUT4` gate.
  - `import/walk/mod.rs`: `mod tests;` at 1028-1029 precedes `resolve_affected_node_names` and `resolve_block_ref_names`. There are no gates there today.
- **Impact**: a bare `stream.bsver() >= 130` or `NifVersion(0x…)` reintroduced in any controller parser, or in `NiTextureEffect`, passes the guard. This is the exact #1042 regression the guard exists to catch, and it sits on the Oblivion no-`block_sizes` path where a wrong gate cascades.
- **Related**: #5119, #1042, #4604, #5164 (the vacuous-source-scan class).
- **Suggested Fix**: blank `#[cfg(test)] mod …` items by brace matching, since `strip_comments_and_strings` already tracks state, instead of truncating at the first one. Alternatively, move the two early test-module declarations to end-of-file and assert that no column-0 production item follows the cut.

**Publish note**: related to open #5100 (the `production_text` cut is duplicated across ~9 source-scan sites). #5100 covers the duplication. This issue covers the negative-scan truncation semantics, so fold the two together if a shared helper lands.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
