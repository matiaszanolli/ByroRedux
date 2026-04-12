# #261: LC-01 — NIF-embedded controllers not traversed

**Severity**: MEDIUM | **Domain**: import-pipeline, legacy-compat | **Type**: enhancement
**Location**: `crates/nif/src/import/walk.rs`

## Problem
controller_ref chains on geometry nodes never traversed during import. NiVisController, NiFlipController, NiTextureTransformController, NiAlphaController silently discarded. Static water/lava/fire.

## Fix
Walk controller_ref chains during import, convert supported types to float/bool channels.
