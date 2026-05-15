# Issue #976: NIF-D4-NEW-02: BSLightingShaderProperty drops Starfield .mat material refs

**State:** OPEN
**Labels:** bug, nif-parser, import-pipeline, medium
**Location:** `crates/nif/src/import/material/walker.rs:116-121`

## Bug

BSLightingShaderProperty branch uses an inline suffix check for `.bgsm`/`.bgem`
only, missing `.mat` (Starfield) and not trimming trailing whitespace.
BSEffectShaderProperty branch correctly delegates to `material_path_from_name`.

## Fix

Replace inline check with `crate::import::mesh::material_path_from_name(...)`.
Add regression test for `.mat` suffix on BSLightingShaderProperty.
