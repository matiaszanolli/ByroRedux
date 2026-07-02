# NIF-D1-01: Pre-4.1 NIF bool fields read as 1 byte where the wire format is 32-bit — full-file cascade on real v <= 4.0.0.2 (Morrowind-era) NIFs

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1843
**Labels**: bug, nif-parser, low

**Severity**: LOW *(graded per the project's #982 "out-of-tier / latent" precedent for Morrowind V4; inside a support tier this would be a sizeless-format cascade with no recovery anchor)*
**Dimension**: Stream Position
**Location**: `crates/nif/src/blocks/base.rs:279` (`has_bounding_volume` via `read_u8`), `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:326,349,379,421-422` (`has_vertices` / `has_normals` / `has_vertex_colors` / `has_uv` via `read_byte_bool`), `crates/nif/src/blocks/texture.rs:913` (`enable_plane` via `read_u8`); family likely extends to other `read_byte_bool` sites reachable pre-4.1.
**Status**: NEW

## Description

**Game Affected**: `NifVariant::Morrowind` and all NetImmerse content at NIF v ≤ 4.0.0.2 (bsver = 0). No shipped content of the target matrix (Oblivion→Starfield) sits in this band — Oblivion's oldest live band is v10.0.1.0, and its lone pre-Gamebryo file (`marker_radius.nif`, #698) is corrupt-by-design and truncates regardless.

nif.xml `<basic name="bool">`: "A boolean; 32-bit up to and including 4.0.0.2, 8-bit from 4.1.0.1 on." `NifStream::read_bool` (`stream.rs:203-211`) implements exactly this switch — but the listed sites bypass it with fixed 1-byte reads. On a real v4.0.0.2 file each such bool under-reads 3 bytes; files in this band carry no `block_sizes` table, so the drift cascades unrecoverably.

## Evidence

OpenMW (`/mnt/data/src/reference/openmw/components/nif/nifstream.cpp:170-177`) reads `bool` as `int32_t` when `version < 4.1.0.0`; its `NiAVObject::read` / `NiGeometryData::read` route these fields through the version-aware bool. The `read_byte_bool` doc comment (`stream.rs:213-215`) contradicts nif.xml for the pre-4.1 band. The synthetic Morrowind fixture (`tests/synthetic_fixtures.rs:434`) writes `has_bounding_volume` as 1 byte, so it is self-consistent with the bug and stays green while a real file drifts. Confirmed live: `stream.rs` has a correct version-aware `read_bool()` alongside a fixed-width `read_byte_bool()`; the five listed call sites all use `read_byte_bool()` / raw `read_u8()` unconditionally, with no `version() <= V4_0_0_2` branch to route to `read_bool()`.

## Impact

Latent today (band is out of the compat matrix), but every entry point for the band exists and is tested, so the parser *claims* the band. #210's closure claim ("Full Morrowind v4.0.0.2 support … verified end-to-end") is false for real files.

## Suggested Fix

Replace the fixed-width reads with `stream.read_bool()` at the listed sites; fix the synthetic fixture to write 4-byte bools at v4.0.0.2; correct the `read_byte_bool` doc comment; sweep remaining pre-4.1-reachable `read_byte_bool` sites. Validate against one real Morrowind NIF before re-claiming the band. (Ship with the related `NiDynamicEffectData` v≤4.0.0.2 affected-nodes gap, which is masked by this one and would otherwise be unreachable-correct.)

## Completeness Checks
- [ ] **SIBLING**: Sweep other `read_byte_bool` sites reachable pre-4.1 beyond the five listed (the report flags this as likely-incomplete)
- [ ] **TESTS**: A regression test with a real (or byte-accurate synthetic) v4.0.0.2 fixture using 4-byte bools, replacing the currently self-consistent-with-the-bug fixture at `tests/synthetic_fixtures.rs:434`
