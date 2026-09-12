# Batch: #4148, #4149, #4150, #4151, #4152

## #4148 — NIF-D1-2026-09-11-01: Unbounded recursion in read_and_skip_bounding_volume's UNION arm
**Severity**: HIGH · **Location**: `crates/nif/src/blocks/base.rs:306-337`

The `UNION` bounding-volume arm (bv_type==4) recurses once per declared child with no depth
limit — 8 on-disk bytes per level, so an N-byte file can drive recursion to depth N/8. Native
Rust call-stack recursion, not a bounded loop/heap alloc — process abort (stack overflow), not
a returned Err.

Fix: Thread a `depth: u32` param, `Err(InvalidData)` past a small cap (e.g. 32).

## #4149 — NIF-D5-2026-09-04-02: Starfield BSLightingShaderProperty runs the FO76 skin/hair-tint tail after the block has ended
**Severity**: HIGH · **Location**: `crates/nif/src/blocks/shader.rs:1249-1254,1300,1595-1630`

`parse_fo76_plus` gates the FO76-only translucency/texture-array tail on `bsver < STARFIELD`
with a comment saying Starfield's block ends before it. Three lines later,
`parse_shader_type_data_fo76` is called UNCONDITIONALLY, and for shader_type 4/5 reads 12-16
more bytes with no bsver gate — contradicting the adjacent comment.

Fix: Corpus-verify; if genuinely absent on Starfield, gate the call the same way.

## #4150 — NIF-D1-2026-09-11-02: Unknown BoundVolumeType continues without consuming its body
**Severity**: MEDIUM · **Location**: `crates/nif/src/blocks/base.rs:328-334`

Unrecognized bv_type wildcard arm logs "skipping" but has no `stream.skip()` call, unlike every
other arm — stream left mid-body with zero bytes consumed.

Fix: Return `Err(InvalidData)` instead so the caller's recovery paths engage.

## #4151 — NIF-D2-2026-09-11-01: carries_typed_shader_flags/carries_crc_shader_flags mis-gate BSSkyShaderProperty/BSWaterShaderProperty at bsver==131
**Severity**: MEDIUM · **Location**: `crates/nif/src/version.rs:524-552`; `crates/nif/src/blocks/shader.rs:414-459,497-565`

`parse_skyrim_shader_base` is shared by 4 block types but the bsver==131 "gap band" predicate is
only correct for BSLightingShaderProperty/BSEffectShaderProperty. nif.xml gates
Sky/WaterShaderProperty on the un-split `!#BS_GTE_132#` (bsver < 132) — both genuinely carry
typed flags at 131.

Fix: Give Sky/Water their own gate (bsver < 132, no gap band); add bsver-131 fixtures for both.

## #4152 — NIF-D2-2026-09-11-02: NiTexturingProperty.Apply Mode gates on STRING_TABLE_THRESHOLD, a constant reserved for header/stream code
**Severity**: MEDIUM · **Location**: `crates/nif/src/blocks/properties.rs:285`; `crates/nif/src/version.rs:165-174`

Numerically correct today but `STRING_TABLE_THRESHOLD`'s own doc declares a lockstep contract
scoped to header.rs/stream.rs only. properties.rs is an undeclared third consumer.

Fix: Add a distinct constant for this call site instead of reusing STRING_TABLE_THRESHOLD.
