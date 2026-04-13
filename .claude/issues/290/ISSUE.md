# #290: PERF-04-13 LOW findings bundle: depth bias, worldDist, scratch vecs, query locks

## Source: docs/audits/AUDIT_PERFORMANCE_2026-04-13.md

### P1-02: Unconditional depth bias command per batch
- **Location**: `draw.rs:534-540`
- Track `last_is_decal`, only emit `cmd_set_depth_bias` on change. ~2us/frame.

### P1-03: Redundant worldDist calculation in fragment shader
- **Location**: `triangle.frag:508,539`
- `float shadowDist = worldDist;` — reuse existing value.

### P2-02: BLAS scratch buffer never shrinks
- **Location**: `acceleration.rs:196-212`
- Destroy scratch after LRU eviction if >4x remaining need. 1-16MB wasted VRAM.

### P2-03: Scene buffer flush covers entire allocation
- **Location**: `buffer.rs:503-530`
- Add `flush_range()` with exact byte count. No-op on HOST_COHERENT (desktop GPUs).

### P2-04: GpuInstance doc comment says 144 bytes, actual is 160
- **Location**: `scene_buffer.rs:45-46`
- Stale comment. Update to "160 bytes per instance, 16-byte aligned (10x16)".

### P4-02: SubtreeCache invalidated on any Name count change
- **Location**: `systems.rs:138-149`
- Track per-root-entity generation for finer invalidation.

### P4-03: GlobalTransform queried twice in build_render_data
- **Location**: `render.rs:97,176`
- Hoist query above both passes and reuse.

### P4-04: query_2_mut for lights uses unnecessary write lock
- **Location**: `render.rs:402`
- Use two separate read queries.

### P4-05: Name storage queried 3x at animation_system start
- **Location**: `systems.rs:134,152,158`
- Query once, reuse count for both generation checks.

### P6-01: draw_commands Vec not amortized across frames
- **Location**: `render.rs:67`
- Store as App field, `.clear()` each frame. ~24-96KB/frame.

### P6-02 (vecs): gpu_lights Vec not amortized across frames
- **Location**: `render.rs:69`
- Same — persistent Vec with `.clear()`. ~2-4KB/frame.

### P6-03: entities_with_players and stack_entities Vecs allocated per frame
- **Location**: `systems.rs:182,340`
- Convert animation_system to closure-based with persistent scratch Vecs.

## Completeness Checks
- [ ] **TESTS**: Existing tests stay green for all changes
