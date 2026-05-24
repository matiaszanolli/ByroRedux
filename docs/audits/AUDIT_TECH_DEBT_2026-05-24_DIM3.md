# Tech-Debt Audit — Dimension 3: Logic Duplication (2026-05-24)

Focused sweep — only Dimension 3 (Logic Duplication). The morning's full-10-dim sweep ([`AUDIT_TECH_DEBT_2026-05-24.md`](AUDIT_TECH_DEBT_2026-05-24.md)) marked D3 verified-clean ("No new duplication site detected. Coord-flip remains canonical."). This sweep looked harder.

## Executive Summary

- **1 NEW LOW finding** + **5 PASS verifications**.
- The NEW finding is a 3-site straggler from the #1149 (TD3-207) `ImageSubresourceRange` consolidation sweep — same pattern, same helper, just missed in the original migration.
- All other recently-tracked Dim 3 fronts (NIF macros, ESM common-fields, coord-flip, DDS format mapping, Vulkan barriers) verified post-consolidation-sweep clean.

| Severity | NEW | Carryover | Total |
|----------|-----|-----------|-------|
| HIGH     | 0   | 0         | 0     |
| MEDIUM   | 0   | 0         | 0     |
| LOW      | 1   | 0         | 1     |

## Method

Verified each known Dim 3 consolidation point post its tracking issue's closure:

| Front | Tracking | Verification |
|---|---|---|
| NIF `impl_ni_object!` macro adoption | #1043 | 69 uses across 25 block files (~2.8/file). Solid adoption. ✓ |
| ESM `CommonItemFields::from_subs` / `CommonNamedFields::from_subs` | #1045, #1113 | 9 sites in `items.rs`, 1 in `tree.rs`, 4 in `common.rs` test fixtures. Adoption progressed beyond TD3-203 stall. ✓ |
| Vulkan `vk::MemoryBarrier` inline emission | #1056, #1061 | 0 inline sites in `draw.rs`, 1 across the whole renderer crate. Post-consolidation clean. ✓ |
| Coord-flip (Z-up → Y-up) outside canonical home | #1044 | Sole canonical source: `crates/core/src/math/coord.rs` (`zup_to_yup_pos`, `zup_to_yup_quat_wxyz`, `euler_zup_to_quat_yup`). One external consumer: `crates/spt/src/import/mod.rs:175` using `byroredux_core::math::coord::zup_to_yup_pos` — correct call. ✓ |
| DDS format mapping (BC1/BC3/BC5/BC7/RGBA) | — | Single source: `crates/renderer/src/vulkan/dds.rs`. All FourCC + DXGI variants routed through one match. ✓ |
| `ImageSubresourceRange` inline literals | #1117 (TD3-206), #1149 (TD3-207) | **3 stragglers found** — see finding below. |

## Findings

### LOW

#### TD3-NEW-01 — 3 inline `ImageSubresourceRange` literals missed by the #1117 / #1149 consolidation sweep

- **Dimension**: 3 (Logic Duplication)
- **Severity**: LOW
- **Effort**: trivial (≤ 30 min; 3 small edits + cargo check)
- **Status**: NEW

##### Sites

1. **`crates/renderer/src/vulkan/context/screenshot.rs:108-113`** — swapchain `PRESENT_SRC → TRANSFER_SRC` barrier:
   ```rust
   .subresource_range(
       vk::ImageSubresourceRange::default()
           .aspect_mask(vk::ImageAspectFlags::COLOR)
           .level_count(1)
           .layer_count(1),
   );
   ```
   Should be: `.subresource_range(color_subresource_single_mip())`

2. **`crates/renderer/src/vulkan/context/screenshot.rs:159-164`** — swapchain `TRANSFER_SRC → PRESENT_SRC` return barrier:
   ```rust
   .subresource_range(
       vk::ImageSubresourceRange::default()
           .aspect_mask(vk::ImageAspectFlags::COLOR)
           .level_count(1)
           .layer_count(1),
   );
   ```
   Should be: `.subresource_range(color_subresource_single_mip())`

3. **`crates/renderer/src/vulkan/texture.rs:381-387`** — texture upload image-view subresource (struct-literal variant):
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

##### Helpers (already exist, no new code needed)

- `crates/renderer/src/vulkan/descriptors.rs:99` — `color_subresource_single_mip()` (landed 2026-05-14 via #1046, `3731df66`)
- `crates/renderer/src/vulkan/descriptors.rs:112` — `color_subresource_mips(level_count)` (same commit)

##### Why these were missed

The helpers landed 2026-05-14. The inline sites were authored before that date:
- `texture.rs:381-387` — 2026-03-29 (`1bea5c867`), 6+ weeks before the helper
- `screenshot.rs:108-113` + `:159-164` — 2026-04-13 (`ddd05a9df`), 4+ weeks before the helper

The #1117 / #1149 sweeps migrated 8 + 14 = 22 sites total but left these 3 untouched. The screenshot-side sites are easy to miss (they use the *builder* `::default().aspect_mask(...).level_count(...).layer_count(...)` form, which doesn't textually match the struct-literal form most sites use — different grep target).

##### Why this is consolidation, not just polish

The morning's full-10-dim sweep verified ImageSubresourceRange front clean ("Workspace texture / barrier / descriptor scaffolding — No new duplication site detected"). That verification used the same struct-literal grep target as #1149's original sweep. The builder-form variant (`::default().aspect_mask(...).level_count(1).layer_count(1)`) is functionally identical but textually invisible to that grep — that's the actual gap this finding fills.

##### Sibling check

After this fix lands, both grep targets need to be added to the audit-skill or a regression test:
- `vk::ImageSubresourceRange::default\(\)\s*\n` (builder form)
- `vk::ImageSubresourceRange\s*\{` (struct literal)

Both should grep to 0 hits outside `descriptors.rs` (the canonical home) and any `aspect_mask: DEPTH` sites (which need their own helper — see Notes).

##### Completeness Checks

- [ ] **UNSAFE**: N/A — `ImageSubresourceRange` is a `repr(C)` POD; no safety contract
- [ ] **SIBLING**: 3 sites listed above are exhaustive for the COLOR-aspect case (verified via two-grep coverage). DEPTH-aspect site at `context/helpers.rs:369` is NOT eligible for the existing helper (different aspect)
- [ ] **DROP**: N/A — no Vulkan object lifecycle change; struct-init refactor only
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing 2493/2493 workspace tests already cover the screenshot path and texture upload; no new test needed for a pure compile-time reorg. Optionally: add the two-grep regression target to `_audit-validate.sh` or a unit-test gate

##### Related

- #1046 (CLOSED) — landed the helpers
- #1117 / TD3-206 (CLOSED) — 8-site migration
- #1149 / TD3-207 (CLOSED) — 14-site migration
- This finding is a straggler of the same sweep, not a new pattern

## Notes

- **DEPTH-aspect single-mip site at `crates/renderer/src/vulkan/context/helpers.rs:369`** is an honorable mention — it follows the same struct-literal pattern as the COLOR sites but uses `aspect_mask: DEPTH`. Not eligible for the existing helpers; would need a sibling `depth_subresource_single_mip()` helper if a second DEPTH site ever appears. For one site only, the cost (new helper + adoption) is higher than the duplication cost (5 inline lines). Defer to "if a second DEPTH site appears."
- **Effort estimate is genuinely trivial** — 3 line-replacements, one cargo check, one commit. No reason to defer.
- This sweep took ~15 minutes inline (no Task agents needed). Confirms the morning's "verified-clean" framing was *almost* right — the morning sweep used one grep target; this sweep used two.
