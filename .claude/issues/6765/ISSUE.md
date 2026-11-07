# CHAR-2026-11-07-L-06: CHARAL prose still describes the pre-#6430 / #6350 / #6422 / #6403 states

**Issue**: #6765
**Labels**: documentation, low, character, doc-rot
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-06, Dim 3 + Dim 5). Game: all (FO3/FNV in practice).

## Description
CHARAL prose still describes the pre-#6430 / #6350 / #6422 / #6403 states. `regen.rs` contradicts itself (the struct rustdoc says FO3/FNV install the config, the tick docstring says Oblivion only), and `charal.md` presents the pre-#6430 `0`-sentinel design as current, which is exactly the defect #6430 retired.

## Location
- `docs/engine/charal.md:247` (`PoolRegenConfig { fatigue_avif: u32, magicka_avif: u32, willpower_avif: u32, .. }`) and `:274-276` ("their ids are `0`; no actor carries AV 0").
- `crates/core/src/character/regen.rs:184-188` ("arriving only with Oblivion's wiring ... four games have carried one ... with no config following") and its mirror `docs/engine/charal-oblivion-ruleset.md:421-425`.
- `docs/engine/charal-fnv-fo3-ruleset.md:550` ("`iVATSConcentratedFireBonus` 5 ... not yet plumbed, #6422").
- `docs/engine/charal.md:682`, `docs/engine/charal-fnv-fo3-ruleset.md:563` and `crates/core/src/character/vats.rs:630` cite #6403 as the open to-hit tracker.
- Outside the CHARAL paths: `docs/prd/vats.md:147,151,6` and `docs/prd/perk-entry-points.md:182` (R9).

## Evidence
`PoolRegenConfig` ids are `Option<u32>` and `build_vats_model` sets them `None` (`byroredux/src/npc_spawn/stamps.rs:288-293`, #6430); the config is installed on FO3/FNV cell load (#6374; `regen.rs:126-132` itself says so). `vats_queue_part` plumbs Concentrated Fire (`5502c2f97`). #6403 and #6422 are CLOSED; the `#[ignore]` placeholder test now names #6350.

## Impact
A reader re-introduces the `0` sentinel, or concludes Concentrated Fire is unplumbed; the "AP only, not inert/live wholesale" rule is violated in comments.

## Related
#6622 (swept comments but did not reach these), #6430, #6422, #6403, #6350, #6644, #6645.

## Suggested Fix
`Option<u32>`/`None` (#6430); config installed for FO3/FNV (AP lane); Concentrated Fire plumbed (#6422); re-point #6403 to #6350/#6644/#6645; update the PRD rows.

## Completeness Checks
- [ ] **SIBLING**: grep the other capture docs, `charal.md`, `feature-matrix.md`, `ROADMAP.md` and code comments for the same stale claim
- [ ] Cite symbols, not line ranges, in the corrected prose
