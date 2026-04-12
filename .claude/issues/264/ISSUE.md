# #264: LC-04 — dark_texture lightmap slot not imported

**Severity**: MEDIUM | **Domain**: import-pipeline, legacy-compat | **Type**: enhancement
**Location**: `crates/nif/src/import/material.rs`

## Problem
NiTexturingProperty slot 1 (dark_texture, multiplicative lightmap) parsed but never extracted. Missing baked shadows on Oblivion interiors.

## Fix
Extract dark_texture path, wire to MaterialInfo.dark_map, shader multiplicative blend.
