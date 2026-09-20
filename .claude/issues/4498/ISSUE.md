# EXT-D3-2026-09-19-03: packed LOD word loses f32 integer precision after ~19.4 h uptime — tier corrupts into wrong-geometry draws

- **ID**: EXT-D3-2026-09-19-03
- **Labels**: low,terrain-exterior,shaders,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4498

**Severity**: LOW (clock-bounded, visual-only, restart-recovers) · **Dimension**: Ground cover (LOD addressing) · **Game Affected**: all (exterior scenes)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D3-2026-09-19-03)

**Location**: `crates/renderer/src/vulkan/groundcover.rs:1374-1381` (pack), `:1854-1862` (`+ tier as f32` per stream); `crates/renderer/shaders/groundcover_blade.vert:104-106` (unpack)

**Description**
The packed LOD word is computed in f32: `gust_and_timing[3] = (time_seconds.max(0.0) * 60.0).floor() * 4.0`, then each stream adds `tier as f32`. `time_seconds` is the unbounded `TotalTime` resource. f32 represents integers exactly only to 2^24; `serial × 4` crosses that at t > 69,905 s ≈ **19.4 h** of continuous uptime. Beyond it the spacing is 2, so the odd tier addition rounds away (`X + 1.0 → X`): the mid ribbon stream silently decodes as tier 0 while its indirect command still carries mid per-point vertices (`accepted × 6`) — point indices land in neighbouring chunks' blade slabs, and `GC_FRAME_SERIAL` desynchronises that stream's blue-noise rank. Same corruption shape as #4056 (fixed), arriving via float rounding instead of a narrow mask. Tier 2 (even) survives; correctness recovers on restart.

**Related**: #4056 (the packing's bit-width ancestor), #4296 (shares the push field)

**Suggested Fix**
Mask the serial to ≤ 22 bits before packing (`serial & 0x3F_FFFF`) on both sides — the blue-noise tile rotation (`vFrameSerial * uvec2(5u,3u)`) already wraps by construction, so a 22-bit period is free — keeping `serial*4 + tier` exactly representable in f32 forever; extend `lod_tier_field_is_wide_enough_for_every_indirect_stream` with a `t > 2^24/4/60` case (the current guard tests `t ∈ {0, 1, 12345}` only).

## Completeness Checks
- [ ] **SIBLING**: Check other `as f32` packings of `TotalTime`-derived words for the same 2^24 horizon
- [ ] **TESTS**: Round-trip guard extended past the precision horizon
