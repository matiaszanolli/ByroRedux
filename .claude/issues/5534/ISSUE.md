# #5534: SF-2026-10-09-D4-03: `game-compatibility.md` and `lighting-from-cells.md` still describe Starfield's 108-byte XCLL as "the Skyrim 92-byte layout plus a 16-byte tail" that is ignored

**Labels**: doc-rot, documentation, game:starfield, legacy-compat, low

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-09.md` — finding `SF-2026-10-09-D4-03` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (doc rot)
- **Dimension**: ESM Resolve Rate + Cell Bring-up
- **Location**: `docs/engine/game-compatibility.md:284-286`; `docs/engine/lighting-from-cells.md:139-142`
- **Status**: NEW. #2364 (closed) fixed only a test assertion message, and #1293 (closed) shipped the decode. Neither touched these two docs.
- **Description**: The code decodes Starfield's own layout:
  - `crates/plugin/src/esm/cell/walkers.rs:36-40`: "NOT 'Skyrim + 16-byte tail' — Starfield's XCLL shares only bytes 0-39 with Skyrim and then diverges into a distinct volumetric height-fog model";
  - `parse_cell_starfield_xcll_decodes_volumetric_height_fog_tail`;
  - #5002's LGTM twin.

  The two docs still say the dispatch "reads the Skyrim 92-byte prefix and ignores the trailing 16 bytes for now". The skill records this framing as stale.
- **Impact**: A reader is told Starfield cells carry Skyrim's ambient cube / specular / fresnel and drop the height fog, which is the opposite of the truth.
- **Related**: #1291, #1293, #5002, #5346 (the two SF 108-byte decoders).
- **Suggested Fix**: Rewrite both passages to the SF1 layout (bytes 0-39 shared; 40-107 height-fog model) and cite `walkers.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
