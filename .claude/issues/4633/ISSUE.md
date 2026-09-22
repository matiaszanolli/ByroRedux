# NIFAL-D2-2026-09-21b-01: #4549's finite gate is inputs-only — compose_transforms' own arithmetic can still manufacture inf before the TLAS instance build

**Issue**: #4633
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIFAL_2026-09-21b.md)

**Severity**: MEDIUM. Defense-in-depth residual on the closed HIGH #4549: the direct NaN/inf path is closed; this one needs an overflowing product.
**Dimension**: Geometry/Transform
**Tier Violated**: no-leak
**Game Affected**: all (corrupt or modded NIFs; vanilla incidence 0, same premise as #4549)
**Location**: `crates/nif/src/import/transform.rs:13-25` (`compose_transforms`); `crates/core/src/ecs/systems.rs` (`GlobalTransform` propagation); `crates/renderer/src/vulkan/acceleration/predicates.rs:115-121` (`tlas_instance_transform`).

## Description
#4549's `is_finite()` gates (`rotation.rs:297`, `:241`/`:264`, `stream.rs:753`/`:780`) are input-only. `compose_transforms` computes `scale = parent.scale * child.scale` and a translation product with no check on the *result*. Two finite corrupt scales (e.g. 1e20 × 1e20) compose to +inf even though both inputs pass the gate. No `is_finite`/`is_nan` exists in production code in `acceleration/`, `render/static_meshes.rs`, `render/skinned.rs`, or the propagation system (only test asserts at `systems.rs:922-923`). `tlas_instance_transform` passes `draw_cmd.model_matrix` straight to `VkTransformMatrixKHR` with no check.

## Evidence
Among random corrupt 32-bit float patterns: ~0.4% are NaN/inf directly (gated by #4549); ~5% have exponent ≥ 2^115 — finite individually, pass the gate, overflow on the first product. The ungated residual is the *more likely* outcome of a random corruption.

## Impact
A non-finite TLAS instance transform or AABB — the #4166/#4549 class — can re-enter through the boundary's own arithmetic despite #4549's input gate.

## Suggested Fix
Validate the product, not just the inputs. Gate `compose_transforms`'s output and `GlobalTransform` propagation on finiteness, or add one check at `tlas_instance_transform`/the instance build that drops the instance with a rate-limited warning.

## Related
#4549, #4396, #4397, #4166 (all closed). #4621 (`NIF-D4-2026-09-21-02`, open): `BSSkin::BoneData` bind transforms bypass the same finite-gate class on a different producer.

## Source
docs/audits/AUDIT_NIFAL_2026-09-21b.md (NIFAL-D2-2026-09-21b-01)
