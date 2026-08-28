# Issue #3396 — SF-2026-08-27-D6-01: shader.rs cites nif.xml as gating the BSEffect FO76 tail on #GTE# 155; the actual token is #EQ# 155

Filed: 2026-08-27 by `/audit-publish` from `docs/audits/AUDIT_STARFIELD_2026-08-27.md`

Labels: `low,documentation,doc-rot,nif-parser,nif,game:starfield,legacy-compat`

> Immutable snapshot of the issue as filed (TD10-001 / #1156).
> GitHub is authoritative for current state: `gh issue view 3396 --json state`.

---

Found by `/audit-starfield` — [`docs/audits/AUDIT_STARFIELD_2026-08-27.md`](docs/audits/AUDIT_STARFIELD_2026-08-27.md), Dimension 6 (NIF shader blocks, BSVER 155+).

- **Severity**: LOW (doc-rot — the *citation* is wrong, the *gate* is probably right)
- **Location**: `crates/nif/src/blocks/shader.rs:1825` (`refraction_power`) and `:1869` (the reflectance / lighting / emittance / emit-gradient / luminance block); field docstrings at `:1675-1683`
- **Status**: NEW

## Description

Sites in `BSEffectShaderProperty::parse_inner` justify a `bsver >= FO76` (i.e. Starfield-inclusive) gate by asserting *"nif.xml gates this on `BSVER #GTE# 155`"*.

That assertion is false. nif.xml gates every one of these fields on the `#BS_F76#` token, and **both** copies in the tree define that token as an **equality**:

```xml
docs/legacy/nif.xml:29
/mnt/data/src/reference/nifxml/nif.xml:29
    <verexpr token="#BS_F76#" string="(#BSVER# #EQ# 155)">Fallout 76 stream 155 only.</verexpr>
```

The fields carrying `vercond="#BS_F76#"` in `<niobject name="BSEffectShaderProperty">` are exactly `Refraction Power`, `Reflectance Texture`, `Lighting Texture`, `Emittance Color`, `Emit Gradient Texture`, `Luminance` — the six the parser reads on Starfield.

The claim originates in `cf9d3480` ("Fix #746 + #747"), whose commit body states the premise verbatim and widened seven sites mechanically on that basis. **Four of those seven have since been re-narrowed for Starfield on corpus evidence** — #1510 rolled back the BLSP translucency/texture-array tail (`shader.rs:1258`) and wetness `unknown_2` (`:1367-1372`); #2622 rolled back wetness `metalness`/`unknown_1` (`:1359-1363`). The two `BSEffectShaderProperty` sites are the ones nobody re-examined, and they still carry the falsified premise as their stated authority.

## Evidence

The premise is additionally contradicted *inside the same file*. The struct docstrings still describe the FO76-only reality that nif.xml actually states, while the code four hundred lines below reads them on Starfield:

```rust
// shader.rs:1675-1683 — docstrings
/// FO76 reflectance texture (BSVER == 155).
pub reflectance_texture: String,
/// FO76 lighting texture (BSVER == 155).
pub lighting_texture: String,
/// FO76 emittance color (BSVER == 155).
pub emittance_color: [f32; 3],
/// FO76 emit gradient texture (BSVER == 155).
pub emit_gradient_texture: String,
/// FO76 luminance params (BSVER == 155).
pub luminance: Option<LuminanceParams>,
```

```rust
// shader.rs:1877 — the code
if bsver >= crate::version::bsver::FO76 {
    reflectance_texture = stream.read_sized_string()?;
    ...
```

#746's two regression tests do not settle it either: both build an **FO76 field body under a Starfield header**, so they assume the conclusion rather than testing it against retail bytes. No corpus evidence was cited for the BSEffect widening (contrast #2622, which cites 4,417 real blocks for the sibling BLSP luminance quad).

### Disproof attempted — the code could not be disproved, only the citation

If the six fields were absent on Starfield, the parser would over-read ~44 B (4 + 3 empty sized strings @ 4 B + 12 + 16) on every block. The observed drift is the opposite sign — `shader.rs:1688` records *"Every retail-Starfield `BSEffectShaderProperty` carries a ~32-byte undocumented tail **beyond** the FO76 fields"* — and 89,276 NIFs parse with zero `BSEffectShaderProperty` failures despite three misalignment-sensitive `read_sized_string()` calls whose length prefixes are bounds-checked (`crates/nif/src/stream.rs:225-230`, `check_alloc`).

That is strong evidence the fields really are present on Starfield. **This issue is therefore filed against the citation, not the gate** — do not "fix" it by narrowing the gate to `== 155`.

## Impact

No runtime misbehaviour demonstrated. The cost is epistemic and concentrated exactly where it hurts: the scoped-out "+32 B BSEffect under-read" follow-up is the next person's job, and the first thing they will read is a comment telling them nif.xml already blesses the Starfield-inclusive gate. It does not. The same sentence has already produced four reverted changes in this file. #2625 (opaque-tail capture disables drift telemetry) compounds it — the 32 B is now silently absorbed into `starfield_tail`, so nothing will contradict the comment automatically.

## Suggested Fix

Replace the "nif.xml gates this on `BSVER #GTE# 155`" comments with the truth — *nif.xml's `#BS_F76#` is `#EQ# 155` and does not document Starfield; the Starfield-inclusive gate rests on corpus evidence (89,276 NIFs parse clean with three misalignment-sensitive sized-string reads), not on the spec* — and bring the five `BSVER == 155` field docstrings at `:1675-1683` into agreement with the code.

## Related

#746 / #747 (`cf9d3480`, origin of the premise), #1510, #2616, #2622 (three rollbacks of the same premise), #2625 (telemetry suppression), #3364 (sibling `BSShaderType155` / Starfield conflation), #1881 (the BSEffect tail capture).

**Highest-value follow-up measurement** (not done this pass): a `starfield_tail` length census across the corpus for both BSEffect (~32 B) and BLSP (38 B) — both figures are currently quoted from in-tree docstrings, not independently measured. That census would empirically settle the underlying gate rather than just correcting its citation.

## Completeness Checks
- [ ] **SIBLING**: check the other `cf9d3480`-widened sites for the same stale citation (three were already re-narrowed by #1510 / #2622)
- [ ] **TESTS**: if the gate itself is ever revisited, the regression test must use retail bytes rather than an FO76 body under a Starfield header
