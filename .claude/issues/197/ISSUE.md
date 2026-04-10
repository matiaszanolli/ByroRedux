# #197: SYNC-3: Missing HOST→VERTEX_SHADER barrier for instance SSBO upload
- **Severity**: MEDIUM
- **Dimension**: Command Buffer Recording
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:323-331`
- **Source**: `docs/audits/AUDIT_RENDERER_2026-04-10b.md`
- **Fix**: Add HOST→VERTEX_SHADER memory barrier after instance upload, or document implicit guarantee
