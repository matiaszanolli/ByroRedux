# NIF-D2-2026-09-21-01: #4156's CInfo2014 branch is gated bsver >= FALLOUT4, but nif.xml uses it only at #BS_FO4# (== 130), and the code doc claims 'derived purely from nif.xml'

**Issue**: #4627
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: LOW
**Dimension**: 2 (Version Gating)
**Location**: `crates/nif/src/blocks/collision/rigid_body.rs:67-69, 300-331`
**Status**: NEW

## Description
#4156's CInfo2014 branch is gated `bsver >= FALLOUT4` (`rigid_body.rs:66-68`: `if bsver >= crate::version::bsver::FALLOUT4 { return Self::parse_fo4_cinfo2014(stream); }`), but per the report's nif.xml cross-check the CInfo2014 layout is specified only at `#BS_FO4#` (== 130) — a `>=` gate would silently apply the same field order to any future bsver > 130 title without re-verification, while the code's own doc comment (`rigid_body.rs:300-303`) claims the layout is "derived purely from nif.xml", implying exact-version fidelity it doesn't actually have.

Confirmed at HEAD `ee6d3fb39`: the gate is unchanged (`bsver >= crate::version::bsver::FALLOUT4`).

## Evidence
- At bsver 130 the layout is corpus-confirmed: 6 third-party blocks decode nif.xml's defaults with zero drift, where nifly's 39-byte FO4 tail would have drifted 13 bytes.
- No content exists anywhere at bsver > 130 today (FO76 is 155, Starfield 172+, both already covered by the same `>=` gate and both corpus-clean), so this is a forward-compatibility precision gap, not a currently observable bug.

## Impact
None today — every title currently reaching this gate (FO4 130, FO76 155, Starfield 172+) is corpus-verified clean. Risk is confined to a hypothetical future bsver in (130, 155) that this audit has no evidence exists.

## Related
#4156 (closed — introduced the CInfo2014 decoder and this gate)

## Suggested Fix
Either narrow the comment to state the gate is `>=` by design (covering FO4/FO76/Starfield as one band, corpus-verified for all three), or split into explicit per-version arms if a future title's layout is found to diverge. This needs a decision, not urgent code change — no shipped content is affected.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D2-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: If the gate changes, corpus regression coverage for FO4/FO76/Starfield stays green
