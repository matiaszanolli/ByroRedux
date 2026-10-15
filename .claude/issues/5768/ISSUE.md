# REN-D12-2026-10-14-01: Raw `restir_history`/`restir_light` debug views are painted late in `main()`; window-portal and thick-glass fragments return first and overwrite them

**Labels**: low,renderer,shaders,bug
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D12-2026-10-14-01`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/shaders/triangle.frag` (window-portal return ~l.2017, thick-glass IOR return ~l.2701, versus the `viewRestirHistory` / `viewRestirLight` blocks)

## Description
Nothing between l.1880 and l.4057 tests `debugMode`, so those early returns write lit blended colour into raw correctness views. `material_lobe` and `rt_lod` were already hoisted above these returns for this reason. The leak predates `restir_history` (mode 16) but the new mode inherits it.

## Evidence
Grep of `debugMode`/`viewRestir*` in `triangle.frag` shows the view branches only at the post-finalize tail.

## Impact
Correctness views show lit colour on portal/glass fragments, misleading a reader of the history view.

## Suggested Fix
Hoist the raw-view branch above those returns, as was done for `material_lobe`/`rt_lod`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
