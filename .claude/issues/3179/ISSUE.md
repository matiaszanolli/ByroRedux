# #3179 — AUD-2026-08-20-D1-01: the above-water state of the new per-track low-pass is not a bypass

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3179
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: MEDIUM
**Dimension**: Spatial Sub-Track Lifecycle & Leaks
**Source**: `docs/audits/AUDIT_AUDIO_2026-08-20.md` (`AUD-2026-08-20-D1-01`) — HEAD `bb0b92f2`

## Location

- `crates/audio/src/lib.rs` — `UNDERWATER_CUTOFF_HZ` / `ABOVE_WATER_CUTOFF_HZ`
- `crates/audio/src/lib.rs` — the filter-construction block in the **queue** dispatch path
- `crates/audio/src/lib.rs` — the byte-identical block in the **entity** dispatch path
- `crates/audio/src/lib.rs` — `update_underwater_filters`

## Description

`75ad0653` adds a `FilterBuilder` low-pass effect to **every** spatial sub-track at construction, in
both dispatch paths, and switches its cutoff between two constants:

```rust
const UNDERWATER_CUTOFF_HZ: f64 = 900.0;
const ABOVE_WATER_CUTOFF_HZ: f64 = 20_000.0;
```

**There is no dry/bypass state.** `FilterBuilder::default()` is `mix: Value::Fixed(Mix::WET)`
(`kira-0.10.8/src/effect/filter/builder.rs`) and the builder chain only sets `.mode(..)` and
`.cutoff(..)`, so the filter is 100 % wet at all times — the "above water" state is a 20 kHz low-pass,
not an absent one.

A 20 kHz cutoff is **not** transparent in kira's filter. It is Simper's SVF
(`kira-0.10.8/src/effect/filter.rs`) with `resonance` defaulting to `0.0`, giving `k = 2.0` and
therefore **Q = 0.5** — two coincident real poles, a gentle but early roll-off rather than a brick wall.
Evaluating `|H| = 1/sqrt((1-r²)² + (k·r)²)` with `r = f/f_c`:

| Frequency (at `f_c` = 20 kHz) | `r` | Gain |
|---|---|---|
| 2 kHz | 0.10 | −0.09 dB |
| 5 kHz | 0.25 | −0.5 dB |
| 10 kHz | 0.50 | **−1.9 dB** |
| 15 kHz | 0.75 | **−3.9 dB** |

### It is device-sample-rate dependent

This is the part that makes it a correctness issue rather than a taste one. The digital response is
*worse* than the analog prototype above, because the cutoff is a large fraction of Nyquist:
`g = tan(π · clamp(f_c/f_s, 0.0001, 0.5))` puts `f_c/f_s` at **0.417 on a 48 kHz device** and
**0.454 on a 44.1 kHz device**, where the tan pre-warp steepens the curve further. **The same content
sounds different on different output hardware**, with no code path that explains why.

## Evidence

Confirmed at HEAD — identical text at both dispatch sites:

```rust
let underwater = audio_world.underwater;
let cutoff = if underwater { UNDERWATER_CUTOFF_HZ } else { ABOVE_WATER_CUTOFF_HZ };
let underwater_filter = track_builder.add_effect(
    FilterBuilder::new()
        .mode(FilterMode::LowPass)
        .cutoff(cutoff),
);
```

`grep -n "\.mix(" crates/audio/src/lib.rs` returns exactly **one** hit — the `ReverbBuilder` site, which
is deliberately `Mix::WET` for a send. `update_underwater_filters` only ever writes `set_cutoff`, never
`set_mix`, so no later path can restore a dry signal.

### Second defect in the same block: the copy-paste is the exact shape #2405 was filed for

The 11-line filter-construction block above is duplicated **byte-for-byte** across both dispatch paths.
That is precisely the divergence hazard **#2405** was filed and fixed for on the reverb-send gate — the
fix for which (`apply_reverb_send`) now sits *three lines above each copy* of this new duplication. The
project standing instruction is to improve existing code rather than duplicate logic; the extraction
seam is already there and was not used.

## Impact

Every spatial sound in the engine loses roughly 2 dB at 10 kHz and 4 dB at 15 kHz, permanently, on dry
land — a subtle but global dulling of the top octave that shifts with the output device's sample rate.
It also costs a per-sample biquad on every one of up to `SUB_TRACK_CAPACITY = 512` tracks for no benefit
in the (overwhelmingly common) above-water case.

Not audible enough that anyone would report it; exactly the kind of thing that becomes impossible to
find later, once REGN/FOOT content is layered on top and "the audio sounds a bit dull" has ten candidate
causes.

## Suggested Fix

Extract the filter construction into an `apply_underwater_filter(track_builder, underwater)` helper next
to `apply_reverb_send` (closing the #2405-shaped duplication in the same edit), and make the above-water
state genuinely transparent — either:

1. **Drive `Mix` alongside the cutoff** — `Mix(0.0)` dry / `Mix::WET` submerged, tweened together in
   `update_underwater_filters`. **Preferred**: exact at every device rate, and does not rely on clamp
   edge behaviour.
2. Or set `ABOVE_WATER_CUTOFF_HZ` above any plausible Nyquist (e.g. `96_000.0`), which pins
   `f_c/f_s` to the `0.5` ceiling and passes the input through unchanged.

## Related

- **#2405** — the reverb-send gate duplication this repeats verbatim, in the same two functions.
- **#847** — same construction-time-vs-live-handle shape, but unlike #847 this one *does* have a live
  setter (`FilterHandle::set_cutoff`) already called per frame, so the fix needs no new mechanism.
- **#3086** (OPEN) — the sibling "the per-track handle exists but is never driven" defect.

## Completeness Checks

- [ ] **SIBLING**: both dispatch paths (queue + entity) go through the new shared helper — the point of
      the extraction is that a future change cannot land on one only
- [ ] **LOCK_ORDER**: the helper takes `&mut TrackBuilder` only; no new resource acquisition inside the
      dispatch loops
- [ ] **TESTS**: a guard pins the above-water state as transparent (dry mix, or cutoff ≥ Nyquist ceiling)
      and pins that both dispatch paths build the filter through one call site
