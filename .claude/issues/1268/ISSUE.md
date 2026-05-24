# TD3-NEW-01: 3 inline ImageSubresourceRange literals missed by #1149 sweep (builder-form variant)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/1268

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-24_DIM3.md` — Dimension 3 (Logic Duplication)

## Severity
**LOW** — readability / consolidation. No correctness or runtime impact.

## Status
**NEW** at HEAD `4b70227b`

## Description
The #1117 (TD3-206) + #1149 (TD3-207) sweeps migrated 22 inline `ImageSubresourceRange` literals to the `color_subresource_single_mip()` / `color_subresource_mips()` helpers added by #1046. Three sites slipped through because they use the **builder form** (`vk::ImageSubresourceRange::default().aspect_mask(...).level_count(1).layer_count(1)`) rather than the **struct-literal form** (`vk::ImageSubresourceRange { aspect_mask: ..., level_count: 1, ... }`) the original sweep grepped for.

The two forms are functionally identical but textually distinct, so a single-grep migration pass missed them.

## Sites

### 1. `crates/renderer/src/vulkan/context/screenshot.rs:108-113` — swapchain `PRESENT_SRC → TRANSFER_SRC` barrier

```rust
.subresource_range(
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1),
);
```
Should be: `.subresource_range(color_subresource_single_mip())`

### 2. `crates/renderer/src/vulkan/context/screenshot.rs:159-164` — swapchain `TRANSFER_SRC → PRESENT_SRC` return barrier

```rust
.subresource_range(
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1),
);
```
Should be: `.subresource_range(color_subresource_single_mip())`

### 3. `crates/renderer/src/vulkan/texture.rs:381-387` — texture upload image-view subresource (struct-literal variant, but with `level_count: meta.mip_count` — needs the mip-aware helper)

```rust
.subresource_range(vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: meta.mip_count,
    base_array_layer: 0,
    layer_count: 1,
});
```
Should be: `.subresource_range(color_subresource_mips(meta.mip_count))`

## Helpers (already exist — no new code needed)

- `crates/renderer/src/vulkan/descriptors.rs:99` — `color_subresource_single_mip()` (landed 2026-05-14 via #1046, `3731df66`)
- `crates/renderer/src/vulkan/descriptors.rs:112` — `color_subresource_mips(level_count)` (same commit)

## Age

- `texture.rs:381-387` — added 2026-03-29 (`1bea5c867`), 6+ weeks before the helper landed
- `screenshot.rs:108-113` + `:159-164` — added 2026-04-13 (`ddd05a9df`), 4+ weeks before the helper

Both predate the helper, so neither is a regression — they're original sites the migration sweep missed.

## Effort
**Trivial** (≤ 30 min). 3 line-replacements + cargo check + commit.

## Sibling check (post-fix)

After this fix lands, two grep targets should both return 0 hits outside `descriptors.rs` (the canonical home):

```bash
grep -rE 'vk::ImageSubresourceRange::default\(\)' crates/renderer/
grep -rE 'vk::ImageSubresourceRange\s*\{' crates/renderer/
```

Exception: the **DEPTH-aspect** single-mip site at `crates/renderer/src/vulkan/context/helpers.rs:369` is NOT eligible for the existing helpers (different aspect_mask). Defer creating a `depth_subresource_single_mip()` until a second DEPTH site appears (not worth a helper for a single use today).

Consider adding the two-grep target to `_audit-validate.sh` or a unit-test gate as a regression guard.

## Completeness Checks
- [ ] **UNSAFE**: N/A — `ImageSubresourceRange` is a `repr(C)` POD; no safety contract
- [ ] **SIBLING**: 3 sites listed above are exhaustive for COLOR-aspect — verified via two-grep coverage. The DEPTH-aspect site at `context/helpers.rs:369` is intentionally NOT a target (see Sibling check exception)
- [ ] **DROP**: N/A — no Vulkan object lifecycle change; struct-init refactor only
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing 2493/2493 workspace tests already cover both code paths (screenshot pipeline + texture upload); no new test needed for a pure compile-time reorg

## Related
- #1046 (CLOSED) — landed `color_subresource_single_mip()` + `color_subresource_mips()` helpers in `descriptors.rs`
- #1117 / TD3-206 (CLOSED) — 8-site struct-literal migration
- #1149 / TD3-207 (CLOSED) — 14-site struct-literal migration

This issue is a straggler of the same sweep, not a new pattern. The builder-form variant required a second grep target the original migration didn't use.
