# Investigation — Issue #474

## Authoritative source

`/mnt/data/src/reference/nifxml/nif.xml` (niftools/nifxml clone).

## Three targeted fixes

### 1. bhkSimpleShapePhantom (`collision.rs:862-881`)

**nif.xml line 2790-2795:**
```xml
<niobject name="bhkSimpleShapePhantom" inherit="bhkShapePhantom" ...>
    <field name="Unused 01" type="byte" length="8" binary="true" />
    <field name="Transform" type="Matrix44" />
</niobject>
```

Inheritance chain: `bhkSimpleShapePhantom` → `bhkShapePhantom` (no fields) → `bhkPhantom` (no fields) → `bhkWorldObject` (Shape ref + Havok Filter + bhkWorldObjectCInfo).

Current parser reads: shape_ref(4) + havok_filter(4) + skip(20) + transform(64) = **92 bytes**.
Missing: 8-byte `Unused 01` between CInfo skip and transform.

Fix: insert `stream.skip(8)?` before transform read. 92 + 8 = **100 bytes** ✓.

### 2. WaterShaderProperty (`shader.rs` — new parser) + `mod.rs:261` (remove from PPLighting alias)

**nif.xml line 6322-6324:**
```xml
<niobject name="WaterShaderProperty" inherit="BSShaderProperty" versions="#FO3_AND_LATER#">
    Bethesda-specific property. Found in Fallout3
</niobject>
```

**No additional fields.** Inherits `BSShaderProperty` — NOT `BSShaderLightingProperty`, so no `texture_clamp_mode`.

`BSShaderProperty` fields (from `base.rs:BSShaderPropertyData` + nif.xml):
- `shade_flags` u16 (from NiShadeProperty)
- `shader_type` u32, `shader_flags_1` u32, `shader_flags_2` u32, `env_map_scale` f32

Current over-read: aliased to PPLighting which additionally reads texture_clamp_mode(4) + texture_set_ref(4) + refraction(8) + parallax(8) = 24 bytes too many. Matches audit "expected 30, consumed 54".

Fix: dedicated `WaterShaderProperty::parse` that reads only `NiObjectNETData` + `BSShaderPropertyData_base` (without texture_clamp_mode).

Need a new `BSShaderPropertyData::parse_base` variant (no texture_clamp_mode).

### 3. TallGrassShaderProperty (`shader.rs` — new parser) + `mod.rs:262` (remove from alias)

**nif.xml line 6354-6357:**
```xml
<niobject name="TallGrassShaderProperty" inherit="BSShaderProperty" versions="#BETHESDA#">
    <field name="File Name" type="SizedString">Texture file name</field>
</niobject>
```

Like WaterShaderProperty but adds a single `SizedString` filename field.

Fix: dedicated `TallGrassShaderProperty::parse` → base + SizedString.

## Other candidates in current alias chain (same inheritance — BSShaderProperty NOT BSShaderLightingProperty)

From nif.xml:
- `DistantLODShaderProperty`: BSShaderProperty, no extra fields → same as WaterShaderProperty
- `BSDistantTreeShaderProperty`: BSShaderProperty, no extra fields → same
- `VolumetricFogShaderProperty`: BSShaderProperty, no extra fields → same
- `HairShaderProperty`: BSShaderProperty, no extra fields → same
- `SkyShaderProperty` (line 6335): inherits `BSShaderLightingProperty` — has texture_clamp_mode + FileName + SkyObjectType
- `BSSkyShaderProperty` (line 6268, Skyrim+): different
- `BSWaterShaderProperty` (line 6695, Skyrim+): different
- `Lighting30ShaderProperty` (line 6367): inherits `BSShaderPPLightingProperty` — correct current alias

**SIBLING scope**: the same over-read bug affects DistantLODShaderProperty / BSDistantTreeShaderProperty / VolumetricFogShaderProperty / HairShaderProperty. SkyShaderProperty also over-reads (it should not have refraction/parallax but IS in the BSShaderLightingProperty branch with a FileName + SkyObjectType field it's missing).

This is a bigger ripple than three types. For this fix I'll do the 3 issue-named types + also route the 4 no-field BSShaderProperty siblings to the same `WaterShaderProperty::parse` (since they all have identical layout). `SkyShaderProperty` gets its own parser.

## Files touched

1. `crates/nif/src/blocks/collision.rs` — bhkSimpleShapePhantom parse
2. `crates/nif/src/blocks/base.rs` — add `parse_base_no_tcm` method
3. `crates/nif/src/blocks/shader.rs` — new WaterShaderProperty + TallGrassShaderProperty + SkyShaderProperty + DistantLOD/DistantTree/VolumetricFog/Hair
4. `crates/nif/src/blocks/mod.rs` — dispatch table update (remove from PPLighting alias, add new arms)

4 files, in scope.

## Issue #475 premise check

Audit claimed `ControlledBlock records start/stop per channel`. Verified at `crates/nif/src/blocks/controller.rs:393-412` — the struct has NO `start_time`/`stop_time` fields. The temporal range in nif.xml lives on:
- `NiControllerSequence.start_time` / `stop_time` (global, captured as `AnimationClip.duration`)
- `NiBSplineInterpolator.start_time` / `stop_time` (already captured in block, sampled internally)

Recommendation: close #475 with a comment that the premise was a misreading — partial-range clips ARE supported via the sequence's global start_time/stop_time, and per-interpolator range for BSpline interpolators. No refactor needed. The real gap #338 (NiControllerManager state machine) is already tracked.

**Not fixing #475 in this commit.**
