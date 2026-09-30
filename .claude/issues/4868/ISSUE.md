# #4868: REN-D12-2026-09-24-05: `bracket_ms` uses `saturating_sub` on raw timestamps and never applies `timestampValidBits`

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D12-2026-09-24-05**._

- **Severity**: LOW (hardening; device-dependent, cannot trigger on the 64-bit dev GPU).
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs` — `snapshot_from_bits` / `bracket_ms`; `device.rs` (`timestamp_supported`).
- **Status**: NEW
- **Description**: `bracket_ms` computes `e.saturating_sub(s) as f32 * ticks_to_ms` and never reads the queue family's `timestampValidBits` (only mentioned in a comment). The spec range is 36..64; a device below 64 bits wraps, and `saturating_sub` then reports `0.0` with `_active == true` for a bracket that straddles the wrap. It also feeds the ray-budget controller as "no measurement". The `device.rs` doc already cites Intel Arc's non-1.0 `timestampPeriod`, so non-NVIDIA devices are in scope.
- **Suggested Fix**: Carry `timestamp_valid_bits` in `DeviceCapabilities` and compute `(e.wrapping_sub(s)) & mask`. Needs a device reporting < 64 bits.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

