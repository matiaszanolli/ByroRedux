# #3754: FO3-2026-08-30-D2-01: authored ramp-up birth-rate curves are discarded whole — float_interpolator_rate reads only keys.first(), so 20 of 294 FO3 emitter meshes run on a name-heuristic guess

**Labels**: bug, nif-parser, medium, legacy-compat, game:fo3, nifal
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_FO3_2026-08-30.md` · **Severity**: MEDIUM · **Dimension**: 2 (NIF parser — typed particle emitters, NIFAL particle slice)
**Game affected**: Fallout 3 (measured); the code path is shared

Severity per the project severity table: *"Translatable block silently dropped by NIFAL (collision shape / particle emitter params) → at least MEDIUM"*.

## Location
- `crates/nif/src/import/walk/mod.rs` — `float_interpolator_rate` (currently `:936-949`), reached by **every** tier of `extract_emitter_rate` including `sequence_emitter_rate`

## Description
Every tier funnels through:

```rust
if let Some(first) = scene.get_as::<NiFloatData>(data_idx)
    .and_then(|d| d.keys.keys.first())
{
    if let Some(r) = sane(first.value) { return Some(r); }
}
sane(interp.value)
```

`sane()` rejects `0.0` by design (#1771 — a zero first key means "ramp-up", so don't latch it as a permanent zero). But the function then looks **no further into `d.keys.keys`**, and on this shape the interpolator's own `value` is the `-FLT_MAX` "use the keyed data" sentinel, which `sane()` also rejects.

The authored curve — **already in hand** — is dropped and the emitter falls through to `byroredux/src/fog.rs::particle_preset`, a name/texture keyword guess whose default arm is `ParticleEmitter::torch_flame()` at **35 /s**.

**Distinct from every closed sibling**: not #1771 (a *constant* 0.0 rate), not #3329 (the empty-`items` manager blend — which fires correctly here and still finds nothing), not #1402 (first-*emitter* match, not first-*key*).

## Evidence
Controlled blocks dumped from three affected FO3 meshes (`NiControllerSequence` → `ctype = "NiPSysEmitterCtlr"` → `NiFloatInterpolator{value = -3.4028235e38}` → `NiFloatData.keys`):

| mesh | authored key values | rate used today |
|---|---|---|
| `meshes\architecture\urban\tenpengate01.nif` (seq `Close`) | `[0.0, 0.0, 600.0, 0.0]` | 35 /s preset — **17× low** |
| `meshes\effects\ambient\fxfallingrocks01.nif` (seq `Idle`) | `[0.0, 0.0, 22.5, 0.0]`, `[0.0, 0.0, 300.0, 0.0]` | 35 /s preset — **8.6× low** |
| `meshes\effects\ambient\fxbubblestall01.nif` (seq `Idle`) | `[0.0, 0.0, 30.0, 30.0]`, `[0.0, 60.0, 60.0, 0.0]` | 35 /s preset |

**Blast radius (measured, whole FO3 corpus)**: 294 emitter-bearing meshes; 65 yield no rate; **20 of those carry an authored ramp-up curve with a positive peak**. Peak histogram `{10.5:1, 45:1, 60:7, 120:4, 150:1, 202.5:1, 300:2, 450:1, 600:1, 900:1}`. Affected content: Tenpenny Tower's gate FX, the Anchorage snow drifts and mesh tubes, the MQ11 vertibird effects, the whole force-field FX set, the falling-rock ambients. (The other 45 rate-less files genuinely author nothing: 43 have no emitter controller at all, 2 carry a bare `-FLT_MAX` interpolator with no keyed data.)

**Disproved alternative**: the first-controller-only `find_map` was the first suspect. Replicating the full tier chain per-controller over all 22 files with a controller: **0** would resolve on a later controller. Not the cause.

## Impact
**6.8 % of FO3's particle library spawns at an invented density.** No crash, no parse error — the emitters exist and animate, they simply run at the wrong rate. This is exactly the class of defect #3343's magnitude floors were added to catch, and they still miss it: those floors count `rate.is_some()`, and these files legitimately report `None`.

## Related
#1771, #3329, #3343, #1402 (all closed and all distinct from this); `docs/engine/nifal.md` particle slice.

## Suggested Fix
When `sane(first.value)` rejects a zero first key, scan the remaining keys for the maximum finite positive value and use that as the steady-state rate — it is the curve's plateau in every sampled case — instead of falling through to `sane(interp.value)` and then to the name preset. Extend `real_archive_torch_meshes_surface_particle_emitters` to assert a per-game *ratio* of rate-bearing emitters so a regression here turns the build red.

## Completeness Checks
- [ ] **SIBLING**: every tier of `extract_emitter_rate` funnels through this one function — confirm the fix reaches all of them, and check FNV/Oblivion corpora for the same shape
- [ ] **CANONICAL-BOUNDARY**: the rate must be resolved at the NIFAL parser→emitter-params boundary (`extract_emitter_rate` in `crates/nif/src/import/walk/mod.rs`), never re-derived at render time and never as a per-game branch. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pinning the Tenpenny gate's `[0,0,600,0]` curve to 600 /s, not 35 /s
