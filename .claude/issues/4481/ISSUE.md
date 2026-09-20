# EXT-D4-2026-09-19-02: WTHR cross-fade and promotion drop the HNAM dimmers — post-fade sun runs on the source weather's values

- **ID**: EXT-D4-2026-09-19-02
- **Labels**: high,terrain-exterior,bug,game:oblivion
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4481

**Severity**: HIGH · **Dimension**: Weather/sun · **Tier Violated**: single-boundary (EXAL-translated values dropped at the state seam) · **Game Affected**: Oblivion (per-WTHR HNAM block; latent 1.0 elsewhere)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D4-2026-09-19-02; orchestrator-verified)

**Location**: `byroredux/src/systems/weather.rs:798-808` (source dim), `:863-871` (target never dimmed), `:1188-1226` (`promote_weather_transition_target` — 11 fields copied, the two dimmers not); target construction `byroredux/src/scene/world_setup.rs:369-374`

**Description**
`translate_weather` correctly carries each WTHR's `sunlight_dimmer`/`grass_dimmer` (guarded by `sunlight_dimmer_translates_from_the_hnam_block`), and `weather_system` multiplies the *source* palette. But (a) the cross-fade's `target_sunlight` is sampled from `tr.target.sky_colors` without applying `tr.target.sunlight_dimmer`, so the fade eases from `src×d_src` to an undimmed target; (b) on completion `promote_weather_transition_target` copies sky/fog/TOD/wind/DALC/clouds but neither dimmer, so from the next frame the promoted target palette is multiplied by the *source's* `sunlight_dimmer` and `GroundCoverDimmer` reverts to the source's — permanently, until the next transition or worldspace reload.

**Evidence**
The only dimmer multiply is `weather.rs:805-807` against `wd` (source); the transition block contains no `tr.target.sunlight_dimmer`; `grep dimmer` in the promotion body (`:1188-1260`) → no hit.

**Impact**
Wrong exterior directional sun brightness and wrong ground-cover dimmer after every qualifying Oblivion weather change (dimmers are per-WTHR, so ordinary same-worldspace changes qualify); one-frame pop at fade end. Visual-only, no crash.

**Related**: EXT-D4-2026-09-19-03 (the unpinned multiply is what let this land)

**Suggested Fix**
Dim `target_sunlight` by `tr.target.sunlight_dimmer` in the transition block; add `sunlight_dimmer`/`grass_dimmer` to the promotion field copy; pin both with a dimmer ≠ 1.0 fixture.

## Completeness Checks
- [ ] **SIBLING**: Check every other field of `WeatherDataRes` for the same promotion omission (wind/DALC were fixed by #1101/#1102 — dimmers were missed)
- [ ] **TESTS**: A dimmer ≠ 1.0 cross-fade test pins both the fade curve and the post-promotion steady state
