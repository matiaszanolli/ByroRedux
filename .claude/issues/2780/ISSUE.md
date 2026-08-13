# REN-D14-NEW-05: caustic.rs clear_for_skip leaves parked_frames stale on skip streak

## Description
`clear_for_skip` zeroes all three layers but leaves `parked_frames[frame]` untouched (`advance_parked_visits` runs only inside `dispatch`), so a skip streak under a bit-identical view-proj resumes at a near-cap decay against an empty pool — at the `CAUSTIC_DECAY_MAX = 0.995` ceiling, a 0.005 new-sample weight, fading back in over ~200 slot-visits. Narrowly reachable (any camera motion resets it); one-line fix.

## Location
`crates/renderer/src/vulkan/caustic.rs` (`clear_for_skip`), `crates/renderer/src/vulkan/context/post_passes.rs`

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2780

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D14-NEW-05).
