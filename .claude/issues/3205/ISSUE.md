# FO3-2026-08-20-D3-02: decode_data_fo3nv's "first sixteen bytes are opaque, not wind or wave controls" premise is falsified by 53/53 vanilla FO3 records — wind and wave amp/freq are sourced from the wrong fields

State: OPEN
Labels: bug,import-pipeline,low,legacy-compat,game:fo3,esm-plugin

- **Severity**: LOW
- **Dimension**: FO3 Dim 3 — ESM Record Coverage, FO3-unique authoring
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:550-554` (the doc-comment), `:555-568` (the arm starts reading at offset 16), `:648-649` (`wind_direction`/`wind_speed` sourced from the noise-layer-1 fields instead), `:581-587` (`wave_amplitude`/`wave_frequency` sourced from the displacement simulator's force/velocity at 76/80)
- **Status**: NEW — a **remainder defect adjacent to #3107**, not covered by it. #3107 is about the `DNAM` arm reading *too little*; this is about the `DATA` arm reading the *wrong fields* for four canonical controls.

## Description

`decode_data_fo3nv` skips offsets 0..16 on the stated grounds that xEdit calls them opaque:

```rust
/// Decode the authoritative full FO3/FNV visual-data layout. The first
/// sixteen bytes are opaque in xEdit's definition; they are not wind or wave
/// controls. Visual fields begin at byte 16, ...
```

**Vanilla FO3 says otherwise.** Offsets 0/4/8/12 are the GECK water defaults *Wind Velocity / Wind Direction / Wave Amplitude / Wave Frequency*, and offsets 16/20/24 are Sun Power / Reflectivity / Fresnel — which is exactly the field order `decode_dnam_pre_fo4` already reads from the same byte range, and exactly what #3107 independently verified against shipped bytes.

Because the arm treats the prefix as opaque, it substitutes:

- `wind_direction` / `wind_speed` ← **noise-layer-1** direction/speed (`:648-649`)
- `wave_amplitude` / `wave_frequency` ← the **displacement simulator's** force/velocity at 76/80 (`:581-587`)

so the two FO3 arms disagree about where four canonical fields live.

## Evidence

Python walker over all 53 `Fallout3.esm` `WATR` visual payloads (`DNAM` and long `DATA` alike):

```
offset  4 : distinct = 1  -> 90.0 on 53/53           (Wind Direction, degrees)
offset  0 : distinct = 3  -> 0.1 x46, 3.0 x6, 2.0 x1 (Wind Velocity)
offset  8 : 0.5 / 0.2                                (Wave Amplitude)
offset 12 : 1.0 / 0.25                               (Wave Frequency)
```

`0.1 / 90.0 / 0.5 / 1.0` is the GECK's default water tuple verbatim — **46 of 53 records never departed from it**. The same file already treats FO3 direction fields as degrees (`:648` calls `.to_radians()` on the offsets-100/104/108 triple), so the two readings of the same record cannot both be right.

#3107's byte-level arbitration reached the same conclusion from the `DNAM` side: `[0]`=wind speed (3.0), `[4]`=direction (90), `[8]/[12]`=wave amp/freq (0.2 / 0.25).

## Impact

On the **11 FO3 records** that ship the long `DATA` (and **8 on FNV**), `wave_amplitude` / `wave_frequency` carry the displacement simulator's `0.4 / 0.6` instead of the authored `0.5 / 1.0`, and `wind_speed` carries a noise-layer scroll rate instead of the authored velocity. No crash — the values are plausible, which is why this survived.

**Its real cost is the merge direction.** A false sourcing claim sits directly on the function that #3107's fix is supposed to collapse `decode_dnam_pre_fo4` into. #3107's suggested fix is "route `GameKind::Fallout3NV` `DNAM` through the same tail decode as the 186-byte `DATA`" — if an implementer trusts this comment and routes the whole record through `decode_data_fo3nv` as-is, they will **preserve the divergence and simultaneously lose the correct 0..16 head decode that `decode_dnam_pre_fo4` already has**, converting a 19-record defect into a 121-record one.

## Related

- **#3107** (`WATR-ARB-04`) — the majority `DNAM` path stopping at byte 52. **Fix ordering matters:** the correct collapse is `decode_dnam_pre_fo4`'s head (0..52) **plus** `decode_data_fo3nv`'s tail (52..196), not one function replacing the other.
- #3105 — the rain/displacement start-size swap in the same function (offsets 72/92).
- #3108 — `normal_magnitude` from `DATA[96]` in the same function.
- #3144 — the degrees/radians half of `wind_direction`.
- #3146 — `decode_data`'s unreachable 144-220 tail (Oblivion arm; different function).
- Project memory `watr_data_layout_shift`.

## Suggested Fix

Delete the opaque-prefix claim from the doc-comment and read 0..28 in `decode_data_fo3nv` exactly as `decode_dnam_pre_fo4` does. Better: make `decode_data_fo3nv` *be* `decode_dnam_pre_fo4` **plus** the shared 52..196 tail — which is the shape #3107 wants, expressed in the direction that keeps both correct halves.

Stop sourcing `wind_direction`/`wind_speed` from `noise_wind_directions[0]`/`noise_wind_speeds[0]`, and `wave_amplitude`/`wave_frequency` from the displacement force/velocity, once the head is read.

---
*Filed from `docs/audits/AUDIT_FO3_2026-08-20.md` (Dim 3). Verified against HEAD `bb0b92f2` — the "first sixteen bytes are opaque … they are not wind or wave controls" comment is live at `water.rs:550-554`, and the arm's first read is `read_f32_at(data, 16)`.*

## Completeness Checks
- [ ] **SIBLING**: the same head/tail split checked against every other per-game `WATR` decoder in `water.rs` (they do not share a helper)
- [ ] **CANONICAL-BOUNDARY**: per-game layout logic stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins the fix against *shipped bytes*, asserting a long-`DATA` FO3 record reports `wave_amplitude == 0.5` / `wave_frequency == 1.0` — not against the decoder's own output
- [ ] **ORDER**: coordinated with #3107 so the collapse keeps `decode_dnam_pre_fo4`'s head **and** `decode_data_fo3nv`'s tail

