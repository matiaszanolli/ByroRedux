# NIFAL-D2-2026-09-21-01: Static NiTransform translate boundary is non-finite-blind — NaN/inf rotations, translations and scales reach BLAS/TLAS as a non-finite AABB

**Labels**: high, nifal, nif-parser, nif, safety, vulkan, bug

**Severity**: HIGH · **Dimension**: Geometry/Transform · **Tier Violated**: no-leak · **Game Affected**: all (corrupt/hand-edited NIFs, modded content; vanilla incidence 0)
**Location**: `crates/nif/src/rotation.rs:14-20`, `:69-81`, `:165-197`; `crates/nif/src/import/coord.rs:58-70`; `crates/nif/src/stream.rs:745-755` / `:769-779` (both readers — translation/scale ungated); consumer `byroredux/src/scene/nif_loader.rs:1245-1253`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
Every gate on the static-transform path is an unordered float comparison, and all such comparisons are false for NaN:
- A **NaN** rotation matrix passes `sanitize_rotation` unchanged — `is_degenerate_rotation`'s `(det-1.0).abs() >= 0.1` is false on a NaN determinant and `is_non_orthonormal`'s column checks are likewise NaN-blind. `zup_matrix_to_yup_quat`'s `(det - 1.0).abs() < 0.1` fast path is also false, routing to SVD whose arithmetic on NaN yields an all-NaN quaternion (`normalize_quat`'s zero-length guard does not catch NaN).
- An **infinite** entry trips the degenerate branch, but `repair_rotation_svd_or_identity` then *returns a NaN matrix*: `max_sv < 0.01` (rotation.rs:176) is false for NaN singular values, so the "no meaningful orientation → identity" escape never fires — the repair function violates its own doc contract for non-finite input.
- `translation` and `scale` have no finite gate at all.

The NaN flows `Quat::from_xyzw` → `Transform` → `GlobalTransform` → TLAS instance / skinned BLAS refit with a non-finite AABB — undefined behaviour under Vulkan's AS-build contract, the exact chain `d53be91be` closed for the animation side (#4396/#4397). Repo-wide grep confirms no `is_finite` gate on static transforms downstream; the single downstream check (`cell_loader/spawn.rs:200-206`) only skips the mesh from the collision-proxy bounds union — the entity still spawns poisoned, and that check is itself the leak pattern in miniature. The collision and emitter paths already gate their own transforms, so geometry is the outlier, not the convention.

### Evidence
Call path: `read_ni_transform` (raw point + `sanitize_rotation(read_ni_matrix3())` + raw scale) → `compose_transforms` → `zup_matrix_to_yup_quat` (coord.rs:62) → `ImportedMesh.rotation` → `Quat::from_xyzw` (nif_loader.rs:1245, no gate).

### Impact
One corrupt float in any placed NIF poisons the entity's `GlobalTransform`; for a skinned or TLAS-instanced mesh that is a non-finite AABB in an acceleration-structure build — GPU UB / potential `VkDevice` loss, not a visual glitch. All games; corrupt/mod content only (vanilla incidence measured 0 for adjacent classes: #3532's 642,589-matrix census, #4397's 16.06M-key census).

### Related
#4396, #4397 (fixed anim-side siblings, `d53be91be`); #4166 (blast-radius precedent); #2383

### Suggested Fix
Gate the nine rotation cells, translation and scale on `is_finite()` at the two `stream.rs` read sites (or at the head of `sanitize_rotation`: non-finite → identity, matching the `max_sv < 0.01` precedent, behind the existing rate-limited warn). Have `repair_rotation_svd_or_identity` verify its output determinant is finite before returning.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix

- [ ] **SIBLING (anim side stays gated)**: the #4396/#4397 `normalized_rotation_sample` gates and the collision/emitter finite guards must not be weakened
