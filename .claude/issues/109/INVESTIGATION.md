# #109 Investigation

## Premise verification

Issue filed 2026-04-05, claiming:

1. `BSLightingShaderProperty::parse` at `shader.rs:313-318` sets shader
   flags to `(0,0)` for FO76/Starfield without reading any bytes — the
   CRC32 flag array is never consumed, so downstream fields read at the
   wrong offset.
2. Same bug mirrored in `BSEffectShaderProperty::parse` at `shader.rs:664-669`.
3. Wetness params missing `unknown_1` / `unknown_2` for `BSVER > 130` at
   `shader.rs:395`.

All three **were** real bugs at the time the audit was filed. All three
**have since been fixed** (line numbers shifted as the file grew).

## Current state (verified 2026-04-22)

### BSLightingShaderProperty (now at `shader.rs:549-580`)

```rust
let (shader_flags_1, shader_flags_2) = if bsver <= 130 {
    (stream.read_u32_le()?, stream.read_u32_le()?)
} else {
    (0, 0)
};
// FO76 BSShaderType155 field (BSVER == 155 only).
let fo76_shader_type = if bsver == 155 { stream.read_u32_le()? } else { 0 };
// Num SF1 / Num SF2 (BSVER >= 132 / 152), then both arrays.
if bsver >= 132 {
    let num_sf1 = stream.read_u32_le()? as usize;
    let num_sf2 = if bsver >= 152 { stream.read_u32_le()? as usize } else { 0 };
    for _ in 0..num_sf1 { sf1_crcs.push(stream.read_u32_le()?); }
    for _ in 0..num_sf2 { sf2_crcs.push(stream.read_u32_le()?); }
}
```

Correctly consumes the CRC32 flag arrays. Commit ancestry: #403 / #409.

### BSEffectShaderProperty (now at `shader.rs:1123-1146`)

Mirrors the BSLighting path identically. Same fix.

### WetnessParams

`unknown_1` read for `bsver >= 130` (line 675), `unknown_2` for
`bsver == 155` (line 680). Struct doc at line 368 had stale "Present for
BSVER > 130" wording — updated to "BSVER >= 130" with #403 citation.

## Evidence the fix is effective

Full integration sweep today (`parse_real_nifs --ignored`):

- `parse_rate_fallout_76`: **58,469 / 58,469 NIFs clean (100.00%)**
- `parse_rate_starfield`: **31,058 / 31,058 NIFs clean (100.00%)**

Zero truncated, zero failed. If the parser were misaligned after the
variable-length flag array, downstream geometry / texture reads would
trip the `allocate_vec` bounds check (#408) and produce parse failures.
They don't — the gate shape is correct.

## Action

Doc comment sync only — no parser changes needed. Core bug is already
resolved; closing with a pointer to this note.
