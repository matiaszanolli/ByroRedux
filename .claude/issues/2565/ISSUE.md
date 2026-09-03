# #2565 — OBL-D1-04: Two latent TexDesc version gaps, plus a PS2 L/K divergence between the two TexDesc readers

**Severity**: LOW · **Dimension**: NIF Version Handling
**Location**: `crates/nif/src/blocks/properties.rs::NiTexturingProperty::{read_tex_desc, parse}`

## Fix

Verified the real field layout against the authoritative `nif.xml`
(niftools/nifxml) `TexDesc` struct rather than guessing:

```
Source            (Ref)     since=3.3.0.13
Clamp Mode        (u32)     until=20.0.0.5
Filter Mode       (u32)     until=20.0.0.5
Flags             (u16)     since=20.1.0.3
Max Anisotropy    (u16)     since=20.5.0.4
UV Set            (u32)     until=20.0.0.5
PS2 L             (i16)     until=10.4.0.1
PS2 K             (i16)     until=10.4.0.1
Unknown Short 1   (u16)     until=4.1.0.12
Has Texture Transform (bool) since=10.1.0.0, + conditional 32-byte body
```

This confirms all three defects the issue names, plus their precise root
cause:

1. **The 20.1.0.0–20.1.0.2 over-read.** `Clamp Mode`/`Filter Mode`/
   `UV Set` are gated `until="20.0.0.5"`; `Flags` is gated
   `since="20.1.0.3"` — two **different** thresholds, not complementary
   halves of one cutoff. `20.1.0.0`..`20.1.0.2` sits strictly between
   them, where nif.xml declares **neither** representation present. The
   pre-fix `else` branch keyed only on `< V20_1_0_3`, so it read the
   12-byte legacy layout for that 3-micro-version gap band too. Split the
   branch three ways: `>= V20_1_0_3` (Flags), `<= V20_0_0_5` (legacy
   triple), and the gap band in between (reads nothing, defaults to
   zero).
2. **`Unknown Short 1` never read.** Added, gated `<= V4_1_0_12`, nested
   inside the legacy branch alongside PS2 L/K (both are strict subsets of
   the `<= V20_0_0_5` legacy band).
3. **PS2 L/K divergence in the shader-map trailer.** The trailer's inline
   reader duplicated the legacy-vs-modern split but omitted the PS2 L/K
   read the primary `read_tex_desc` correctly had. Eliminated the
   duplication at the source per the issue's own suggested fix (below)
   rather than patching the copy — there is no longer a second copy to
   patch.

Also confirmed `Max Anisotropy` (`since="20.5.0.4"`) is a genuine,
separate nif.xml field this parser still doesn't decode — deliberately
left out of scope and documented in the new shared helper's doc comment:
no currently-supported game ships `NiTexturingProperty` at or past that
version (FO3/FNV top out at `V20_2_0_7`; Skyrim+ dropped
`NiTexturingProperty` entirely for the `BSShaderProperty` family), so
it's unreachable in practice. Recorded rather than guessed at, matching
this session's no-guessing discipline.

## SIBLING (issue's own checklist item — "shared helper ensures the
primary and shader-map-trailer readers can't diverge again")

Factored the entire post-`Source` body (everything the two readers
duplicated) into one new function, `read_tex_desc_body`, called by both:
- `read_tex_desc` (the primary reader) — reads the leading bool +
  `Source`, then delegates to `read_tex_desc_body` and wraps the result
  in `TexDesc`.
- The shader-map trailer loop (`NiTexturingProperty::parse`'s "Shader
  textures trailer" section) — reads `Source` (it already consumed the
  leading `Has Map` bool separately), then delegates to
  `read_tex_desc_body`, discarding the result (it doesn't retain
  shader-map TexDescs today).

There is now exactly one implementation of the `TexDesc` body layout;
the two readers structurally cannot drift out of lockstep again.

## TESTS (issue's own checklist item)

Two new tests, both against `NifVersion::V4_0_0_2` / the exact
`20.1.0.0`/`20.1.0.2` boundary values, verified by exact
`stream.position()` match (proves byte-for-byte consumption, not just
returned values):
- `parse_ni_texturing_property_at_v4_0_0_2_reads_ps2_lk_and_unknown_short1`
  — a full `NiTexturingProperty::parse` integration test (reusing the
  exact ancient-version `NiObjectNET` prologue shape the file's existing
  `..._rejects_8_bit_bool_layout` test already established) with real PS2
  L/K + `Unknown Short 1` bytes present; confirms the whole property
  consumes exactly the expected byte count.
- `tex_desc_body_reads_nothing_for_the_20_1_0_x_gap_band` — calls
  `read_tex_desc_body` directly (private-fn visibility from the
  descendant test module) at both `V20_1_0_0` and `V20_1_0_2`, asserting
  it consumes nothing beyond the trailing `Has Texture Transform` byte.

Since both call sites now share one implementation, testing
`read_tex_desc_body` (directly, and via the primary reader) transitively
covers the shader-map trailer too — no separate trailer-specific
regression test was needed; the existing
`parse_ni_texturing_property_shader_map_consumes_full_transform` /
`..._consumes_has_transform_bool` tests (unmodified, still passing)
already exercise the trailer through the shared function.

**Reintroduce-and-revert verification**: temporarily restored the
pre-fix `else` branch (keyed only on `< V20_1_0_3`, no PS2 L/K nesting
change, no `Unknown Short 1` read) — confirmed both new tests failed.
Restored the fix and reran — all 32 tests in
`blocks::properties::tests` pass again.

## Verification

- `cargo check -p byroredux-nif --tests`: clean, zero warnings.
- `cargo test -p byroredux-nif --lib blocks::properties::tests::`: 32
  tests passing, 0 failing (+2 new).
- `cargo test -q -p byroredux-nif`: 1229 tests passing (+2), 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7158 passing, 0
  failing**.
