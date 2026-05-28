# Issue #1279: Split BSLightingShaderProperty::parse into per-variant dispatch (#1277 Task 2)

**State**: OPEN
**Labels**: enhancement, nif-parser, high

## Body

**Child of #1277 — deferred starter task.**

Split `BSLightingShaderProperty::parse` ([crates/nif/src/blocks/shader.rs:778](../blob/main/crates/nif/src/blocks/shader.rs#L778)) into per-variant dispatch. The current monolith has **12+ embedded BSVER comparisons** spanning Skyrim LE/SE, FO4, and FO76/Starfield in a single 100-line `parse` body. It's the single most complex parser in the NIF crate per the per-game survey ([docs/engine/per-game-translation-survey.md §4.1](../blob/main/docs/engine/per-game-translation-survey.md#41-nif-parser-cratesnifsrcblocks-versionrs)).

## Why this was deferred from the starter-task batch

A wrong-arm shipped write would still parse 34,995 FO4 vanilla NIFs to "100%" but produce wrong material values downstream. Verifying a bit-for-bit equivalent refactor needs real-data testing across all four BSLSP-shipping games. That budget didn't fit in the 6-task starter batch.

## Recommended 5-step approach (from #1277 survey §9)

1. Add per-variant `BsLightingShaderRaw::parse_skyrim`, `parse_fo4`, `parse_fo76_plus` returning the same `BsLightingShaderRaw` shape.
2. Top-level `BSLightingShaderProperty::parse` dispatches on `NifVariant` (with the same `Unknown`-low-bsver caveat as #1277 Task 5 — `ni_node_parses_unknown_variant_with_low_bsver` is the regression guard pattern).
3. Each per-variant parser is bit-for-bit equivalent to the corresponding slice of the current monolith — no logic change.
4. Verify against `cargo test -p byroredux-nif --test parse_real_nifs -- --ignored`. All four BSLSP-shipping games must still pass 100% recoverable:
   - `BYROREDUX_SKYRIMSE_DATA`
   - `BYROREDUX_FO4_DATA`
   - `BYROREDUX_FO76_DATA`
   - `BYROREDUX_STARFIELD_DATA`
5. Cross-check against the **Task 8 translation-completeness harness** (`cargo test -p byroredux-nif --test translation_completeness -- --ignored --nocapture`) — the per-game `m_kind%` / `metO%` / `nrm%` fill-rates printed in the comparison table will surface any unintended drift at per-game granularity. Baseline from this session:

   ```
   Oblivion     m_kind=  0.0%  tex= 99.1%  nrm=  0.2%
   FO3          m_kind=  5.6%  tex= 94.2%  nrm= 92.1%
   FNV          m_kind=  9.6%  tex= 94.4%  nrm= 89.1%
   SkyrimSE     m_kind= 27.4%  tex= 99.0%  nrm= 94.6%
   FO4          m_kind= 38.3%  tex= 74.3%  nrm= 70.1%
   ```

## Threshold inventory (from survey §4.1)

The 12+ BSVER comparisons currently in the parse method, grouped by per-variant slice:

| Slice | Branch | Bytes |
|---|---|---|
| Skyrim LE/SE | Legacy shader type (BSVER 83–139, before `name`) | 4 |
| Skyrim LE/SE/FO4 | shader_flags_1/2 u32 pair (BSVER ≤ 130) | 8 |
| FO76+ | shader type (BSVER >= 155, typed as `BSShaderType155`) | 4 |
| FO4+ (BSVER >= 132) | CRC32 num_sf1 + sf1 array | 4 + 4N |
| FO76+ (BSVER >= 152) | CRC32 num_sf2 + sf2 array | 4 + 4M |
| FO76+ | material-reference stopcond on BGSM/BGEM/.mat suffix | 0 (early-return) |
| Skyrim (BSVER < 130) | lighting-effects block | … |
| FO4 (BSVER 130–139) | subsurface_color / rolloff / rimlight / backlight | … |
| FO4+ (BSVER >= 130) | grayscale_to_palette_scale / fresnel_power | … |
| FO4+ (BSVER >= 130) | wetness block (with internal BSVER == 130 gate for `unknown_1`) | … |
| FO4+ (BSVER >= 130) | `root_material_path` (NiFixedString) | varies |

The splits map cleanly to three per-variant parsers; the per-bsver inner gates within each (e.g. FO76+ SF2-arrays-only-on-BSVER>=152) stay inline in the matching variant body.

## Definition of done

- [ ] Three per-variant parsers exist as private helpers in `crates/nif/src/blocks/shader.rs`.
- [ ] Top-level `parse()` is a thin dispatcher on `stream.variant()` with the documented `Unknown`-low-bsver caveat handled.
- [ ] `cargo test -p byroredux-nif --lib` green.
- [ ] `cargo test -p byroredux-nif --test parse_real_nifs -- --ignored` 100% recoverable on Skyrim SE / FO4 / FO76 / Starfield.
- [ ] `cargo test -p byroredux-nif --test translation_completeness -- --ignored --nocapture` produces a per-game `m_kind%` table within ±2pp of the baseline above for the four BSLSP-shipping games.

## References

- Parent epic: #1277
- Survey: [docs/engine/per-game-translation-survey.md §4.1 + §9](../blob/main/docs/engine/per-game-translation-survey.md)
- Task 5 (Unknown-low-bsver pattern): commit `2bd447d5`
- Task 8 (verification harness): commit `294e68f1`
