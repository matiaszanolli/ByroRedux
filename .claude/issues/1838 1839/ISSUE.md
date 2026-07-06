# #1838 (NIF-D2-01, MEDIUM) + #1839 (NIF-D2-02, LOW) — variant-helper gates re-introduced

## Root cause
Commit `2bd447d5` (#1277 Task 5 refactor) migrated four version-gated field
reads from raw `stream.bsver()` compares back to `variant().has_*()` helpers.
nif.xml gates all four purely on BSVER, so the helpers misfire on hybrid
headers where the routed `NifVariant` is `Unknown` — `(uv=11, bsver 35..=82)`
under-reads; `(uv=12, bsver < gate)` over-reads. No retail title is affected
(every shipping game classifies onto the aligned side); the divergence hits
unclassified/hybrid exports only. #1838 also silently reverted the shipped
#982 audit fix, leaving contradicting raw-bsver comments in place.

## Sites fixed (all 4)
| Issue | File | Field | Correct gate |
|-------|------|-------|--------------|
| #1838 | `blocks/tri_shape/ni_tri_shape.rs:135` | shader/alpha refs | `bsver > FO3_FNV` |
| #1838 | `blocks/tri_shape/ni_tri_shape.rs:341` | material CRC | `bsver > FO3_FNV` |
| #1839 | `blocks/collision/shape_compound.rs:127` | MOPP Build Type | `bsver > FO3_FNV` |
| #1839 | `blocks/node.rs:257` | Culling Mode | `bsver >= SKYRIM_LE` |

## Fix
Restored `stream.bsver() > crate::version::bsver::FO3_FNV` (three sites) and
`>= SKYRIM_LE` (culling), matching the surviving comments + the raw-bsver
doctrine (#160/#982/#1331). Comments updated to reference #1838/#1839.

## Regression tests (all verified to trip on a revert)
Each parses a real block on a hybrid `Unknown`-variant header and asserts
byte consumption / field value:
- `nigeometry_data_reads_material_crc_on_hybrid_unknown_bsver_over_34`
- `ni_tri_shape_reads_shader_alpha_refs_on_hybrid_unknown_bsver_over_34`
- `mopp_bv_tree_reads_build_type_on_hybrid_unknown_bsver_over_34` (new file `collision/shape_compound_tests.rs`)
- `bs_multi_bound_node_reads_culling_mode_on_hybrid_unknown_bsver_ge_83`

## Out of scope
NIF-D2-03 (removing the now-orphaned `has_shader_alpha_refs` /
`has_material_crc` / `has_culling_mode` helpers) is a separate issue. The
helpers remain referenced by their own `version.rs` unit tests, so no
dead-code warning results from orphaning them here.
