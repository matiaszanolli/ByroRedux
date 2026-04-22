# #536 Investigation — M33-04 FNAM empty body

## Domain
**esm** — `crates/plugin/src/esm/records/weather.rs`.

## Root cause

Parser has two mutually-conflicting assumptions:

- `weather.rs:149` HNAM arm gates `>= 16` and reads 4 f32 as
  `fog_{day,night}_{near,far}`. Comment elsewhere (both the field comment
  and the scene.rs call-site) treats HNAM as the fog source.
- `weather.rs:158` FNAM arm gates `>= 4` with an empty body ("fallback
  when HNAM is absent").

Byte-level evidence from the deleted audit harness (re-checked below):

| Master | HNAM | FNAM |
|---|---|---|
| FalloutNV.esm (63 WTHRs) | **0 records** | 63 / 63 (size 24) |
| Fallout3.esm (27) | **0** | 27 / 27 (size 24) |
| Oblivion.esm (37) | 37 (size 56, lighting params — see #537) | 37 (size 16, fog) |

FNV FNAM peek (`NVHooverFinalBat…` #2):
```
00 00 7a c4 | 00 50 c3 47 | 00 00 20 c1 | 00 50 43 48 | ?? ?? ?? ?? | ?? ?? ?? ??
= -1000.0   | 100012.5    | -10.0       | 200012.5    | (unknown)   | (unknown)
  day_near   day_far        night_near    night_far
```

Oblivion FNAM peek (`SE13JiggyWeather`):
```
00 00 fa c3 | 00 00 fa 45 | 00 00 16 c4 | 00 80 bb 45
= -500.0    | 16000.0     | -600.0      | 6000.0
  day_near   day_far        night_near    night_far
```

First 4 f32 layout matches across all three games: `[day_near, day_far,
night_near, night_far]`. FNV/FO3 carries 8 trailing bytes whose semantics
I have not pinned (no UESP byte-by-byte reference on hand); leaving them
unread is the "no guessing" path.

## Interaction with existing HNAM arm

Oblivion emits sub-records in schema order `EDID CNAM DNAM NAM0 FNAM
HNAM DATA …`. The parser iterates that order. Pre-fix:

- FNAM matches (empty body) → no-op
- HNAM matches (`>= 16`; actual 56 B) → reads 4 f32 from the 56-byte
  lighting-params payload as fog distances → `fog_far ≈ 4.0`

If I just fill the FNAM body, HNAM will still fire **after** FNAM in the
iteration and overwrite the correct fog values with garbage. Two options:

1. **Remove HNAM arm entirely** — pushes scope into #537 territory.
2. **Tighten HNAM gate to `== 16`** — excludes Oblivion's real 56-byte
   HNAM (which will be handled correctly under #537) while preserving
   the existing synthetic unit-test fixture that uses a 16-byte HNAM
   for fog.

Option 2 is the minimum-scope fix. Oblivion HNAM falls through to the
unknown-subrecord skip; FNAM's 16 bytes carry the fog, so Oblivion fog
parses correctly for the first time.

## Files touched

1. `crates/plugin/src/esm/records/weather.rs` — FNAM body + HNAM gate
2. `crates/plugin/tests/parse_real_esm.rs` — non-default-fog assertions

2 files. Under scope.

## Deferred

- **#537 M33-05 Oblivion HNAM 56-byte semantic decode** — untouched by
  this fix; once it lands, the `== 16` gate here can be unified with
  #537's per-size dispatch.
- **FNV/FO3 FNAM trailing 8 bytes** — left unread. Need UESP-
  authoritative cross-check before interpreting.
