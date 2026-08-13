# Batch: #2588 #2589 #2590 #2591

## #2588 — SK-D6-03: BSTreeNode wind-bone lists are imported but have no consumer outside the NIF crate

**Severity**: LOW · **Status**: NEW (informational — forward scope)
**Location**: `crates/nif/src/import/walk/mod.rs:1589-1600`; `import/types.rs:161`

`BSTreeNode`'s two trailing `NiNode` ref lists (SpeedTree wind rig) are
parsed correctly and surfaced onto `ImportedNode.tree_bones` by both
walkers, but nothing outside `crates/nif`/`crates/spt` reads the field.

Impact: None today (Skyrim trees render static). Recorded so the
parse-vs-consume gap is on record rather than rediscovered as "the
parser drops it."

Suggested Fix: None required now — ready hook for when SpeedTree wind
lands.

Completeness Checks: TESTS N/A — forward-scope note, no action required.

---

## #2589 — SKY-D7-01: Skyrim's parser arm zeroes two FO4-only BSLSP scalars, and the importer copies them un-gated -- canonical Material.fresnel_power is 0.0 instead of 5.0

**Severity**: MEDIUM
**Location**: producer `crates/nif/src/blocks/shader.rs:938-939` (`parse_skyrim`); un-gated copy `dedicated_shader.rs:321-322`; neutral defaults `import/material/mod.rs:1033-1034`, `import/types.rs:562,565`, `crates/core/src/ecs/components/material.rs:408`; boundary `byroredux/src/material_translate.rs:200`

`grayscale_to_palette_scale`/`fresnel_power` are FO4+ wire fields
(BSVER ≥ 130); every default site in the pipeline agrees on the
neutral fallback (`1.0`/`5.0`) **except** `parse_skyrim`, which
constructs the block with literal `0.0`/`0.0` for fields Skyrim never
serializes. `apply_bs_lighting_shader` copies both unconditionally
with no BSVER gate, so the Skyrim-arm `0.0` survives
`into_imported_material` and lands in canonical
`Material.fresnel_power = 0.0` for essentially all lit Skyrim
geometry — while Oblivion/FO3/FNV (no BSLSP) keep `5.0` and FO4+ get
their authored value.

Evidence: `shader.rs:938-939` — `grayscale_to_palette_scale: 0.0,
fresnel_power: 0.0,` inside `parse_skyrim`; `dedicated_shader.rs:321-322`
— unconditional copy. The guard test
`material_info_default_matches_bslsp_parser_stub_defaults` only
compares `MaterialInfo::default()` against the FO76+ stopcond stub,
never against `parse_skyrim`.

Impact: Latent today (no GPU consumer for `fresnel_power` yet). The
moment a `triangle.frag` consumer lands (#2284 follow-up), Skyrim gets
Schlick exponent 0.0 → full Fresnel at every angle, uniform
edge-bright/washed shading vs correct FO4 rendering.

Related: #2284, #1241, SKY-D7-02.

Suggested Fix: Make `parse_skyrim` construct both fields with the same
neutral literals every other default site uses (`1.0`/`5.0`).
Extend the guard test to assert the invariant against all three parser
arms.

---

## #2590 — SKY-D7-02: MaterialInfo default docs cite a BSLSP parser stub default that the Skyrim parser arm contradicts, at line numbers stale since the #1279 parser split

**Severity**: LOW
**Location**: `import/material/mod.rs:588-598,1029-1031`; `lighting_shader_pbr_tests.rs:205-209`

Three sites anchor the neutral-default doc to specific `shader.rs`
line numbers that, since the #1279 three-arm parser split, land in
unrelated code (the `starfield_tail` doc, not the stub). The docs also
assert a single "parser stub default" exists when there are two
disagreeing ones (`material_reference_stub` = `1.0/5.0`,
`parse_skyrim` = `0.0/0.0`).

Impact: A reader following these anchors lands in unrelated code and
concludes the default contract is upheld — the documentation half of
why SKY-D7-01 went unnoticed.

Related: SKY-D7-01.

Suggested Fix: Anchor to the function name, not a line number; state
plainly which parser arms honour the neutral default.

---

## #2591 — SKY-D7-03: EmissiveSource::None's documented contract is contradicted by the unconditional Lighting tag -- on Skyrim the discriminator degenerates to "has a BSLightingShaderProperty"

**Severity**: LOW
**Location**: contract `crates/core/src/ecs/components/material.rs:452-457`; Skyrim set-site `dedicated_shader.rs:298-300`

`apply_bs_lighting_shader` sets `EmissiveSource::Lighting`
unconditionally regardless of whether `emissive_color`/
`emissive_multiple` are actually non-zero. Vanilla Skyrim ships the
overwhelming majority of BSLSP blocks with an unauthored `[0,0,0]`/
`1.0` emissive, all tagged `Lighting` anyway — the discriminator
carries no emissive-authoring information on Skyrim, contrary to its
own doc's parenthetical.

Evidence: `dedicated_shader.rs:298-300` — set with no check on the
emissive values.

Impact: None at runtime today (no `GpuMaterial` field, no shader
branch reads it yet). Cost is that the #1280 discriminator doesn't yet
answer the question its doc promises.

Related: #1280, #166.

Suggested Fix: Either amend the doc to describe actual behavior, or
gate the three set-sites on a non-zero emissive contribution.

Completeness Checks: If gated, a regression test confirms
`EmissiveSource::Lighting` is only set for non-zero emissive
authoring.
