# #871: SkinSlot output_buffer leaked when allocate_descriptor_sets fails after buffer alloc succeeds

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/871
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Source**: `docs/audits/AUDIT_CONCURRENCY_2026-05-06.md` (dim 3, finding L2)
- **Status**: NEW — verified present at the cited lines on 2026-05-06.

## Location

`crates/renderer/src/vulkan/skin_compute.rs:316-340`

## Trigger Conditions

NPC enters view AND `descriptor_pool` is exhausted AND
`GpuBuffer::create_device_local_uninit` succeeds before
`allocate_descriptor_sets` fails. Pool exhaustion requires
> 32 unique skinned entities visible at once.

## Why this exists

`GpuBuffer::Drop` is warn-only by design (the C3-10 leak-on-drop pattern
from `AUDIT_CONCURRENCY_2026-04-12.md`); the `VulkanContext::Drop` chain
is the safety net for *registered* resources. `output_buffer` here is
not yet registered — the `SkinSlot` construction at line 342-352 is
what would have made it owned by `self.skin_slots`.

## Fix shape

Match the descriptor-set allocation explicitly and destroy
`output_buffer` on the err arm. Local precedent: `caustic.rs:195` and
`caustic.rs:351`.

## Sibling sites to audit

- `caustic.rs::create_slot`-style helpers — already use the
  `partial.destroy` rollback pattern.
- `svgf.rs` / `taa.rs` / `gbuffer.rs` `recreate_on_resize` partial-
  allocation leaks — carried-over C3-01/C3-02/C3-03 from 2026-04-12.

## Test plan

Integration test under the renderer test harness: pre-allocate
`max_slots` slots, then attempt one more, assert memory accounting
unchanged after the failure path.

## Suggested next step

`/fix-issue 871`
