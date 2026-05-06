## FNV-D3-NEW-02: XCLL extended fog and ambient-cube fields parsed but dropped at renderer layer

## Source Audit
`docs/audits/AUDIT_FNV_2026-05-05.md` — Dim 3 MEDIUM

## Severity / Dimension
MEDIUM / Cell loading × Renderer — data propagation gap

## Location
`byroredux/src/components.rs:108-123` (`CellLightingRes` definition — only 3 fog fields)
`byroredux/src/scene.rs:728-739` (only XCLL→CellLightingRes conversion site)
`byroredux/src/cell_loader.rs:429-441` (LGTM fallback documents this as future work)

## Description
The plugin layer's `CellLighting` struct carries a fully-populated 92-byte XCLL with 9 optional Skyrim/FNV-extended fields:
- `directional_fade`
- `fog_clip`, `fog_power`
- `fog_far_color`, `fog_max`
- `light_fade_begin` / `light_fade_end`
- `directional_ambient` (ambient cube)
- `specular_color` / `specular_alpha` / `fresnel_power`

`CellLightingRes` (the renderer-facing resource) has only **3 fog-related fields**:

```rust
pub(crate) struct CellLightingRes {
    pub(crate) fog_color: [f32; 3],
    pub(crate) fog_near: f32,
    pub(crate) fog_far: f32,
    // (plus ambient + directional_color + directional_dir)
}
```

The conversion in `scene.rs:728-739` reads only those plus `ambient`, `directional_color`, and `directional_dir`. **Everything else is dropped on the floor.**

## Evidence
```bash
$ grep -rn "fog_clip\|fog_power\|directional_fade\|fog_far_color\|fog_max\|light_fade_begin\|directional_ambient" \
    byroredux/src/ crates/renderer/src/
# Returns hits only inside cell_loader_lgtm_fallback_tests.rs (synthetic test data)
# and crates/plugin/src/esm/cell/tests.rs (parser tests).
# Zero hits in the renderer or in any system that consumes CellLightingRes.
```

XCLL parser already handles all three on-disk shapes correctly (28-byte Oblivion / 40-byte FNV / 92-byte Skyrim+) at `crates/plugin/src/esm/cell/walkers.rs:149-240`, pinned by 3 cross-game tests. The data reaches the plugin layer; it just doesn't survive the conversion to `CellLightingRes`.

## Impact
- **FNV** authored interior fog renders as plain linear (clip-at-`fog_far`, no power curve). Visible on close camera approaches in cells like Doc Mitchell's House and the Goodsprings Source Pump.
- **Skyrim** cells with authored ambient cubes (Solitude inn cluster, Dragonsreach, Markarth) render as flat single-channel ambient — they parse correctly upstream but the data never reaches the GPU.
- M44+ Skyrim SE / FO4 visual fidelity will look subtly wrong against Bethesda reference until this is plumbed through.

## Suggested Fix
1. Extend `CellLightingRes` with the `Optional<f32>` / `Optional<[f32;3]>` fields matching `CellLighting`.
2. Propagate them through both XCLL conversion site (scene.rs) AND the LGTM fallback (`cell_loader.rs:429-441` already TODOs this).
3. Add the corresponding shader uniforms on geometry / sky-composite pipelines.
4. Pin with a fixture-cell regression test asserting `CellLightingRes.fog_clip == Some(7500.0)` (the existing parser fixture in `crates/plugin/src/esm/cell/tests.rs:1597`).

## Related
- See companion finding `FNV-D3-NEW-06` (`fog_clip` / `fog_power` LOW) — same root cause, narrower scope.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Both XCLL conversion sites (scene.rs `--esm` arm, cell_loader.rs LGTM fallback) need the same field set
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Cross-game fixture cells with non-trivial extended fields; pin renderer uniform contents end-to-end
