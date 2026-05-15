# Issue #1069: F-WAT-09 — WATR reflection_color propagation

**State**: OPEN | **Severity**: LOW | **Domain**: renderer + ESM

reflection_color parsed in WatrRecord.params but never transferred to WaterMaterial
and never reaches the shader. Fix: add reflection_tint to WaterMaterial, transfer it,
add tint_reflect vec4 to WaterPush, update water.frag to use it.

Files: water.rs (core), cell_loader/water.rs, renderer/water.rs, render.rs, water.frag
