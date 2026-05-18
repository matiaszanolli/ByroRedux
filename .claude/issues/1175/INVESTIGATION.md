# #1175 Investigation — Backlight Power gate inverted

## Spec verification (three independent sources agree)

1. **nif.xml** (`/mnt/data/src/reference/nifxml/nif.xml`):
   ```xml
   <field name="Rimlight Power" type="float" default="#FLT_MAX#" vercond="#BS_FO4_2#" />
   <field name="Backlight Power" type="float" range="#F0_1000#"
          cond="(Rimlight Power #GTE# #FLT_MAX#) #AND# (Rimlight Power #LT# #FLT_INF#)"
          vercond="#BS_FO4_2#" />
   ```
   → Backlight present iff `rim >= FLT_MAX && rim < INFINITY`, i.e. `rim == FLT_MAX` (sentinel).

2. **openmw** (`/mnt/data/src/reference/openmw/components/nif/property.cpp`):
   ```cpp
   nif->read(mRimlightPower);
   if (mRimlightPower == std::numeric_limits<float>::max())
       nif->read(mBacklightPower);
   ```

3. **nifly** (`/mnt/data/src/reference/nifly/src/Shaders.cpp`):
   ```cpp
   stream.Sync(rimlightPower2);
   if (rimlightPower2 >= NiFloatMax && rimlightPower2 < NiFloatInf)
       stream.Sync(backlightPower);
   ```

## Our code (inverted)

`crates/nif/src/blocks/shader.rs:880-886`:
```rust
let back = if rim < 3.0e38 {
    stream.read_f32_le()?
} else {
    0.0
};
```

Boolean opposite — reads backlight when `rim < FLT_MAX` (finite override), skips when `rim == FLT_MAX` (sentinel).

## Drift modes

- **rim == FLT_MAX (common — sentinel default)**: spec says backlight follows. Our code skips → reads grayscale-to-palette into our `backlight_power=0.0` slot → 4-byte drift across `fresnel_power`, all 7 `WetnessParams` floats, and shader-type trailing data.
- **rim < FLT_MAX (rare — author override)**: spec says no backlight. Our code reads → consumes 4 bytes that are actually grayscale-to-palette → same drift, opposite direction.

`block_size` recovery resyncs at block boundary, so parse-rate stays at 100% but field values are wrong.

## Fixture chain to update

- `shader_tests.rs:506` (`build_bs_lighting_fo4_env_map` helper) authors `rim=2.5f32`, backlight=1.0 — matches the inverted code.
- `shader_tests.rs:598` (BSVER 131 fixture) — same pattern.
- `shader_tests.rs:670` (BSVER 132 fixture) — same pattern.
- `shader_tests.rs:1049-1050` (`parse_bs_lighting_fo4_env_map_with_wetness` assertion) — asserts `rimlight==2.5`.

After fix: fixtures author `rim=f32::MAX` (keeping backlight bytes present per spec), assertion updates to expect `f32::MAX` for rim.

## Add regression test

New test: pin the inverted case — fixture with `rim = 2.5f32` (finite override) and NO backlight bytes following → grayscale at the post-rim offset. Assert `backlight_power == 0.0` (the spec-default when absent).
