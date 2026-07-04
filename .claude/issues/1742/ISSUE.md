# SCR-D7-02: trigger-box rotation frame may not match the permuted half-extents

Filed as: matiaszanolli/ByroRedux#1742
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: LOW (verify-not-confirmed)
- **Dimension**: Engine Attach & Trigger Wiring
- **Location**: `byroredux/src/cell_loader/references.rs:1412-1436`
- **Labels**: low, import-pipeline, legacy-compat, bug

## Description
Half-extents are permuted z-up→y-up (`[x,z,y]`) but `rotation` passes through verbatim; `TriggerVolume::contains` (Box) tests `rotation.inverse() * (p-center)` against the permuted extents. If `rotation` isn't in the same permuted frame, a rotated OBB trigger is wrong. Dedicated tests only use `Quat::IDENTITY` (permute invisible). Exposure limited by Bethesda's mostly axis-aligned boxes.

## Suggested Fix
Add a rotated-box trigger test end-to-end (placement → volume → `contains`) with a non-identity REFR rotation; permute `rotation` into the extents frame if it fails.

## VERIFIED — not a bug (2026-07-04 session)
Traced the frame math by hand, then confirmed empirically. `rotation` (the
REFR's `ref_rot`, from `euler_zup_to_quat_yup_refr` — the same conversion
every other placed REFR in the cell loader uses) and `half_extents`
(z-up→y-up permuted `[x, z, y]`) ARE already in the same frame:

- `half_extents`'s permutation swaps which raw bound feeds y-up Y vs Z —
  exactly the axis-swap half of the canonical z-up→y-up conversion
  (`zup_to_yup_pos: (x,y,z) → (x,z,-y)`). The sign is irrelevant here since
  extents are unsigned (`.abs()`).
- `euler_zup_to_quat_yup_refr` (mode 1, CW+ZYX, shipping default) is
  derived to rotate y-up vectors consistently with that same position
  conversion — for a pure z-up-Z-axis (yaw) rotation it reduces to
  `Ry(-θ)`, which by hand-derivation (CW convention: `(x,y,z) →
  (x·cosθ+y·sinθ, -x·sinθ+y·cosθ, z)` in z-up, then `zup_to_yup_pos`)
  matches exactly.

Added `rotated_box_trigger_composes_rotation_in_same_frame_as_permuted_extents`
(`byroredux/src/cell_loader/references.rs`) to prove this end-to-end rather
than relying on the derivation alone: it rotates a point on the box's own
z-up **+Y** face (`bounds[1]`, one of the two permuted axes — NOT
`bounds[0]`/X, which the permutation doesn't touch and would falsely pass
even if Y/Z were swapped) using Bethesda's clockwise convention implemented
independently of `ref_rot`, converts that rotated point to y-up via the
canonical `zup_to_yup_pos` (not via anything `trigger_volume_from_primitive`
touches), and checks `TriggerVolume::contains` classifies points ±0.1 units
across that rotated face correctly.

**Confirmed the test has teeth**: temporarily swapped the `[x, z, y]`
permutation back to an unswapped `[x, y, z]` (the exact regression this
issue worried about) and the new test failed immediately
(`just_outside_world` misclassified as inside); reverted, test passes
again. No production code change — the audit's "verify-not-confirmed"
concern is resolved as **not a bug**, now locked in by a permanent
regression test.
