# SKY-D2-01: Import-side decal docs still name Skyrim SLSF2 bit 21 Cloud_LOD, and ~10 comments cite shader.rs:LINE into the split blocks/shader/ directory

Labels: low, nif-parser, nif, documentation, doc-rot, game:skyrim, legacy-compat
Source: docs/audits/AUDIT_SKYRIM_2026-10-27.md

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-27.md` finding `SKY-D2-01` (Dimension 2, shader-type dispatch) · Severity LOW (doc-rot) · Status NEW (residual of closed #3545 and #6167) · Game: Skyrim.

**Location**: `crates/nif/src/import/material/mod.rs:103-105,:121-125,:150-153` (Cloud_LOD wording); `import/material/double_sided_tests.rs:272,:281`; `import/mesh/bs_tri_shape_shader_flag_tests.rs:278,:293`. Stale `shader.rs:LINE` cites: `import/material/mod.rs`, `dedicated_shader.rs:331,:454,:465`, `double_sided_tests.rs:330`, `lighting_shader_pbr_tests.rs:43`, `crates/nif/src/version.rs:522`, `blocks/shader_tests/fo76.rs:325`, `anim/channel.rs:523`, `anim/tests/transform.rs:357`, `import/material/legacy_properties.rs:595,626,695,722,800`. Gate: `crates/core/tests/stale_line_references.rs:104`.

## Description
nif.xml `SkyrimShaderPropertyFlags2` defines bit 20 as `Cloud_LOD` and bit 21 as `Anisotropic_Lighting`. `shader_flags.rs` now says that correctly. The import tier still says "flags2 bit 21 (`Cloud_LOD` on Skyrim, `Anisotropic_Lighting` on FO4 …)", so the crate contradicts itself.

Separately, `crates/nif/src/blocks/shader.rs` no longer exists (split into `blocks/shader/`), but about ten comments cite line numbers in it. `stale_line_references.rs:104` skips those cites, because the basename `shader.rs` matches two live files (`blocks/controller/shader.rs`, `blocks/dispatch_tests/shader.rs`) and `Some([target])` only accepts a unique match. TD3-2026-10-27-01 covers zero-match basenames; this is the multi-match blind spot, and its sites are not in TD3's list.

## Evidence
```
crates/nif/src/import/material/double_sided_tests.rs:281: // Flag2 bit 21 — Cloud_LOD on Skyrim / Anisotropic_Lighting on
crates/nif/src/import/mesh/bs_tri_shape_shader_flag_tests.rs:293: "flags2 bit 21 is Cloud_LOD (Skyrim) / Anisotropic_Lighting (FO4), not a decal bit"
crates/nif/src/version.rs:522: ... See `shader.rs:943-957` for
crates/nif/src/import/material/lighting_shader_pbr_tests.rs:43: `crates/nif/src/blocks/shader.rs:679-695`
```
`ls crates/nif/src/blocks/shader.rs` fails (no such file).

## Impact
The code is correct; the modern helper no longer takes flags2. Readers of the decal helper docs are misled about Skyrim's bit map, and the line cites point nowhere.

## Related
#3545, #6167, TD3-2026-10-27-01.

## Suggested Fix
Say "bit 21 = `Anisotropic_Lighting` on Skyrim and FO4, `Alpha_Decal` on FO3/FNV". Replace each `shader.rs:N` with the symbol plus its `blocks/shader/<file>.rs`. Make the gate fail on an ambiguous basename instead of skipping it.

## Completeness Checks
- [ ] **SIBLING**: Every `shader.rs:N` and `Cloud_LOD` hit in `crates/` and `docs/` swept (grep), not only the listed sites
- [ ] **TESTS**: `stale_line_references.rs` fails on an ambiguous basename, with a fixture pinning it

