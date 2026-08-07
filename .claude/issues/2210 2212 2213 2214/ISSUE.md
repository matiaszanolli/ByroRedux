# Issues 2210, 2212, 2213, 2214

## #2210 — NIFAL-D3-02: the 2048.0 no-attenuation light fallback is an uncited constant that is the shipped behaviour (82/82 FNV)
**Location**: `crates/nif/src/import/walk/mod.rs:1637-1660` (`attenuation_radius`).

`attenuation_radius` solves `1 / (const + lin·d + quad·d²) = 1/256` for effective light radius, falling through to a bare `2048.0` when neither quadratic nor linear coefficient is present. This is the operative radius for 82/82 measured FNV spawnable point lights (not a rare fallback) and carries no citation. Contrast `LIGHT_RANGE_EXTENSION`, which cites OpenMW.

**Fix**: cite a source for `2048.0` (Gamebryo 2.3 `NiLight` default or measured derivation) or replace with a cited value. Per project no-guessing policy — research before picking a number.

## #2212 — NIFAL-D8-01: synthesized FO4 alpha-test threshold (128/255) blocks the authored BGSM alpha_test_ref
**Location**: `byroredux/src/asset_provider/material.rs:1038-1042`.

The `#1985`-seeded synthesized alpha-test threshold gates on `!mesh.alpha_test`, which arrives pre-set from the NIF F4SF2 bit-25 path (`crates/nif/src/import/material/dedicated_shader.rs:283`) — not chain-local. When both are present, the synthesized 128/255 threshold wins and the authored BGSM `alpha_test_ref` never lands. The BGEM sibling at `material.rs:1152` overwrites unconditionally (chain-local), making BGSM the outlier — inverting #1592's documented priority (NIF flag is strictly lower priority than the BGSM merge).

**Fix**: add a `set_alpha_test` chain-local sentinel so authored BGSM `alpha_test_ref` wins over the synthesized NIF-flag threshold, matching every other payload-carrying field in the loop.

## #2213 — NIFAL-D9-01: completeness harness alphabetical truncation collapses each game's sample to one directory
**Location**: `crates/nif/tests/translation_completeness.rs:110,236-237`.

`files.sort(); files.truncate(SAMPLE_LIMIT)` (200) — alphabetical order means the 200-file window never leaves the first top-level directory (Skyrim 100% from `meshes\actors\`, Oblivion 100% from `meshes\architecture\`). The sort itself is correct/deliberate (#1279 — deterministic vs BA2 HashMap order); the defect is truncating after sort without stratification.

**Fix**: stratified sampling — round-robin across top-level directories before truncation, preserving deterministic ordering.

## #2214 — NIFAL-D9-02: completeness harness measures the raw tier — translate_material is never called
**Location**: `crates/nif/tests/translation_completeness.rs:145,224-254`.

`MaterialStats::record` takes `&ImportedMesh`; `translate_material` (the actual NIFAL boundary) is never called anywhere in the harness — no canonical type is ever constructed. Root cause: crate-graph constraint (`crates/nif` sits below `byroredux`, where `translate_material` lives), so the harness physically cannot reach the canonical tier from where it is.

**Fix**: add a canonical-tier sibling harness in `byroredux/tests/` that drives `translate_material` and asserts on canonical `Material` output rather than `Imported*` fill rates. Keep the existing raw-tier harness as-is (measures a different thing).
