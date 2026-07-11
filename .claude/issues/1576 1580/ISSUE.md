# #1576: SF-D4-03: Model-less STAT/BNDS/ACTI/ARMO Starfield forms drop because geometry lives in a BFCB component block

Severity: LOW · Dimension: SF ESM Resolve-Rate
Location: `crates/plugin/src/esm/cell/support.rs:38-160`
Source: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D4-03)

STAT/BNDS/ACTI/ARMO forms with no top-level `MODL` may carry a model ref inside
a `BFCB`-wrapped `TESModel_Component`. Blocked on the same reflection-derived
component schema gap as sibling #1567 (SF-D4-01) — already investigated twice
in prior sessions (see issue comments), left open both times with findings
documented in `support.rs`. No new schema material has surfaced; #1567 closed
via an unrelated flat-`DAT2` fix, not a BFCB walker, so this is still blocked.

# #1580: SF-D9-02: BGEM grayscale_to_palette_alpha bool parsed but not forwarded

Severity: LOW · Dimension: BGSM/BGEM External Flow
Location: `crates/bgsm/src/bgem.rs:49` (parsed) / `byroredux/src/asset_provider.rs:1399-1501` (not forwarded)
Source: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D9-02)

BGEM parses `grayscale_to_palette_alpha: bool` but the merge arm in
`asset_provider.rs` only forwards the LUT texture (→ `EFFECT_PALETTE_COLOR`),
never the alpha-variant bool, so `EFFECT_PALETTE_ALPHA` is set only from the
inline `BSEffectShaderProperty` SLSF1 bit, never from the `.bgem` file itself.

Suggested fix: in the BGEM merge arm, when `bgem.grayscale_to_palette_alpha`,
OR in `EFFECT_PALETTE_ALPHA` symmetrically with how `EFFECT_PALETTE_COLOR` is
already forwarded.
