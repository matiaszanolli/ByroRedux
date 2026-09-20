# REN-D12-2026-09-20-01: #4315's 36→38 bracket bump left four prose mentions in gpu_timers.rs still saying 18/eighteen/36-query — the doc-count rot recurred on the very next bump

- **ID**: REN-D12-2026-09-20-01
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Debug/Telemetry
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D12-2026-09-20-01)

**Location**: `crates/renderer/src/vulkan/gpu_timers.rs` — four prose sites

**Description**
The #4210 row-count test guards the table; the surrounding prose count rotted again exactly as #4210's own history predicted.

**Evidence**
Audit D12, 2026-09-20.

**Impact**
The next bracket-bump PR trusts four wrong numbers.

**Suggested Fix**
Fix the four sites; derive the prose from the constant or state it once.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
