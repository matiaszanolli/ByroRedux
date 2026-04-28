# INVESTIGATION — Issue #729

## Findings

### Root cause: NAM0 group indices are off-by-one starting at index 2

Authoritative source: tes5edit fopdoc (Fallout 3 + Fallout NV WTHR), confirmed
identical between both games:

```
0  Sky-Upper
1  Fog
2  Unused        ← parser had SKY_AMBIENT here
3  Ambient       ← parser had SKY_SUNLIGHT here
4  Sunlight      ← parser had SKY_SUN here
5  Sun           ← parser had SKY_STARS here
6  Stars         ← parser had SKY_LOWER here
7  Sky-Lower     ← parser had SKY_HORIZON here
8  Horizon       ← parser had SKY_CLOUDS_LOWER here (and never read it)
9  Unused        ← parser had SKY_CLOUDS_UPPER here (and never read it)
```

The pre-fix parser tagged the indices as
`sky_upper, fog, ambient, sunlight, sun, stars, sky_lower, horizon, clouds_lower, clouds_upper`
— i.e. it dropped the "Unused" slot at index 2 and re-numbered everything
after, then invented two cloud-color slots at 8 and 9 that are not in the
on-disk schema.

### Effect on the renderer

`scene.rs` `apply_worldspace_weather` and `weather_system` both index by the
parser constants. Pre-fix:

| What renderer asks for | Slot read | What slot actually contains |
|---|---|---|
| `SKY_AMBIENT[DAY]` | 2 | "Unused" — junk authored RGB |
| `SKY_SUNLIGHT[DAY]` | 3 | real Ambient |
| `SKY_SUN[DAY]` | 4 | real Sunlight |
| `SKY_HORIZON[DAY]` | 7 | real Sky-Lower |

The user's logged values reconcile cleanly under this hypothesis:

```
"sun"      [1.0,   0.890, 0.667]  warm yellow ← actually real Sunlight (slot 4)
"sunlight" [0.341, 0.412, 0.541]  cool blue   ← actually real Ambient   (slot 3)
"ambient"  [0.592, 0.659, 0.718]  cool grey   ← "Unused" slot (slot 2) authored junk
"horizon"  [0.361, 0.451, 0.580]  desat blue  ← actually Sky-Lower (slot 7)
```

The "Unused" slot in `NVWastelandClear` happens to carry a desaturated
cool-blue-grey, not green — so the off-by-one alone does not visibly cause
the green tint. But its consequences are still wrong:

* directional sun light is sourced from real *Ambient* (cool blue) rather
  than real *Sunlight* (warm yellow) — the directional term hits every
  fragment that isn't shadowed.
* sky `horizon_color` is sourced from *Sky-Lower* rather than the real
  *Horizon* group. Real *Horizon* (slot 8) was completely unread.
* `SKY_CLOUDS_LOWER`/`SKY_CLOUDS_UPPER` constants were dead — cloud colors
  for clouds come from the actual cloud TEXTURES (DNAM/CNAM/ANAM/BNAM
  paths), not from NAM0.

### Green-tint hypothesis still standing

`SKY_FOG = 1` is correctly indexed per xEdit. But its contents for
`NVWastelandClear` aren't logged — that is the user's hypothesis 1, and it
remains the most likely explanation since fog blends into every fragment
past `fog_near` (= -10 here).

This fix:
1. Corrects the NAM0 group indices to match xEdit's authoritative layout.
2. Adds `fog_color` to the bootstrap log so the next FNV streaming session
   immediately confirms or denies hypothesis 1 without another patch
   round-trip.

## Files touched

* `crates/plugin/src/esm/records/weather.rs` — slot indices + doc + tests
* `byroredux/src/scene.rs` — log fog_color
* (Existing `parse_wthr_basic` test offsets updated to match new SKY_HORIZON)
