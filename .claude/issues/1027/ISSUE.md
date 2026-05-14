# Issue #1027

**Title**: REN-D3-001/002: Stale '76-byte Vertex' doc comments — actual stride is 100 B post-M-NORMALS

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D3-001 / REN-D3-002
**Severity**: LOW (doc-only)
**Files**: `crates/renderer/src/vulkan/pipeline.rs:561` (UI pipeline) ; `crates/renderer/src/vertex.rs:219` (UiVertex)

## Issue

Both sites comment "76-byte Vertex" — actual `Vertex` size is 100 B (25 floats) since #783 added the tangent slot for M-NORMALS. Runtime is correct (`vertex_size_matches_attribute_stride` test pin holds), only the docs are stale.

## Fix

Mechanical edit: replace the comment text. Two-line change.

