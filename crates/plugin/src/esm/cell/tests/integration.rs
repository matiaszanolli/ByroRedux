//! Real ESM integration tests.
//!
//! Walks FNV.esm, Oblivion.esm, Fallout3.esm, Skyrim.esm, Fallout4.esm and
//! confirms cell / SCOL / PKIN / lighting / worldspace surfaces. Also pins
//! the `read_zstring` helper.

use super::super::helpers::read_zstring;
use super::super::*;

#[test]
#[ignore]
fn parse_real_fnv_esm() {
    let path = crate::esm::test_paths::fnv_esm();
    if !path.exists() {
        eprintln!("Skipping: FalloutNV.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = parse_esm_cells(&data).unwrap();

    eprintln!("Interior cells: {}", index.cells.len());
    eprintln!("Static objects: {}", index.statics.len());

    // Should have hundreds of interior cells and thousands of statics.
    assert!(
        index.cells.len() > 100,
        "Expected >100 cells, got {}",
        index.cells.len()
    );
    assert!(
        index.statics.len() > 1000,
        "Expected >1000 statics, got {}",
        index.statics.len()
    );

    // Check which cells have refs.
    let cells_with_refs = index
        .cells
        .values()
        .filter(|c| !c.references.is_empty())
        .count();
    eprintln!("Cells with refs: {}", cells_with_refs);

    // Check the Prospector Saloon specifically.
    let saloon = index.cells.get("gsprospectorsalooninterior").unwrap();
    eprintln!("Saloon: {} refs", saloon.references.len());
    assert!(
        saloon.references.len() > 100,
        "Saloon should have >100 refs"
    );

    // Look for the Prospector Saloon.
    let saloon_keys: Vec<&str> = index
        .cells
        .keys()
        .filter(|k| k.contains("goodsprings") || k.contains("saloon") || k.contains("prospector"))
        .map(|k| k.as_str())
        .collect();
    eprintln!("Goodsprings/saloon cells: {:?}", saloon_keys);

    // Print a few cells for debugging.
    for (key, cell) in index.cells.iter().take(10) {
        eprintln!("  Cell '{}': {} refs", key, cell.references.len());
    }
}

/// Regression guard: proves the existing FNV-shaped XCLL parser is
/// byte-compatible with Oblivion for the fields we consume.
///
/// XCLL in Oblivion (36 bytes) and FNV (40 bytes) share an identical
/// prefix for ambient / directional colors + fog colors + fog
/// near/far + directional rotation XY + fade + clip distance. FNV
/// appends a `fog_power` float; Skyrim+ has a completely different
/// (longer) layout. Since `parse_esm_cells` only reads bytes 0-27
/// (ambient, directional, and rotation), the byte offsets work for
/// both games without any per-variant branching.
///
/// This test validates that assumption against a real `Oblivion.esm`:
/// ≥90% of interior cells must produce a populated CellLighting
/// record, and the sampled color values must land in the expected
/// 0..1 normalized float range.
#[test]
#[ignore]
fn oblivion_cells_populate_xcll_lighting() {
    let path = crate::esm::test_paths::oblivion_esm();
    if !path.exists() {
        eprintln!("Skipping: Oblivion.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let idx = parse_esm_cells(&data).expect("Oblivion walker");

    let total = idx.cells.len();
    let with_lighting = idx.cells.values().filter(|c| c.lighting.is_some()).count();
    let with_directional = idx
        .cells
        .values()
        .filter(|c| {
            c.lighting
                .as_ref()
                .is_some_and(|l| l.directional_color.iter().any(|&x| x > 0.0))
        })
        .count();

    eprintln!(
        "Oblivion.esm: {total} cells, {with_lighting} with XCLL \
             ({:.1}%), {with_directional} with non-zero directional",
        100.0 * with_lighting as f32 / total.max(1) as f32,
    );

    // Log a couple of directional samples so that any future
    // XCLL-layout regression shows up in test output as obviously
    // wrong color values or rotations.
    for (name, lit) in idx
        .cells
        .values()
        .filter_map(|c| {
            c.lighting
                .as_ref()
                .map(|l| (c.editor_id.clone(), l.clone()))
        })
        .filter(|(_, l)| l.directional_color.iter().any(|&c| c > 0.0))
        .take(2)
    {
        eprintln!(
            "  '{name}': ambient={:.3?} directional={:.3?} rot=[{:.1},{:.1}]°",
            lit.ambient,
            lit.directional_color,
            lit.directional_rotation[0].to_degrees(),
            lit.directional_rotation[1].to_degrees(),
        );

        // Sanity: normalized color channels must sit in [0, 1].
        for c in lit.ambient.iter().chain(lit.directional_color.iter()) {
            assert!(
                (0.0..=1.0).contains(c),
                "color channel {c} out of [0,1] for cell '{name}' — \
                     XCLL byte offsets may have drifted"
            );
        }
    }

    // For the parser to be considered working on Oblivion, the vast
    // majority of interior cells must produce lighting data. The
    // residual are cells that legitimately omit XCLL (wilderness
    // stubs, deleted, or inherited from a template).
    let lighting_pct = with_lighting * 100 / total.max(1);
    assert!(
        lighting_pct >= 90,
        "expected >=90% of Oblivion cells to have XCLL lighting, \
             got {with_lighting}/{total} ({lighting_pct}%)"
    );
    assert!(
        with_directional > 100,
        "expected >100 cells with non-zero directional light, got {with_directional}"
    );
}

/// Smoke test: does `parse_esm_cells` survive a real `Oblivion.esm`
/// walk now that the reader understands 20-byte headers?
///
/// This does NOT assert a cell count or that specific records
/// parsed — the FNV-shaped CELL / REFR / STAT subrecord layouts may
/// still trip over Oblivion-specific fields. It only validates
/// that the top-level walker reaches the end of the file without a
/// hard error, which is the minimum bar for future per-record
/// Oblivion work.
#[test]
#[ignore]
fn parse_real_oblivion_esm_walker_survives() {
    let path = crate::esm::test_paths::oblivion_esm();
    if !path.exists() {
        eprintln!("Skipping: Oblivion.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();

    // Sanity-check auto-detection.
    use crate::esm::reader::{EsmReader, EsmVariant};
    assert_eq!(
        EsmVariant::detect(&data),
        EsmVariant::Oblivion,
        "Oblivion.esm should auto-detect as Oblivion variant"
    );
    let mut reader = EsmReader::new(&data);
    let fh = reader.read_file_header().expect("Oblivion TES4 header");
    eprintln!(
        "Oblivion.esm: record_count={} masters={:?}",
        fh.record_count, fh.master_files
    );

    // Now run the full cell walker. We only assert it returns Ok —
    // the record contents are Phase 2 work.
    match parse_esm_cells(&data) {
        Ok(idx) => {
            eprintln!(
                "Oblivion.esm walker OK: cells={} statics={} \
                     cells_with_refs={}",
                idx.cells.len(),
                idx.statics.len(),
                idx.cells
                    .values()
                    .filter(|c| !c.references.is_empty())
                    .count(),
            );
        }
        Err(e) => panic!("parse_esm_cells failed on Oblivion.esm: {e:#}"),
    }
}

/// Regression bench for #456: pin the Megaton Player House parse-
/// side reference count. ROADMAP originally quoted "1609 entities,
/// 199 textures at 42 FPS" for MegatonPlayerHouse; the 1609 figure
/// was measured AFTER cell-loader NIF expansion (each REFR spawns
/// N ECS entities depending on its NIF block tree), so it isn't
/// a parse-side assertion.
///
/// Disk-sampled on 2026-04-19 against Fallout 3 GOTY: 929 REFRs
/// live directly in the CELL. That's the stable number the
/// parser must not drop. The 42 FPS figure predates TAA / SVGF /
/// BLAS batching / streaming RIS and needs a fresh GPU bench —
/// tracked in #456.
#[test]
#[ignore]
fn parse_real_fo3_megaton_cell_baseline() {
    let path = crate::esm::test_paths::fo3_esm();
    if !path.exists() {
        eprintln!("Skipping: Fallout3.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = parse_esm_cells(&data).expect("parse_esm_cells");
    let megaton = index
        .cells
        .iter()
        .find(|(k, _)| k.contains("megaton") && k.contains("player"))
        .expect("expected a Megaton Player House interior cell in Fallout3.esm")
        .1;
    eprintln!(
        "MegatonPlayerHouse: {} REFRs (observed 929 on 2026-04-19)",
        megaton.references.len(),
    );
    assert!(
        megaton.references.len() > 800,
        "expected >800 REFRs for MegatonPlayerHouse (observed 929), got {}",
        megaton.references.len()
    );
}

/// Validates that `parse_esm_cells` handles Skyrim SE's 92-byte XCLL
/// sub-records and can find The Winking Skeever interior cell.
#[test]
#[ignore]
fn parse_real_skyrim_esm() {
    let path = crate::esm::test_paths::skyrim_se_esm();
    if !path.exists() {
        eprintln!("Skipping: Skyrim.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let idx = parse_esm_cells(&data).expect("Skyrim.esm walker");

    eprintln!(
        "Skyrim.esm: {} cells, {} statics, {} worldspaces",
        idx.cells.len(),
        idx.statics.len(),
        idx.exterior_cells.len(),
    );

    // The Winking Skeever must exist.
    let skeever = idx.cells.get("solitudewinkingskeever");
    assert!(
        skeever.is_some(),
        "SolitudeWinkingSkeever not found in Skyrim.esm cells. \
             Available keys (sample): {:?}",
        idx.cells.keys().take(20).collect::<Vec<_>>()
    );
    let skeever = skeever.unwrap();
    eprintln!(
        "Winking Skeever: {} refs, lighting={:?}",
        skeever.references.len(),
        skeever.lighting.is_some()
    );
    assert!(
        skeever.references.len() > 50,
        "Winking Skeever should have >50 refs, got {}",
        skeever.references.len()
    );

    // Skyrim XCLL should populate the extended fields.
    if let Some(ref lit) = skeever.lighting {
        eprintln!(
            "  ambient={:.3?} directional={:.3?} fog_near={:.1} fog_far={:.1}",
            lit.ambient, lit.directional_color, lit.fog_near, lit.fog_far,
        );
        // Skyrim's 92-byte XCLL must populate directional_fade.
        assert!(
            lit.directional_fade.is_some(),
            "Skyrim XCLL should have directional_fade (92-byte layout)"
        );
        // Ambient should be non-zero for a tavern interior.
        assert!(
            lit.ambient.iter().any(|&c| c > 0.0),
            "Winking Skeever ambient should be non-zero"
        );
    }

    // Check overall Skyrim cell stats.
    let with_lighting = idx.cells.values().filter(|c| c.lighting.is_some()).count();
    let with_skyrim_xcll = idx
        .cells
        .values()
        .filter(|c| {
            c.lighting
                .as_ref()
                .is_some_and(|l| l.directional_fade.is_some())
        })
        .count();
    eprintln!(
        "Skyrim lighting: {with_lighting}/{} cells with XCLL, \
             {with_skyrim_xcll} with Skyrim extended fields",
        idx.cells.len()
    );
}

#[test]
fn read_zstring_handles_null_terminator() {
    assert_eq!(read_zstring(b"Hello\0"), "Hello");
    assert_eq!(read_zstring(b"NoNull"), "NoNull");
    assert_eq!(read_zstring(b"\0"), "");
    assert_eq!(read_zstring(b""), "");
}

/// Regression: #405 — vanilla Fallout4.esm must surface every SCOL
/// record with its full ONAM/DATA child-placement data. Pre-fix
/// the MODL-only parser discarded 15,878 placement entries across
/// 2617 SCOL records. The exact counts drift with DLC patches;
/// this test just asserts we're in the right order of magnitude.
#[test]
#[ignore]
fn parse_real_fo4_esm_surfaces_scol_placements() {
    let path = crate::esm::test_paths::fo4_esm();
    if !path.exists() {
        eprintln!("Skipping: Fallout4.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let idx = parse_esm_cells(&data).expect("parse_esm_cells");

    let total_placements: usize = idx
        .scols
        .values()
        .flat_map(|s| s.parts.iter())
        .map(|p| p.placements.len())
        .sum();
    let scol_count = idx.scols.len();
    let parts_count: usize = idx.scols.values().map(|s| s.parts.len()).sum();
    eprintln!(
        "FO4 SCOL: {} records, {} parts, {} total placements",
        scol_count, parts_count, total_placements,
    );

    // Audit numbers from April 2026 Fallout4.esm scan:
    //   2617 SCOL records, 15878 ONAM/DATA pairs. Floors are set
    //   ~5% below observed so the test stays stable across
    //   patches without becoming meaningless.
    assert!(
        scol_count > 2400,
        "expected >2.4k SCOL records, got {}",
        scol_count
    );
    assert!(
        parts_count > 15000,
        "expected >15k ONAM/DATA parts, got {}",
        parts_count
    );
    assert!(
        total_placements > 15000,
        "expected >15k per-child placements, got {}",
        total_placements
    );
}

/// Regression: #589 — vanilla Fallout4.esm must surface every PKIN
/// record with a non-empty `contents` list. Pre-fix 872 PKIN
/// records silently produced zero world content because they were
/// routed through the MODL-only catch-all (PKIN carries no MODL).
/// Ignored by default — opt in with `cargo test -p byroredux-plugin
/// -- --ignored`.
#[test]
#[ignore]
fn parse_real_fo4_esm_surfaces_pkin_contents() {
    let path = crate::esm::test_paths::fo4_esm();
    if !path.exists() {
        eprintln!("Skipping: Fallout4.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let idx = parse_esm_cells(&data).expect("parse_esm_cells");

    let pkin_count = idx.packins.len();
    let non_empty_pkin = idx
        .packins
        .values()
        .filter(|p| !p.contents.is_empty())
        .count();
    let total_contents: usize = idx.packins.values().map(|p| p.contents.len()).sum();
    eprintln!(
        "FO4 PKIN: {} records, {} with contents, {} total refs",
        pkin_count, non_empty_pkin, total_contents,
    );

    // Audit floor per issue body: 872 vanilla PKIN records, all
    // with non-empty `contents`. Set the floor ~5 % below observed
    // so DLC patches don't break the test.
    assert!(
        pkin_count >= 820,
        "expected ≥820 PKIN records, got {}",
        pkin_count
    );
    assert!(
        non_empty_pkin >= 820,
        "expected ≥820 PKIN records with non-empty contents, got {}",
        non_empty_pkin
    );
}

/// Real-data smoke: parses `Oblivion.esm` (if present) and asserts
/// the Tamriel WRLD record now lands in `worldspaces` with sane
/// usable bounds. Pre-#965 this map didn't exist at all. Ignored by
/// default — opt in with `cargo test -p byroredux-plugin -- --ignored`.
#[test]
#[ignore]
fn parse_real_oblivion_esm_surfaces_tamriel_worldspace() {
    let path = crate::esm::test_paths::oblivion_esm();
    if !path.exists() {
        eprintln!("Skipping: Oblivion.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let idx = parse_esm_cells(&data).expect("Oblivion walker");
    let tam = idx
        .worldspaces
        .get("tamriel")
        .expect("Tamriel WRLD record must be decoded after #965");
    eprintln!(
        "Tamriel WRLD: form {:08X}, world bounds {:?}..{:?} (cells {:?}), \
         flags 0x{:02X}, map='{}', water={:08X?}, music={:08X?}, parent={:08X?}",
        tam.form_id,
        tam.usable_min,
        tam.usable_max,
        tam.usable_cell_bounds(),
        tam.flags,
        tam.map_texture,
        tam.water_form,
        tam.default_music,
        tam.parent_worldspace,
    );
    let cell_bounds = tam
        .usable_cell_bounds()
        .expect("Tamriel must author NAM0/NAM9 in world units");
    let (min_cell, max_cell) = cell_bounds;
    assert!(
        min_cell.0 <= max_cell.0 && min_cell.1 <= max_cell.1,
        "usable_min must be SW of usable_max"
    );
    // The Cyrodiil playable region runs roughly cell (-64,-64)..(70,70).
    // Floor of 1 cell on each side keeps the assertion stable under
    // future authoring tweaks while still catching a 0-bounds regression.
    assert!(
        max_cell.0 - min_cell.0 >= 32 && max_cell.1 - min_cell.1 >= 32,
        "Tamriel bounds rectangle should span >=32 cells in each axis, got {:?}",
        cell_bounds,
    );
    assert_eq!(
        tam.parent_worldspace, None,
        "Tamriel is a root worldspace — must not author WNAM"
    );
}

/// Real-data smoke: FO3 `Wasteland` WRLD must author NAM0/NAM9. The
/// FO3 master ships a single root worldspace; later DLCs add
/// derived ones (Anchorage, Zeta). Ignored by default.
#[test]
#[ignore]
fn parse_real_fo3_esm_surfaces_wasteland_worldspace() {
    let path = crate::esm::test_paths::fo3_esm();
    if !path.exists() {
        eprintln!("Skipping: Fallout3.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let idx = parse_esm_cells(&data).expect("FO3 walker");
    let waste = idx
        .worldspaces
        .get("wasteland")
        .expect("FO3 'wasteland' WRLD must be decoded after #965");
    eprintln!(
        "FO3 Wasteland WRLD: form {:08X}, world bounds {:?}..{:?} \
         (cells {:?}), flags 0x{:02X}, parent={:08X?}, parent_flags=0x{:04X}",
        waste.form_id,
        waste.usable_min,
        waste.usable_max,
        waste.usable_cell_bounds(),
        waste.flags,
        waste.parent_worldspace,
        waste.parent_flags,
    );
    assert!(
        waste.usable_cell_bounds().is_some(),
        "FO3 Wasteland must author NAM0/NAM9"
    );
}
