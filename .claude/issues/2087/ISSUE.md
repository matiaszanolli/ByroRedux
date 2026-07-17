# FO3-D6-01: Closed #1219's "no retail FO3 NIF ships at v20.0.0.4" premise is empirically false — sample data now exists

- **Severity**: LOW
- **Labels**: low, nif-parser, documentation
- **Location**: `crates/nif/src/version.rs:429-466` (`NifVariant::detect`, the #1219 one-shot warn)
- **Related**: #1219 (CLOSED)

## Description
#1219 shipped a one-shot warning for the ambiguous `(V20_0_0_4, uv=11, uv2=11)` header tuple (nif.xml lists v20.0.0.4 as "Oblivion, Fallout 3"; the parser routes it to `Oblivion`), on the premise that "vanilla content never hits it." Running `nif_stats` against the real `Fallout - Meshes.bsa` fires this warning exactly once. A header-only scan isolated the trigger: `meshes\triggers\collisionboxstatic.nif` — a real, vanilla, unmodified FO3 archive entry — ships at that exact tuple. The "never hits it" claim is false; sample data the closed issue asked for now exists.

## Evidence
`NifVariant::detect` (`version.rs:453-466`) contains a one-shot `std::sync::Once`-gated `log::warn!` for `version == NifVersion::V20_0_0_4 && user_version == 11`, with the comment directly above (lines 447-452) stating verbatim: "Vanilla content never hits it; the warn is silent on every supported game today." This is precisely the premise now disproven by the real `collisionboxstatic.nif` sample.

## Impact
None today. `NifVariant` has exactly one production consumer, `havok_scale_for`, which maps `Oblivion` and `Fallout3` to the identical 7.0 Havok-to-engine scale — misrouting this one file is unobservable. The file is also a generic invisible trigger-volume utility mesh, not player-visible geometry, plausibly a literal Oblivion-era asset carried over unchanged.

## Suggested Fix
No code change needed (impact provably zero). Update the #1219 docstring/comment to record the sample file (`meshes\triggers\collisionboxstatic.nif`) and correct the now-false "vanilla content never hits it" claim, since `NifVariant` invites more future consumers per its own doc comment.

## Completeness Checks
- [ ] **TESTS**: Add a regression note/test capturing the known-ambiguous sample file so a future `havok_scale_for` divergence between Oblivion/Fallout3 doesn't silently misroute it
