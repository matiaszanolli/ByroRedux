# Batch: #2089 #2090 #2091 #2092 — Oblivion/FO4 audit LOWs

All four LOW, all OPEN. Premises verified against current code first
(line numbers in the issues predate the Session-34 `actor.rs` → `actor/mod.rs`
split and the walker → `dedicated_shader.rs` move).

## #2089 — DIM3-OBL-02 (LOW): `flags_oblivion` parsed but no consumer
- **Verdict**: premise current — only field decl / init / parse + tests read it.
- **Action**: not a bug (deliberate forward-sequencing for CHARAL). Added a
  doc note on the field (`actor/mod.rs`) flagging it as intentional and
  naming CHARAL's Oblivion class-flag pass as the future consumer, so it
  doesn't get rediscovered as a "surprise" gap. This is exactly the
  suggested fix.

## #2090 — OBL-D7-01 (LOW, doc): `legacy_particle.rs` overclaims Oblivion dep
- **Verdict**: premise current — module doc still asserted "Oblivion is
  v20.0.0.5 and still serializes them"; the per-block baseline shows only
  `NiParticleSystem 547 0` (modern), no legacy-stack rows.
- **Action**: rewrote the module doc to state the parsers are
  nif.xml-completeness / defensive coverage, cite the baseline evidence
  and #1327's dead-arm removal, and explicitly warn against re-deriving a
  "dropped Oblivion particle FX" finding.
- **SIBLING**: swept block docs for the same "still ships on game X"
  framing — none (`extra_data.rs:230`'s "subclass still serializes its
  payload" is a factual serialization note, not a game-dependency claim).

## #2091 — FO4-D5-01 residual (LOW, bug): shader-flag alpha-test inert when a non-test NiAlphaProperty was consumed
- **Verdict**: premise current and real. `dedicated_shader.rs` FO4
  `ALPHA_TEST` arm seeded `128/255` only `if !info.alpha_property_consumed`.
  A blend-only/opaque `NiAlphaProperty` runs `apply_alpha_flags`, which
  sets `alpha_property_consumed = true` but leaves `alpha_threshold = 0.0`
  → `alpha_test=true, alpha_threshold=0.0` → `triangle.frag` discard gate
  (`> 0.0`) inert.
- **Action**: changed the guard to seed whenever `alpha_threshold == 0.0`
  (a property that authored a real test threshold already has `> 0.0`, so
  authored intent per #1201/#1202 is never overridden).
- **SIBLING**: FO76+ is a no-op (typed flag word zero on BSVER >= 132);
  the other `!alpha_property_consumed` guards (`dedicated_shader.rs:488`,
  `legacy_properties.rs:65`) are implicit-*blend* gates where `consumed`
  is the correct signal — not the same bug.
- **TESTS**: added `fo4_alpha_test_flag_seeds_threshold_over_blend_only_alpha_property`
  to `fo4_shader_flag_tests.rs` (blend-only NiAlphaProperty + Alpha_Test flag
  → threshold 128/255).

## #2092 — SK-D2-01 (LOW): FO4 Skin Tint alpha parsed then discarded
- **Verdict**: **already fixed** (stale premise). `ShaderTypeData::SkinTint`
  now carries `skin_tint_alpha: Option<f32>`; the FO4 type-5 arm
  (`shader.rs:1433`) reads it as `Some(..)` on BSVER 130–139 (was
  `let _skin_tint_alpha`); `MaterialInfo.skin_tint_alpha` exists
  (`material/mod.rs:660`) and is populated (`mod.rs:1105`,
  `shader_data.rs:129`). Round-trip tests already assert `Some(0.25)`
  reaches MaterialInfo (`shader_type_fields_tests.rs:217`,
  `shader_type_data_tests.rs:174`). No code change; closed with evidence.

## Verification
- `cargo test -q -p byroredux-nif -p byroredux-plugin` + full workspace — all green.
- clippy clean on both touched crates. No shader/GLSL change → no SPIR-V recompile.
