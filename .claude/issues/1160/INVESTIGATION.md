# #1160 Investigation

## Verification

Bug was live at HEAD `93600cd7`. Two DST-side `BOTTOM_OF_PIPE` sites found in the renderer crate sweep:

1. **`composite.rs:448`** — outgoing subpass dependency (composite subpass → SUBPASS_EXTERNAL); cited by the issue
2. **`screenshot.rs:169`** — pipeline barrier for present-layout transition after screenshot readback; sibling not called out explicitly in the issue but caught by the SIBLING sweep

Both paired `BOTTOM_OF_PIPE` with an empty `dst_access_mask` — the spec-mandated pairing, which is why the migration is purely mechanical (no access-mask changes).

## Fix

Migrated both DST-side sites to `vk::PipelineStageFlags::NONE`, the Vulkan 1.3 canonical form for "no further synchronization required". Mirrors the SRC-side sweep that closed under #949 / #1100 / #1121 / #1122.

## Helpers.rs comment update

`helpers.rs:181-192` previously read:

> composite.rs:408 and screenshot.rs:164 also use BOTTOM_OF_PIPE in `dst_stage_mask` but pair it with an empty `dst_access_mask`, which the spec permits — so they're left alone.

Updated to record the migration:

> The two sibling sites that previously paired `BOTTOM_OF_PIPE` with an empty `dst_access_mask` (composite.rs outgoing dep + screenshot.rs present-layout transition) migrated to `vk::PipelineStageFlags::NONE` under #1160 / REN-D10-NEW-13.

The helpers.rs site itself (`#573 / SY-2`) is unchanged — that pre-existing decision OMITS `BOTTOM_OF_PIPE` because it combined the flag with a non-empty `SHADER_READ` access mask, which Synchronization2 validation rejects. Different class of fix — left alone.

## SIBLING sweep (complete)

Final grep for `BOTTOM_OF_PIPE` in `crates/renderer/src/`:
- composite.rs:448 → now in a **comment** describing the migration
- helpers.rs:181/182/190 → all in **comments** explaining the policy

No live code path uses `BOTTOM_OF_PIPE` in the renderer crate.

## Verification

- `cargo check -p byroredux-renderer`: clean
- `cargo test -p byroredux-renderer --lib`: 278/278 pass

## TESTS gap (acknowledged)

Issue notes "Validation-layer integration test (or RenderDoc capture) on the resize / first-frame path would surface any IHV-specific behaviour change. Low risk given the spec compatibility guarantee." Same RenderDoc gap as TD9-200/201, #1231, #1159. No new test added; the migration is mechanical and the spec compatibility clause makes runtime behaviour equivalent.

## Pattern observation

Yet another doc/comment-update wave riding alongside the actual fix — `helpers.rs:181-192` would have become a stale "left alone" reference the moment this fix landed without the comment update. Same hygiene discipline as the M28.5 #1230 cleanup earlier today.
