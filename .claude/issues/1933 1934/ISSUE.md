# #1933: TAA-D13-02 — taa.rs comment claims the OTHER history slot is UNDEFINED on frame 0

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/taa.rs:530-538` (comment inside `write_descriptor_sets`)

## Description
The comment asserted "on session frame 0, the OTHER slot's images are in
`UNDEFINED` layout (initialized but never written)." That's stale:
`initialize_layouts` (called once right after `new()`) transitions all
history slots UNDEFINED→GENERAL before any dispatch runs. At frame 0 the
OTHER slot is in GENERAL layout with undefined contents — there is no
layout hazard, only a contents one, and the `params.y > 0.5` first-frame
guard still correctly avoids reading the undefined contents.

## Suggested Fix
Reword to "the OTHER slot's images are in GENERAL layout but hold
undefined contents (allocated + layout-initialised, never written)."

---

# #1934: CAUSTIC-D14-01 — #1234 named-macro fix in caustic_splat.comp has no regression-test coverage

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/shader_constants.rs` (`triangle_shaders_use_named_instance_flag_constants`);
target `crates/renderer/shaders/caustic_splat.comp:200`

## Description
The #1234 fix — replacing the bare literal `4u` with
`INSTANCE_FLAG_CAUSTIC_SOURCE` in `caustic_splat.comp` — is not protected
by any regression test. The anti-literal scan iterates only over
`triangle.frag`/`triangle.vert` and searches for the token `inst.flags`.
`caustic_splat.comp` is absent from that list, and its accessor is a local
`flags` variable (`uint flags = instances[instIdx].flags;`), a different
pattern the scan wouldn't match even if added naively. A future edit
reverting caustic line 200 to `flags & 4u` would compile clean and pass
the entire suite.

## Suggested Fix
Extend `triangle_shaders_use_named_instance_flag_constants` (or add a
sibling) to also scan `caustic_splat.comp`, generalizing the accessor
pattern to also catch the local-variable form.
