# PERF-D4-2026-09-11-01: The two biggest scene SSBOs are eagerly allocated at a 5x worst-case capacity no measured cell approaches

Labels: medium,performance,renderer,memory,bug

**Description**: `instance_buffers` and `previous_model_buffers` are allocated at context init at the full `MAX_INSTANCES` (262144) ceiling — 262144 x 160 B and 262144 x 64 B, x 2 FIF = 83.9 MB + 33.6 MB = 117.5 MB resident, roughly half the entire scene-buffer budget. `MAX_INSTANCES` itself is deliberately sized with ~5x headroom over the densest observed city cells (~50K REFRs); at the densest measured real workload (MedTek, 7359 instances) only ~3.3 MB is needed. The remaining ~95 MB is never touched by any measured workload. This is a residency finding, not an upload one — upload is already correctly O(live data).

**Evidence**:
`crates/renderer/src/vulkan/scene_buffer/buffers.rs:495-497,565-576` — unconditional `MAX_INSTANCES`-sized allocation inside the `0..MAX_FRAMES_IN_FLIGHT` loop, no resize path. `constants.rs:97-141`, `docs/engine/memory-budget.md:92-93`.

**Impact**: ~95 MB of VRAM held for a case no shipped content reaches (~2.4% of the 4 GB budget ceiling, ~5% of the doc's ~1.91 GB typical-FNV-interior total). Not a frame-time cost.

**Related**: `memory-budget.md:92-93,109,793`; #992 (mesh-ID encoding, unaffected).

**Suggested Fix**: Allocate at a working capacity (e.g. 64K instances = 28.7 MB for the pair) and grow by doubling up to `MAX_INSTANCES` at a frame/cell-load boundary, rewriting the two descriptor writes on grow. Must not ship without on-device measurement; if the grow machinery is judged not worth the risk, document the 117.5 MB as an accepted cost instead.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
