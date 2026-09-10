# Bundle: #3480 #3481 #3572 #3657

Fetched 2026-09-09 from `matiaszanolli/ByroRedux`. Three fixed, one deliberately
left OPEN.

| # | Title (abridged) | Severity | Crate | Outcome |
|---|---|---|---|---|
| 3480 | Skyrim pool bases read race off the `Use Stats` chain, but race is a `Use Traits` field | MEDIUM | `byroredux-plugin` | **fixed** |
| 3481 | FO4 `calculated_health == 0` "absent" sentinel not honoured across template resolution | MEDIUM | `byroredux-plugin` | **fixed** |
| 3572 | TAA resolves only the pre-composite direct HDR | MEDIUM | `byroredux-renderer` | **not fixed — blocked** |
| 3657 | `skin_slots` teardown nested under `skin_compute.is_some()` | MEDIUM | `byroredux-renderer` | **fixed** |

## #3480 — two `TPLT` chains, not one

`derive_npc_actor_values` resolved `resolve_inherited_stats` once and handed the
single record to all three arms; the Skyrim arm then read `race_form_id` off it.
Race rides the independently-set `Use Traits` bit (`0x0001`), walked by
`resolve_inherited_traits` — which `stamp_character_components`
(`byroredux/src/npc_spawn.rs:182`) already uses for the `Background` it writes on
the *same entity*. Verified at HEAD: both call sites read as described.

Fix: resolve both chains in `derive_npc_actor_values` and hand each arm the
record whose fields it reads — `stats` for the signed `ACBS` offsets (which
really are stats), `traits` for `race_form_id`. `Background.race_form_id` and the
race behind the pools now agree by construction.

The pre-existing #3381 test `skyrim_pools_follow_use_stats_template` asserted
race followed the *stats* chain, so it was amended to set both bits — its intent
(template precedence) is preserved, and the divergent case it can no longer cover
is pinned by a new test rather than dropped.

## #3481 — `0` means absent, so fall back

`derive_stored_actor_values` pushed the baked `DNAM` pair only `if baked > 0`,
reading it off the resolved record. `0` is the documented absent sentinel
(`crates/plugin/src/esm/records/actor/mod.rs:464`), so a shell that authored its
own Health under a template that authored none yielded nothing.

Fix: `baked_or_shell` — the resolved record wins when it authors the field,
otherwise the shell's own value stands, mirroring how `resolve_inherited_record`
already falls back when the flag or the template is missing. Applied to both
`calculated_health` and `calculated_action_points`.

`PRPS` got the same treatment: an empty property array is the identical "absent"
statement. Measured impact on vanilla FO4 is **0 records** (the issue's own table
confirms no shell loses its `PRPS`), so this is a consistency change, not a
behaviour change — but it means the arm no longer applies two different
absent-contracts to two adjacent fields.

## #3657 — un-nest the drain, keep the free gated

`SkinSlot::destroy(device, allocator)` now owns the allocation half; the drain in
`teardown.rs` is unconditional and only the `free_descriptor_sets` call stays
inside a `skin_compute` guard, *inside* the loop. `destroy_slot` is now the
composition of the two rather than a second copy of the buffer teardown.

Not reachable at HEAD — the issue says so, and it re-checks out: `skin_compute`
is assigned once and never reassigned, and every `skin_slots.insert` sits inside
a `skin_compute` guard. This closes a defence-in-depth gap, pinned by a
source-shape test in the style of #3374's.

**DROP check:** the pin also asserts the drain still precedes
`SkinComputePipeline::destroy`, since that local ordering is load-bearing
(`VUID-vkFreeDescriptorSets-descriptorPool-parameter`).

**SIBLING sweep:** `teardown.rs` has exactly three drains — `image_health_buffers`
(`:53`) and `morph_slots` (`:74`) were already unconditional; `skin_slots` was the
last Option-nested one. No other drain carries the coupling.

## #3572 — investigated, NOT fixed, left OPEN

Premise re-verified at HEAD (audit-finding-hygiene discipline: check before
proposing):

- `post_passes.rs:270` records TAA strictly before `record_composite_pass`.
- `taa.rs:560` still binds `curr_hdr` from `hdr_views[f]` — the raw main-pass HDR
  attachment.
- `post_passes.rs:1091-1095` still hands FSR `composite.scene_image(frame)`, the
  fully composited post-bloom scene.
- `composite.frag:771-772` still classifies with a hard binary
  `depthIsSurface(depth)` / `is_sky` against the jittered depth buffer.

Every clause of the finding holds. It is not fixed here because both proposed
remedies are exactly the class of change this project has a standing rule
against: the primary one is a render-pass/barrier restructure of the frame tail
(`scene_images` is `COLOR_ATTACHMENT | SAMPLED | TRANSFER_SRC | STORAGE` and
already changes layout twice there), and the narrower one adds a second temporal
history. Neither failure mode is observable to `cargo test`, and the issue's own
publish-time policy note says the same: *"do not ship the barrier reshuffle on
test evidence alone."*

**Unblock:** a RenderDoc capture of the frame tail, or a `BYRO_VALIDATION=1`
sync-validation run, on `--upscaler taa` in an exterior cell. That needs a live
device and a human at the capture.
