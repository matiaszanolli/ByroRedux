# Issue #947 — REN-D4-NEW-01: Outgoing subpass dep omits EARLY_FRAGMENT_TESTS in src_stage_mask

**Severity**: LOW
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-11_DIM4.md
**Labels**: bug, renderer, vulkan, sync, low

## Location

`crates/renderer/src/vulkan/context/helpers.rs:149-159` — `dependency_out` builder.

## Evidence

`dependency_out.src_stage_mask = COLOR_ATTACHMENT_OUTPUT | LATE_FRAGMENT_TESTS`. The incoming subpass dep (lines 129-133) covers both `EARLY` + `LATE` depth stages.

## Fix sketch

Add `EARLY_FRAGMENT_TESTS` to `dependency_out.src_stage_mask` for symmetry. One-line change.
