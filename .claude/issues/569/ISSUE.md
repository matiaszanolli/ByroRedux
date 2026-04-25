# SK-D2-01: No committed full-archive extraction sweep for Skyrim SE BSAs

State: OPEN

## Severity
**LOW** — Test-coverage gap. BSA v105 LZ4 path is functionally correct (verified end-to-end by extracting `sweetroll01.nif`), but no gated regression test exercises a real Skyrim SE archive.

## Location
- `crates/bsa/src/archive.rs:501-842` (test module)

## Description
The test module has FNV-only fixture constants:
```rust
const FNV_MESHES_BSA: &str = "/mnt/data/SteamLibrary/.../Fallout New Vegas/Data/Fallout - Meshes.bsa";
```

No equivalent constant or `#[ignore]`-gated test for any Skyrim SE archive. The only v105 exercise is the ad-hoc `bsa_extract_one.rs` example. If the v105 LZ4 frame path regresses, nothing gates it.

## Impact
- Cannot regression-test BSA v105 changes against real Skyrim data.
- Any accidental change to the frame-decoder dispatch, 24-byte folder record size, or u64 offset read would slip through CI.

## Suggested Fix
Add `#[ignore]`d sister tests mirroring the existing FNV pattern:

```rust
const SKYRIM_MESHES_BSA: &str =
    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Meshes0.bsa";
const SKYRIM_TEXTURES_BSA: &str =
    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Textures0.bsa";

#[test]
#[ignore]
fn skyrim_meshes0_contains_sweetroll() {
    let Some(path) = skip_if_missing(SKYRIM_MESHES_BSA) else { return; };
    // assert file_count > 18000, extract sweetroll, verify NIF magic
}

#[test]
#[ignore]
fn skyrim_textures0_extracts_dds() {
    // assert DDS magic on first file
}
```

Follow the existing `skip_if_missing()` pattern so CI without Steam installed stays green.

## Completeness Checks
- [ ] **SIBLING**: Apply the same pattern for any other v105 archive we expect to load (Dawnguard.bsa, Dragonborn.bsa, HearthFires.bsa — DLC sibling archives).
- [ ] **TESTS**: Assert file count (~18,862 for Meshes0) to catch regressions in folder/file record counting.
- [ ] **TESTS**: Assert the LZ4 frame decoder roundtrips a known-size file (sweetroll: 10,245 bytes).

## Source
Audit `docs/audits/AUDIT_SKYRIM_2026-04-22.md` finding **SK-D2-01**.
