# REN-D12-2026-10-14-02: `viewRestirHistory` block sits under the comment describing `viewRestirLight`

**Labels**: low,renderer,shaders,documentation,doc-rot
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D12-2026-10-14-02`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/shaders/triangle.frag` (`viewRestirHistory` block), from `c2647d468`

## Description
The "ReSTIR-selected light per pixel: stable categorical hue…" comment now precedes `if (viewRestirHistory)`, which has no comment of its own; the comment belongs to the `viewRestirLight` block below it.

## Evidence
`triangle.frag` ~l.4051-4063.

## Impact
Comment rot only.

## Suggested Fix
Give the history block its own comment; move the light-hue comment back down.
