# Issue #935 Investigation — `until=X` semantic sweep

## Doctrine to flip

niftools' own `verexpr` token table (`/mnt/data/src/reference/nifxml/nif.xml:6-9`)
defines `#NI_BS_LTE_FO3#` with the operator `#LTE#` and the description
"All NI + BS *until* Fallout 3" — the description writes "until"
colloquially; the operator is `<=`. nifly mirrors this consistently:

- `/mnt/data/src/reference/nifly/src/Shaders.cpp:25` — `if (fileVersion <= V10_0_1_2)`
- `/mnt/data/src/reference/nifly/src/Objects.cpp:217` — `if (fileVersion <= V10_0_1_3)`

ByroRedux's #765 / #769 sweep chose the **exclusive** interpretation
(`version < V10_0_1_3` excludes 10.0.1.3). The fix is to flip every
such site to `<= V10_0_1_3` (inclusive) to match niftools/nifly.

## Sites to flip

`grep -rn "see #765 sweep\|exclusive" crates/nif/src/blocks/` finds the
following call sites — every one matches the pattern
`if stream.version() < NifVersion(0xN)` paired with an `until=X`
comment.

| File | Line | Field | until= |
|------|------|-------|--------|
| base.rs | 89 | NiObjectNETData Velocity | 4.2.2.0 |
| base.rs | 112 | NiAVObjectData (legacy bound volume read) | 4.2.2.0 |
| base.rs | 343 | (doc-comment site, no flip needed — verify) | 4.2.2.0 |
| extra_data.rs | 74 | NiExtraData Name | 4.2.2.0 |
| node.rs | 504 | (verify) | various |
| properties.rs | 40 | NiMaterialProperty Flags | 10.0.1.2 |
| properties.rs | 225 | (NiSpecularProperty / similar) Flags | 10.0.1.2 |
| properties.rs | 235 | (?) | 20.1.0.1 |
| properties.rs | 451 | TexDesc PS2 L/K | 10.4.0.1 |
| properties.rs | 1306 | NiFogProperty Flags | 10.0.1.2 |
| properties.rs | 1515 | NiStencilProperty (already inclusive — flip in opposite direction is a NO-OP) | 10.0.1.2 |
| properties.rs | 1521 | (paired check on NiStencilProperty) | 10.0.1.2 |
| texture.rs | 54 | NiSourceTexture Use Internal | 10.0.1.3 |
| texture.rs | 403 | (doc-comment) | 10.0.1.3 |
| texture.rs | 441 | NiSourceTexture Use Internal post-write | 10.0.1.3 |
| texture.rs | 677 | NiTextureEffect PS2 L/K | 10.2.0.0 |
| texture.rs | 690 | (early texture effect field) | 4.1.0.12 |
| controller/sequence.rs | 257 | NiControllerSequence (?) | various |
| collision.rs | 401 | (collision shape gate) | 10.0.1.2 |
| tri_shape.rs | 132 | (Oblivion bound volume / similar) | various |
| tri_shape.rs | 1467 | (Morrowind-era TriShape data) | 4.0.0.2 |
| particle.rs | 690 | (particle since/until pair) | 20.1.0.3 |
| particle.rs | 862 | (Oblivion particle gate) | various |
| particle.rs | 1315 | (PS2 L/K analog on particles) | 10.4.0.1 |
| interpolator.rs | 294 | (since/until pair) | 10.1.0.0 |
| interpolator.rs | 724 | (legacy interp data) | 4.2.2.0 |
| interpolator.rs | 1476 | (doc-comment) | 10.1.0.0 |
| interpolator.rs | 1498 | (Order field on legacy interp) | 10.1.0.0 |

## NiStencilProperty special case

`properties.rs:1515-1521` already uses the **inclusive** boundary
(`< NifVersion(0x0A000103)` = `<= V10_0_1_2`). This is correct under
the new doctrine. **Do not flip it again** — it would over-shoot to
`<= V10_0_1_3`. Keep as-is, but update the comment to reflect that it
now matches the doctrine (was previously documented as a deviation).

## NIF-D1-NEW-02 sub-fix

`crates/nif/src/blocks/controller/shader.rs:75` reads `target_color`
unconditionally despite nif.xml `since="10.1.0.0"`. Add a version gate
`if stream.version() >= NifVersion(0x0A010000) { read } else { default }`.
This is a `since=` gate, not an `until=` flip — distinct from the main
sweep but bundled per the audit.

## Mechanical replacement strategy

`Edit` with `replace_all` per file would be easiest, but each site has
a slightly different version literal (`0x0A000103`, `0x0A000102`,
`0x04020200`, etc.) so a single regex won't work. I'll do per-site
edits, making sure:

1. Comments updated from "exclusive" → "inclusive" / "per niftools"
2. The version literal stays the same — only the operator changes
   (`<` → `<=`)
3. NiStencilProperty's already-inclusive site gets its comment fixed

## Doctrine doc

Add a top-of-`version.rs` doctrine paragraph documenting:

- `until=X` per nif.xml is **inclusive** (`<= X`)
- `since=X` per nif.xml is **inclusive** (`>= X`)
- The post-#765/#769 sweep that interpreted `until=` as exclusive was
  reverted in this commit; future contributors should follow nifly /
  niftools.

## Regression test

Add a unit test in `texture.rs::tests` building a v10.0.1.3
NiSourceTexture with `use_external == false` and verifying that the
`Use Internal` byte IS read (under the new inclusive doctrine).

## Risk surface

- Bethesda content unaffected — every gate is older than 20.0.0.5.
- Pre-Bethesda content (Civ4 etc.) was previously broken; now it works.
- Test corpus on disk is Bethesda-only, so empirical regression risk
  is minimal.
- The 49 existing `properties.rs` tests + the per-block fixture tests
  must continue to pass — they were written under the exclusive
  doctrine but the test fixtures are at v20.0.0.5+ where the doctrine
  doesn't bite.

## Scope

10 files, ~32 sites + 1 sub-fix in `controller/shader.rs` + 1 doctrine
docstring + 1 regression test. One commit per the user's scope answer.
