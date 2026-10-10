# #5516: EXT-D1-2026-10-09-01: #5374 implemented Oblivion inherit-all twice. The parse-time PNAM stamp is the one that runs; the flag-less `inherit_all_up_chain` copy is dead on parsed data, and only the dead copy is tested

**Labels**: bug, esm-plugin, game:oblivion, low, tech-debt, terrain-exterior, test-gap

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-09.md` — finding `EXT-D1-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (tech-debt / test-gap; behaviour is correct)
- **Dimension**: EXAL boundary discipline (and WATAL default water)
- **Location**:
  - Parse stamp: `crates/plugin/src/esm/cell/wrld.rs:215-235`. It ORs `LAND|LOD|WATER|CLIMATE` into `parent_flags` for Oblivion children, using local copies of the bit constants.
  - Second walk: `byroredux/src/env_translate.rs:229-231` (Oblivion water arm) and `:398-436` (`inherit_all_up_chain`).
  - Redundant climate step: `:556-561` (the first step of `oblivion_climate_rungs`).
  - Tests: `:2757` (`oblivion_child_inherits_parents_water_without_pnam`) and `:5571` (`oblivion_child_inherits_the_parent_authored_climate`). Both fixtures leave `parent_flags` at 0.
  - The parse helper hard-codes the game: `crates/plugin/src/esm/cell/tests/wrld.rs:84` (`GameKind::Skyrim`).
- **Status**: NEW (introduced by `67afa4b95`, `217bcbc9f`)
- **Tier Violated**: single-boundary (two mechanisms for one inheritance rule)
- **Game Affected**: Oblivion
- **Description**:
  - Because `parse_wrld_group` stamps the inherit bits, the bit-gated `inherit_up_chain` already resolves an Oblivion child's `NAM2`, and `resolve_worldspace_climate` (rung 1) already resolves its `CNAM`.
  - The `or_else(inherit_all_up_chain)` and the chain step in `oblivion_climate_rungs` can therefore fire only on hand-built records with zero flags, which means test fixtures only.
  - `inherit_all_up_chain` re-copies the cycle guard, the linear `form_id` reverse lookup and the precedence rule. #2814 consolidated exactly that into `inherit_up_chain` ("a future fix to the walk would have landed in one copy and silently missed this one"). Its termination cases also lose `inherit_up_chain`'s `warn!` diagnostics.
  - The stamp's own comment says it exists "without every consumer carrying a second, game-aware walk", yet the same fix added that walk.
  - No test drives the stamp. If it were deleted, every test would still pass through the dead fallbacks.
- **Impact**: None today. It is a maintenance trap: a walk fix can land in one copy, and the production path is untested.
- **Related**: #5374, #5388, #2814, #2735.
- **Suggested Fix**:
  - Keep the parse stamp, which is the boundary-shaped one. Delete `inherit_all_up_chain` and the duplicated chain step.
  - Add an Oblivion-variant `parse_synthetic_wrld` test: an Oblivion child with no PNAM gets `parent_flags == 0x1B`, and a root stays 0.
  - Point the env_translate fixtures at stamped flags.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
