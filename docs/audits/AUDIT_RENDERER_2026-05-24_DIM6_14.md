# Renderer Audit — Material-Deep — 2026-05-24

## Scope

Dimension 6 (Shader Correctness) + Dimension 14 (Material Table R1) per
the `material-deep` preset. Targets the 4 GpuMaterial growths shipped
2026-05-23: #1248 (`ior` 280→284), #1249 (`subsurface`/`sheen`/`sheen_tint`
284→296), #1250 (`anisotropic` 296→300), #1251 (`pub mod presets`
non-struct addition).

Sibling audits in the same suite cover the cross-section:
[`AUDIT_PERFORMANCE_2026-05-24_SEC8.md`](AUDIT_PERFORMANCE_2026-05-24_SEC8.md)
covers §8 (memory / bandwidth);
[`AUDIT_SAFETY_2026-05-24_SEC8.md`](AUDIT_SAFETY_2026-05-24_SEC8.md)
covers §8 R1 invariants. Both report zero CRITICAL/HIGH/MEDIUM.

## Dimension 6 (Shader Correctness) — Findings

### REN-D6-2026-05-24-01 (MEDIUM): Disney sheen lobe over-amplified by π at per-light loop

**Location**: `crates/renderer/shaders/triangle.frag:~2604` (per-light loop site
introduced by `005eba25` / #1249); `disneyDiffuseTerm` helper at
`triangle.frag:~647-680`.

**Description**: `disneyDiffuseTerm` returns
`albedo * mix(Fd+Fretro, ss, subsurface) * (1.0 / PI) + Fsheen` —
the diffuse component is correctly normalised by `1/PI` per Lambert /
Burley convention, but Fsheen is intentionally NOT divided by PI per
Disney 2012's specification (sheen is a layered Fresnel-shaped edge
highlight, not a Lambertian term).

The per-light BRDF call site then multiplies the whole result by `PI`:

```glsl
diffuseBrdf = disneyDiffuseTerm(
    albedo, roughness, mat.subsurface, mat.sheen, mat.sheenTint,
    NdotL, NdotV, HdotL
) * (1.0 - metalness) * PI;
```

The `* PI` exists to compensate the legacy `kD * albedo` (no `/PI`)
scaling at this site — but applying it to the whole Disney return
also scales the sheen component by π, over-amplifying it ~3.14×.

The fallback-directional site at the top of the BRDF section is
correct — it uses `disneyDiffuseTerm(...) * (1.0 - metalness)` WITHOUT
the `* PI` because that site keeps the legacy `kD * albedo / PI`
scaling shape. Asymmetric per-site fix is the regression vector.

**Impact**: Visible only on materials that author `sheen > 0` AND set
`MAT_FLAG_BGSM_PBR` (cloth / silk / velvet / fabric surfaces). Currently
ZERO corpus impact because: (a) the default `sheen = 0` keeps every
legacy NIF on the Lambert branch, (b) no preset in `pub mod presets`
sets sheen, (c) no BGSM importer surfaces sheen yet. Latent regression
that activates the moment a BGSM v9+ material with authored sheen lands.

The over-amplification is on the per-LIGHT loop, so the artifact
compounds per scene light — a cluster with 4 lights at the cloth
surface produces 4 × π ≈ 12.5× over-bright sheen edge.

**Suggested Fix**: Split `disneyDiffuseTerm` into two helpers returning
the diffuse and sheen parts separately so each call site can apply its
own scaling:

```glsl
vec3 disneyDiffuseLobe(...) { return albedo * mix(Fd+Fretro, ss, subsurface) / PI; }
vec3 disneySheenLobe(...) { return Fsheen; } // never divided by PI
```

Then the per-light site composes them with PI applied only to the
diffuse:

```glsl
diffuseBrdf = (disneyDiffuseLobe(...) * PI + disneySheenLobe(...))
            * (1.0 - metalness);
```

And the fallback site:

```glsl
diffuseBrdf = (disneyDiffuseLobe(...) + disneySheenLobe(...) / PI)
            * (1.0 - metalness);
// or restructure so both sites share the same scaling convention
```

Alternative: have `disneyDiffuseTerm` return a struct
`{ vec3 diffuse; vec3 sheen; }` so the compositional shape is explicit
at every call site.

