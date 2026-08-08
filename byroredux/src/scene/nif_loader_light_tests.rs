//! Regression test for #2530 / NIFAL-D3-NEW-01 — the loose-NIF load
//! path (`parse_import_and_merge`, backing every `load_nif_bytes` /
//! `load_nif_bytes_with_skeleton` call including standalone `cargo run
//! -- <mesh>.nif` and every skeleton/body/hand NPC-part load) never
//! called `byroredux_nif::import::import_nif_lights`, so a torch /
//! candle / lantern / streetlamp NIF loaded through this path rendered
//! its geometry but contributed zero light to the scene.
//!
//! The ECS half of the fix (`ImportedLight` → spawned `LightSource`) is
//! covered by `cell_loader::nif_light_spawn_gate_tests`'s
//! `spawn_nif_lights_attaches_light_source_for_spawnable_light` — that
//! test exercises the exact function `load_nif_bytes_with_skeleton` now
//! calls. What's missing from THAT coverage is proof `parse_import_and_
//! merge` actually makes the call at all; `import_nif_lights` has no
//! byte-level NIF fixture elsewhere in the tree to build a full parse-
//! through-spawn integration test from (and `load_nif_bytes_with_
//! skeleton` itself needs a real Vulkan device for the GPU-upload half
//! regardless — the established "can't unit test this function
//! directly" constraint seen throughout this crate). Pinned at the
//! source level instead, from a SEPARATE file to avoid the self-
//! reference trap where the assertion's own string would match its own
//! `include_str!` (see `cell_loader/precombined_clip_handle_tests.rs`
//! for the identical technique on a different fix).

#[test]
fn parse_import_and_merge_extracts_lights_from_the_raw_scene() {
    let src = include_str!("nif_loader.rs");
    assert!(
        src.contains("imported.lights = byroredux_nif::import::import_nif_lights(&scene)"),
        "parse_import_and_merge must extract authored lights from the raw NifScene \
         while it's still in scope and store them on the returned ImportedScene \
         (#2530) — without this, every loose-loaded / NPC-part-loaded NIF's \
         embedded lights are silently dropped"
    );
}

#[test]
fn load_nif_bytes_with_skeleton_spawns_the_extracted_lights() {
    let src = include_str!("nif_loader.rs");
    assert!(
        src.contains("cell_loader::spawn::spawn_nif_lights("),
        "load_nif_bytes_with_skeleton must spawn a LightSource for each of \
         imported.lights (#2530) — extracting them in parse_import_and_merge \
         alone is not sufficient if nothing ever spawns them"
    );
    assert!(
        src.contains("&imported.lights"),
        "the spawn call must be fed the lights parse_import_and_merge \
         extracted, not an empty/unrelated array"
    );
}
