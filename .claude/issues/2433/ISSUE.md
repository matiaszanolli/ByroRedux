# TD9-002: gpu_instance_does_not_re_expand_with_per_material_fields is a no-op test (and cites a stale byte size)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2433
**Finding ID**: TD9-002 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 9 — Test Hygiene
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:91-101`
**Status**: NEW

## Description
`gpu_instance_does_not_re_expand_with_per_material_fields` builds `GpuInstance::default()` and discards it — since `GpuInstance` already implements `Default`, this cannot fail under any circumstance; the test is permanently green regardless of the struct's actual shape. Its comment also cites a stale "112 B" (current: 128 B since #2219, the same commit that last touched this test).

## Suggested Fix
Either delete the test (it duplicates the working sibling size guard, `gpu_instance_is_128_bytes_std430_compatible`) or add a real inline assertion that would actually catch a re-expansion; fix "112 B" → "128 B" in whichever survives.

## Age
Last touched `c4cb26146`, 2026-08-03.

## Completeness Checks
- [ ] **TESTS**: Whichever survives (deletion or fixed assertion) is confirmed to actually catch a deliberate re-introduction of a per-material field (spot-check by temporarily adding one back)
