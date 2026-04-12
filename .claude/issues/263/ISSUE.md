# #263: LC-03 — Alpha test function bits not extracted

**Severity**: MEDIUM | **Domain**: import-pipeline, legacy-compat | **Type**: bug
**Location**: `crates/nif/src/import/material.rs`

## Problem
NiAlphaProperty flags bits 10-12 (test function) not extracted. Always assumes GREATEREQUAL. Meshes with LESS test render wrong pixels.

## Fix
Extract bits 10-12, map to vk::CompareOp, pass through MaterialInfo.
