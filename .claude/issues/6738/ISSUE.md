# REN-D5-02: two residency changes for one handle can land in one flush batch; the `Residency::apply` refusal is ignored

Labels: medium,renderer,memory,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6738

**Severity**: MEDIUM (GPU consequence unconfirmed; accounting skew is certain on the described path) · **Dimension**: Memory/Lifecycle

## Location
`flush_upload_batch` install loop (`residency.apply(change);` result dropped, then `entry.texture.replace(texture)`), `queue_residency_change`, `Residency::apply` (`crates/renderer/src/texture_registry/`)

## Description
`queue_residency_change` plans against the entry's current residency, not against changes already in `pending_promotions`. `apply_wanted` avoids this for its own candidates (its `queued` map), but a promotion whose host read was already in flight (`TextureStreamer::in_flight`) lands through `queue_promotion_bytes` regardless of a demotion queued for that handle meanwhile. A budget drop queues many demotions (`spend_budget` sweep 2 demotes any texture above its floor that is not `locked`; `locked` only means "change already queued"), so the overlap is reachable.

## Evidence
No dedupe in `queue_residency_change`; the stale guard (`current.first_resident == change.old_first`) is in the record loop only, so a second change in the same batch passes it; `residency.apply(change)` is a statement and `landed_changes.push` runs unconditionally. Across batches the stale check works. Test `a_stale_change_is_refused` pins `apply` itself, not the install loop.

## Impact
Wrong budget / telemetry accounting at minimum. A later change is recorded with `old_levels` computed from a residency that no longer matches the image, so a barrier / copy subresource can be out of range (a spec violation `BYRO_VALIDATION=1` would flag). The GPU consequence is unconfirmed.

## Suggested Fix
In the record loop keep a small set of handles already staged this batch and treat a second change for the same handle as stale; in the install loop, if `apply` refuses, retire the new texture through `pending_destroy` instead of installing it. Optionally refuse a handle with a queued change in `queue_residency_change`.

**Needs RenderDoc / validation**: `BYRO_VALIDATION=1` with streaming on and a shrinking `render.texture_budget_mib` while promotions are in flight. Related: #6626.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **DROP**: The retired texture on the refused-apply path is destroyed through the deferred queue
- [ ] **TESTS**: A regression test pins this specific fix (two changes for one handle in one batch)
