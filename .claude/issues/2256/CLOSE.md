Fixed in db487cc5.

## The file had nearly doubled since filing

Measured before touching anything: **4074 total / 3033 production LOC**, not the 2075 in the issue. `new_inner` had grown from ~556 to ~780. So the split was more overdue than filed, and the numbers below are against the current file, not the Session-62 ones.

## Took the second option, not the first

You offered two: extract helpers *within* `new_inner`, or move the construction path to a `volumetrics/init.rs` sibling. The second, for the reason the skill's own `draw.rs` note records — #3739 and #3738 both extracted helpers within a file and neither moved its host across the threshold, because a function-level split and a file-level split are independent signals. Extracting from `new_inner` would have left `volumetrics.rs` at 3033.

`volumetrics/init.rs` now owns `new` / `new_inner` / `create_volume` / `initialize_layouts`. The parent keeps the per-frame recording path (`dispatch`, `record_neutral_frame`, the `write_*` descriptor updates), every pure helper, and every GPU-struct definition the shader contract is pinned against.

    volumetrics.rs        4074 total / 3033 production  ->  2936 / 1895
    volumetrics/init.rs                                     1160 / 1160

Both under threshold. The directory already existed (`mod noise;`), so this added a sibling rather than a new module shape.

## The move is verifiably verbatim

`sed`-extracted the exact line range rather than retyping, then diffed the extraction against the new file's body and the retained region against the original — byte-identical both ways, so no line changed meaning in transit. On top of that, exactly two deltas, both enumerable:

- Six `super::{descriptors,pipeline,texture}::` paths, which resolved to `vulkan::<mod>` one level up. Imported the three modules by name instead of re-spelling them `super::super::` — that keeps every moved line at or below its original length, so `rustfmt` wanted nothing and the diff stays readable. (The `super::super::` form re-wrapped a whole closure body for pure churn.)
- One stray blank line left at the cut point.

## The part that needed care: seven pins scan this file

This is the hazard the split actually carried, and it is the same class as #4034/#4035 from this week — a source-scan pin left pointing at a file the code moved out of. Audited all seven first:

- **One broke loudly** and was repointed: `the_descriptor_table_does_not_credit_private_layout_passes_with_global_sets` asserts `set_layouts(std::slice::from_ref(`, and both sites moved. It now scans the parent and `init.rs` concatenated, so it no longer cares which side of the seam builds the pipeline layout.
- **Two would have gone silently vacuous** — negative scans that fail open, not closed. Extended to `init.rs`: `temporal_history_indexing_uses_the_general_previous_slot_form` (#2771/#3442) and `swept_sources_carry_no_bare_line_number_anchors` (#2922/#4009).
- **Four stay correctly scoped to the parent**, verified needle by needle: `caustic.rs`'s neutral-clear/TRANSFER_WRITE pair (in `dispatch` and `record_neutral_frame`), the GLSL/Rust struct mirrors, the `froxel_grid_cost` field list, and the `CausticPipeline::write_tlas` mirror.

One deliberate non-change: `compute_dispatches_derive_their_grid_from_the_generated_workgroup_size` still scans the parent alone. Adding `init.rs` would break its *positive* half, since construction has no dispatch grid — the pin is about dispatch, and dispatch stayed.

Also updated `/audit-tech-debt`'s Dim 1 bucket, which listed this file at ~2940. Left stale, the next sweep would have re-filed this issue; it now carries the same do-not-re-propose note the `draw.rs` split has.

## On a regression test

No LOC-threshold assertion added. A "must stay under 2000" test fails on unrelated growth and invites gaming the measurement rather than the structure. The durable guard is the three repointed pins above, which is what actually breaks if the seam is undone.

**DROP**: `destroy` was not moved and not edited; teardown order is unchanged. **UNSAFE**: no new `unsafe`; `initialize_layouts` moved with its SAFETY comments intact.
