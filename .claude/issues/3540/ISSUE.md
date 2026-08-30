# RT-1: Starfield citycydoniamainlevel never renders a frame — 10-minute single-threaded stall at 20.6 GB RSS

Source: docs/audits/AUDIT_RUNTIME_2026-08-30.md (RT-1)
State: OPEN · labels: bug, critical, performance, game:starfield, physics

Cell loads (95095 fixed colliders, rapier_bodies=95651, grounded=true), then the frame
loop stops at frame 0. CPU pinned to ONE core; RSS oscillates 12.0 -> 20.6 GB.

Prime suspects: M28.5 static-collider AABB / broad-phase build (byroredux/src/systems/character.rs),
BLAS construction over 95k bodies (crates/renderer/src/vulkan/acceleration/blas_static.rs).