### REN-D6-2026-05-24-02 (LOW): `dielectricF0FromIor` lacks IOR-domain clamp

**Location**: `crates/renderer/shaders/triangle.frag:~600`
(`dielectricF0FromIor`).

**Description**: The Schlick F0 formula `((1-η)/(1+η))²` produces
non-negative output for any real η (the square absorbs sign), but the
formula is physically meaningful only for η > 0. If `mat.ior == 0`
(uninitialized garbage from a future importer that forgets to set it
or a BGSM parser regression), the formula yields `((1-0)/(1+0))² = 1.0`
— mirror-like F0 on what should be a dielectric. The current
`GpuMaterial::default() → ior = 1.5` makes this a defensive concern
only; no live path produces ior = 0 today.

**Impact**: Defense-in-depth gap only. Default value blocks the bad
state from materialising on legacy NIF; importer-side bugs that ship
zero IOR would render as full-mirror Fresnel on what should be a
dielectric, looking like a silvered glass / chrome surface where one
shouldn't be.

**Suggested Fix**: Add a single-line clamp inside the helper:

```glsl
float dielectricF0FromIor(float eta) {
    float e = max(eta, 1e-3); // guard against importer-side zeros
    float r = (1.0 - e) / (1.0 + e);
    return r * r;
}
```

Or rely on importer-side clamping at the `to_gpu_material` boundary.
The latter is preferred (catches the bug at its source) but the
shader-side guard is cheaper insurance.

### REN-D6-2026-05-24-03 (LOW): `deriveAxAy` lacks anisotropic-domain clamp → potential sqrt(negative)

**Location**: `crates/renderer/shaders/triangle.frag:~625`
(`deriveAxAy`).

**Description**: The Disney aspect formula is
`aspect = sqrt(1 - anisotropic * 0.9)`. For `anisotropic ∈ [0, 1]`
the radicand `1 - 0.9·a` ranges from 1.0 down to 0.1 — all positive.
But if a future authoring surface ships `anisotropic > 1.0`
(unclamped BGSM v9+ value, or a Starfield .mat field with a different
range convention), `1 - 0.9·a < 0` and `sqrt(...)` becomes NaN on GLSL
spec-conformant drivers (implementation-defined / poison value on
others). Downstream `ax = alpha / aspect` then divides by NaN, and
`distributionGGXAniso` returns NaN → black pixel or undefined-color
fragment.

Same shape concern: `anisotropic < 0` would give aspect > 1 → valid
sqrt but the `ax / aspect` would shrink ax below the intended floor.

**Impact**: No live path today (no importer surfaces anisotropic).
Same defense-in-depth concern as REN-D6-NEW-02. Failure mode is
visible black pixels at the boundary fragments where anisotropic
authoring crosses the bad range.

**Suggested Fix**: Add a single-line clamp at the top of the helper:

```glsl
void deriveAxAy(float roughness, float anisotropic, out float ax, out float ay) {
    float a = roughness * roughness;
    float aniso = clamp(anisotropic, 0.0, 1.0); // guard against bad importer data
    float aspect = sqrt(1.0 - aniso * 0.9);
    ax = max(0.025 * 0.025, a / aspect);
    ay = max(0.025 * 0.025, a * aspect);
}
```

### REN-D6-2026-05-24-04 (INFO): Anisotropic GGX algebraic equivalence at isotropic case — confirmed

**Location**: `crates/renderer/shaders/triangle.frag:~615`
(`distributionGGXAniso`).

**Description**: The helper's docstring claims that when `ax = ay = α`
the anisotropic NDF reduces algebraically to the isotropic form
`α²/(π · (NdotH²(α²-1) + 1)²)` matching legacy `distributionGGX`.

Verification: using the unit-tangent-space identity
`HdotX² + HdotY² + NdotH² = 1`:

```
D_aniso(ax=ay=α) = 1 / (π · α² · (HdotX²/α² + HdotY²/α² + NdotH²)²)
                 = 1 / (π · α² · ((HdotX² + HdotY²)/α² + NdotH²)²)
                 = 1 / (π · α² · ((1 - NdotH²)/α² + NdotH²)²)
                 = 1 / (π · α² · ((1 - NdotH² + α²·NdotH²) / α²)²)
                 = α² / (π · (1 + NdotH²(α² - 1))²)
```

