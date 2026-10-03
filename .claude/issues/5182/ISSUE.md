# #5182: EXT-D5-2026-10-02-02: The Oblivion row of the #4910 table settles a frame without evidence; by the doc's own convention the un-rotated read mirrors north and south

**Labels**: low,terrain-exterior,water,documentation,doc-rot,game:oblivion
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `docs/engine/watal.md:481` (the Oblivion row: "**un-rotated**", not OPEN).
  - `byroredux/src/env_translate.rs:592-595` and `:608`.
  - The test comment at `env_translate.rs:3230-3232`.
  - The parser derives the angle at `crates/plugin/src/esm/records/misc/water.rs:600-606`.
- **Status**: NEW. Related to #5136 and #4910.
- **Tier Violated**: no-fabrication
- **Game Affected**: Oblivion
- **Description**:
  - The parser stores Oblivion's layer 0 as `atan2(y, x)` of the `DATA[28]/[32]` scroll pair, and the docs call it a counter-clockwise direction-of-travel angle in the record frame. The un-rotated read then places `(cos θ, sin θ)` straight into engine XZ.
  - The same doc's Z-up→Y-up map (`watal.md:405-406`: "(x, y, z) maps to Y-up (x, z, −y)", "+Z is game south") sends a record-frame pair (x, y) to engine (x, −y), so φ = −θ.
  - So if the pair is a world-frame Z-up vector, the un-rotated read is a north/south mirror. If it is a UV-space vector, its relation to world depends on Oblivion's water UV mapping, which nobody has established.
  - Either way, "the angle is not a bearing" only shows that the +90° bearing formula does not apply. It does not show that φ = θ.
  - The commit message itself says the un-rotated read is "the pre-2026-09-24 status quo, not a frame claim". The table nevertheless lists FO3/FNV and Starfield as OPEN and Oblivion as settled.
  - #4910 also silently changed the frame of Oblivion's `wind_direction` (`DATA[4]`, authored per #4931), which feeds the physics-current fallback at `env_translate.rs:1001-1003`. It went from +90° to un-rotated, and neither table mentions it.
- **Evidence**: Census of `Oblivion.esm`, 23 WATR:
  - The scroll pairs are `(0.0011, 0.0011)` (DefaultWater family, θ = 45°), `(0.001, 0.002)` (dungeon/sewer, θ = 63°), `(0.0008, 0.0008)` (SwampWater), and zero on the rest.
  - Read as world-frame vectors, they would drift NE/NNE in game terms. The un-rotated read drifts them SE/SSE.
  - No Oblivion WATR has a directional kind: no river/stream/creek/rapid name, no NAM5, no NAM0. So the `wind_direction` physics-fallback change is latent in vanilla.
- **Impact**: The pattern drift direction on every Oblivion water may be mirrored, though the visual effect on near-isotropic slow scrolls is small. The spec presents an unverified frame as resolved, which is the same defect class as #5136.
- **Suggested Fix**: Mark the Oblivion row "un-rotated, OPEN", as FO3/FNV are. State that a world-frame reading would need φ = −θ, and add a row for Oblivion's `wind_direction`. Settle the question with a capture, or from Oblivion's water shader UV convention, before choosing a frame.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it
