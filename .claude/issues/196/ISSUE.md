# #196: SYNC-2: Instance SSBO uploaded after cluster cull dispatch
- **Severity**: MEDIUM
- **Dimension**: Command Buffer Recording
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:207-227, 323-327`
- **Source**: `docs/audits/AUDIT_RENDERER_2026-04-10b.md`
- **Fix**: Update misleading barrier comment, or move instance upload before dispatch
