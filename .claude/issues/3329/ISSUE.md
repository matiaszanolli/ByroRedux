# FNV-2026-08-26-D6-03

**Issue**: #3329
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 6 — Animation, Skinning & Particles
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/src/import/walk/mod.rs:900-931` (`extract_emitter_rate`)

**Premise verified**: the chain is controller → `interpolator_ref` →
(a) `NiFloatInterpolator` keyed/const, (b) `resolve_blend_interpolator_target`
dominant sub-interpolator (#2548), (c) the blend block's own `value`. For the
manager-controlled shape — `NiBlendInterpolator.items` empty, which
`resolve_blend_interpolator_target` deliberately returns `None` for
(`anim/controlled_block.rs:105-110`) — (b) fails and (c) sees a non-positive
`value`, so `extract_emitter_rate` returns `None` and
`apply_emitter_overlays` (`systems/particle.rs:82`) leaves `preset.rate` at
the heuristic value from `fog.rs::particle_preset`. #2548 (CLOSED) fixed the
*non-empty* blend case only; this is its residual.

**Evidence**:
```
$ ... --example _tmp_fo3_emitter_survey -- "Fallout - Meshes.bsa"
files=14881 with_emitter=307 params_some=307 rate_some=100
ctlr_files=270 ctlrdata_files=0 growfade_files=237 growfade_with_base_scale=237

$ ... --example _tmp_fo3_rate_probe -- "Fallout - Meshes.bsa"
NiPSysEmitterCtlr.interpolator_ref targets: {"NiBlendFloatInterpolator": 997, "NiFloatInterpolator": 207} (null refs: 0)

$ ... --example _tmp_fnv_d6_rate_cause -- "Fallout - Meshes.bsa"
{ "OK float const": 94, "OK float keyed": 6,
  "blend: no sub + zero/neg value": 168, "float: first key == 0.0": 2 }
blend.items length histogram: {0: 168}          ← every one is the empty-items form
```
The authored rate is *present in the file* and recoverable: walking the
embedded `NiControllerSequence` controlled-blocks for a `*EmitterCtlr` type
and reading its `NiFloatInterpolator` recovers **155 of the 168**:
```
$ ... --example _tmp_fnv_d6_seq_rate -- "Fallout - Meshes.bsa"
affected(manager-blend, rate None)=168 recoverable_from_sequences=155 files_with_no_sequence=0
  meshes\effects\ambient\fxambdust04.nif:              seq 'SpecialIdle' rate=25
  meshes\clutter\snowglobes\snowglobes_nelis.nif:      seq 'Idle'        rate=6.0
  meshes\dungeons\vault\roomu\vgeardoor01.nif:         seq 'Open'        rate=30
  meshes\dlc03\effects\crawlerfx\dlc03crawlerdustexplosion.nif: seq 'Forward' rate=300
  meshes\effects\fxhelios_charging.nif:                seq 'OFF'         rate=150
  meshes\dlc04\effects\dlc04fxcrashthroughfloor.nif:   seq 'Forward'     rate=510
```

**Impact**: 168 of the 307 emitter-bearing FNV meshes (55%), and 62% of those
with a controller, run at a *guessed* density. Presets are `torch_flame` 35/s,
`explosion` 96/s, `smoke`/`magic` (`particle.rs` presets), against authored
rates of 6/s (snowglobes — ~6× too dense) through 510/s
(`dlc04fxcrashthroughfloor` — ~15× too sparse). The affected list is dominated
by exactly the ambient FX the player stands in front of: `fxambdust*`,
`fxdrippingsewage/water/blood*`, Lucky 38 reactor, the Strip fountain
(`ul_fountainnewfx`), Helios One steam, snowglobes. Note the *other* authored
fields survive — `params_some=307/307` and `growfade_with_base_scale=237/237`,
so size/speed/life/`base_scale` are correct; **only density is guessed.**

**Fix sketch**: add a fourth fallback tier to `extract_emitter_rate` — when the
interpolator resolves to a blend block with empty `items`, scan the scene's
`NiControllerSequence.controlled_blocks` for a block whose resolved
`controller_type` contains `EmitterCtlr` and run its `interpolator_ref`
through the existing `float_interpolator_rate`. Preferring a sequence named
`Idle`/`SpecialIdle` (the steady-state loop) over transient ones would avoid
picking an ignition ramp.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