Matches legacy `distributionGGX` exactly. The `mat.anisotropic > 0`
gate at both BRDF sites makes this academic for default-isotropic
content — but the math is correct.

### REN-D6-2026-05-24-05 (INFO): `dielectricF0FromIor` default produces F0 ≈ 0.04 — confirmed

**Location**: `crates/renderer/shaders/triangle.frag:~600`.

**Description**: `dielectricF0FromIor(1.5) = ((1-1.5)/(1+1.5))² = 0.04`
exactly. The pre-#1248 hardcoded `vec3(0.04)` literal is reproduced
byte-for-byte for legacy NIF content with no authored IOR. No
visible regression for the corpus.

### REN-D6-2026-05-24-06 (INFO): MAT_FLAG_BGSM_PBR gate correctly preserves Lambert for legacy

**Location**: `crates/renderer/shaders/triangle.frag` fallback-directional
BRDF (~line 2390) + per-light loop (~line 2580).

**Description**: Both BRDF sites gate Disney vs Lambert on
`(mat.materialFlags & MAT_FLAG_BGSM_PBR) != 0u`. Every NIF without a
v>=8 BGSM never sets that flag, so the Lambert branch fires and the
diffuse term computation is byte-equivalent to the pre-#1249 shader.

## Dimension 14 (Material Table R1) — Findings

### Dim 14 — No additional findings

The audit-safety §8 R1 invariants pass confirms all of the Dim 14
correctness claims (size pin 300, offset pins 280/284/288/292/296,
GLSL needle pins for the 4 new fields, hash walk lockstep, 7 DrawCommand
construction sites complete, `to_gpu_material` 1:1 forward, preset
table tests pass). Dim 14 looks at material-table CORRECTNESS through
a different lens than safety's INVARIANTS, but in this case both
arrive at the same baseline: clean.

One INFO-level positive confirmation worth recording:

### REN-D14-2026-05-24-01 (INFO): R1 dedup contract preserved across 4-field growth

**Location**: `crates/renderer/src/vulkan/material.rs:~459`
(`hash_gpu_material_fields`) + `crates/renderer/src/vulkan/context/mod.rs:~438`
(`DrawCommand::material_hash`).

**Description**: The `material_hash_matches_gpu_material_field_hash`
test exercises a `fully_populated_draw_command` fixture with distinct
non-default values for every new field (1.45 / 0.42 / 0.18 / 0.66 /
0.27). Both walks include the 4 new fields in identical trailing
order. The byte-equal-safe contract pinned by #781 still holds.

### REN-D14-2026-05-24-02 (INFO): Preset table inheritance guard catches future drift

**Location**: `crates/renderer/src/vulkan/material.rs::presets::tests::presets_inherit_defaults_for_unset_fields`.

**Description**: This sentinel test pins that `polished_metal()`
inherits `ior == 1.5`, `anisotropic == 0.0`, `sheen == 0.0` via
`..Default::default()`. A future GpuMaterial growth that drifts any
preset's inherited values away from the Hyperion table reference
surfaces immediately. Good defensive coverage of the #1251 work.

## Summary

| Severity | Count | Items |
|----------|-------|-------|
| CRITICAL | 0     | |
| HIGH     | 0     | |
| MEDIUM   | 1     | REN-D6-2026-05-24-01 (sheen × π over-amplification at per-light) |
| LOW      | 2     | REN-D6-2026-05-24-02 (no IOR clamp), REN-D6-2026-05-24-03 (no anisotropic clamp) |
| INFO     | 5     | 3 × Dim 6 positive confirmations, 2 × Dim 14 positive confirmations |

**Headline**: 1 MEDIUM regression introduced by this session's own #1249
(`005eba25`) — the sheen lobe is over-amplified by ~π at the per-light
BRDF site. Masked today because every legacy NIF + every preset has
`sheen = 0`, but it activates the moment a BGSM v9+ material with
authored sheen lands (or a future preset uses sheen). 2 defense-in-depth
LOW findings on missing input-domain clamps that future bad authoring
data would trigger. No Dim 14 findings (the R1 contract held across
the 4-field growth).

## Recommended next step

`/audit-publish docs/audits/AUDIT_RENDERER_2026-05-24_DIM6_14.md` to
file the MEDIUM + 2 LOW as GitHub issues, then `/fix-issue` the MEDIUM
sheen bug while the context is fresh.
