# NIF-D6-01: BhkMultiSphereShape::parse fills Vec<[f32;4]> with a per-element push loop instead of bulk read

**Issue**: #1902 · **Severity**: LOW · **Labels**: low, nif-parser, nif, bug
**Dimension**: Allocation Hygiene · **Filed from**: docs/audits/AUDIT_NIF_2026-07-06.md (nif-deep suite)
**Location**: crates/nif/src/blocks/collision/shape_primitive.rs:58-65 (BhkMultiSphereShape::parse)

## Description
The sphere array is a contiguous run of [f32;4] (cx,cy,cz,r) with no per-element transform — byte-
identical to read_ni_color4_array / read_pod_vec::<[f32;4]>. Current code does allocate_vec then a
for-loop of 4× read_f32_le + push (lines 59-65). The allocate-then-loop-fill shape the read_pod_vec
discipline (#833/#873) collapses for POD arrays.

## Impact
Negligible — NOT a #833 regression (already single-alloc, no intermediate Vec<u8>), num_spheres ≤~8.
Filed LOW for idiom consistency only.

## Suggested Fix
`let spheres = stream.read_ni_color4_array(num_spheres as usize)?;` — one read_exact. Byte-equivalent
on the LE host the crate requires.

**Related**: #833, #873.
