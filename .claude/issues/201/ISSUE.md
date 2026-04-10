# #201: SYNC-05: Bindless descriptor writes without UPDATE_AFTER_BIND
- **Severity**: MEDIUM — **Domain**: renderer — **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/texture_registry.rs:65`
- **Fix**: Enable UPDATE_AFTER_BIND_BIT on bindless array binding + pool + layout
