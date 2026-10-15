# REN-D5-2026-10-14-02: `groundcover.rs` module doc still says 64 MiB arena / 256 chunks / 3000-unit draw distance

**Labels**: low,renderer,terrain-exterior,documentation,doc-rot
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D5-2026-10-14-02`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/src/vulkan/groundcover.rs` (module doc, `//!` header)

## Description
`b731b01f5` made it 512 chunks, a 128 MiB arena and an 8000-unit reach; the module doc was not updated. Same rot class as #4497.

## Evidence
`groundcover.rs` ~l.24-29: "= 64 MiB device-local", "256 chunks × 16,384 blades", "3000-unit draw distance".

## Impact
Doc rot only; the memory ledger rows match the code.

## Suggested Fix
Update the module doc to the current constants.
