# Issue #1134 — PERF-D8-NEW-01: instance buffer dirty-gate

**Source**: AUDIT_PERFORMANCE_2026-05-16 (sibling of closed #878)
**Severity**: MEDIUM (perf)
**Status**: CLOSED in 4f55b2f1

## Resolution

Mirror of #878's content-hash dirty-gate on `upload_materials`, applied to `upload_instances`. New `hash_instance_slice` helper, `last_uploaded_instance_hash` field on `SceneBuffers`, 4 regression tests in `instance_hash_tests.rs`.

Sibling for follow-up: `upload_lights` also lacks the gate but volume is ~100× less.
