//! Dump decoded `NAVM` navmesh tiles, or sweep a master for decode coverage.
//!
//! The #2738 verification tool: a decoded tile can be eyeballed against the
//! cell it belongs to before anything consumes it, and the sweep reports how
//! much of a plugin's navmesh actually decoded — which is what established
//! the Creation-Engine `NVNM` layout in the first place (see `parse_navm`).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_navmesh -- <file.esm>
//!   cargo run -p byroredux-plugin --example dump_navmesh -- <file.esm> <FORMID_HEX>

use byroredux_plugin::esm::reader::EsmReader;
use byroredux_plugin::esm::records::misc::parse_navm;
use byroredux_plugin::esm::records::NavmRecord;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: dump_navmesh <file.esm> [FORMID_HEX]"))?;
    let selector = args
        .next()
        .map(|raw| u32::from_str_radix(raw.trim_start_matches("0x"), 16))
        .transpose()?;

    let bytes = std::fs::read(&path)?;
    let mut reader = EsmReader::new(&bytes);
    let _ = reader.read_file_header()?;

    let mut meshes = Vec::new();
    collect(&mut reader, bytes.len(), &mut meshes)?;

    match selector {
        Some(form_id) => dump_one(&meshes, form_id),
        None => sweep(&path, &meshes),
    }
    Ok(())
}

/// Per-plugin decode coverage. The columns that matter for #2738: how many
/// tiles yielded geometry, how many kept an undecoded blob, and how many
/// know which cell or grid they belong to.
fn sweep(path: &str, meshes: &[NavmRecord]) {
    let total = meshes.len();
    if total == 0 {
        println!("{path}: no NAVM records");
        return;
    }
    let decoded = meshes.iter().filter(|m| m.has_decoded_geometry()).count();
    let packed = meshes
        .iter()
        .filter(|m| m.packed_geometry.is_some())
        .count();
    let located = meshes
        .iter()
        .filter(|m| m.cell_form.is_some() || m.grid.is_some())
        .count();
    let in_range = meshes
        .iter()
        .filter(|m| m.has_decoded_geometry() && m.indices_are_in_range())
        .count();
    let linked: usize = meshes.iter().map(|m| m.linked_meshes().len()).sum();
    let vertices: usize = meshes.iter().map(|m| m.vertices.len()).sum();
    let triangles: usize = meshes.iter().map(|m| m.triangles.len()).sum();

    println!("{path}: {total} NAVM");
    println!("  geometry decoded   {decoded:>6}/{total}");
    println!("  blob retained      {packed:>6}/{total}  (body layout not recognised)");
    println!("  cell or grid known {located:>6}/{total}");
    println!("  indices in range   {in_range:>6}/{decoded}");
    println!("  {vertices} vertices, {triangles} triangles, {linked} external links");

    // Hand the caller something to pass back as the FormID argument — an
    // exterior tile with links (the interesting case) where one exists.
    if let Some(sample) = meshes
        .iter()
        .find(|m| m.grid.is_some() && !m.external_connections.is_empty())
        .or_else(|| meshes.first())
    {
        println!(
            "  sample: dump_navmesh <esm> {:08X}   (grid {:?}, {} links)",
            sample.form_id,
            sample.grid,
            sample.external_connections.len()
        );
    }
}

fn dump_one(meshes: &[NavmRecord], form_id: u32) {
    let Some(mesh) = meshes.iter().find(|m| m.form_id == form_id) else {
        println!("no NAVM {form_id:08X} in this plugin");
        return;
    };
    println!("== NAVM {:08X} {} ==", mesh.form_id, mesh.editor_id);
    println!("  version        {}", mesh.version);
    println!("  worldspace     {:08X?}", mesh.worldspace_form);
    println!("  cell           {:08X?}", mesh.cell_form);
    println!("  grid           {:?}", mesh.grid);
    println!(
        "  vertices       {} ({}decoded)",
        mesh.vertices.len(),
        if mesh.has_decoded_geometry() {
            ""
        } else {
            "not "
        }
    );
    println!("  triangles      {}", mesh.triangles.len());
    println!("  external links {}", mesh.external_connections.len());
    if let Some(blob) = &mesh.packed_geometry {
        println!("  packed blob    {} B (body not decoded)", blob.len());
    }
    for (index, tri) in mesh.triangles.iter().take(8).enumerate() {
        println!(
            "    tri {index:>3} verts {:?} edges {:?} flags {:08X}",
            tri.vertices, tri.edge_neighbours, tri.flags
        );
    }
    for conn in mesh.external_connections.iter().take(8) {
        println!(
            "    link → mesh {:08X} triangle {}",
            conn.mesh_form, conn.triangle
        );
    }
}

fn collect(
    reader: &mut EsmReader<'_>,
    end: usize,
    out: &mut Vec<NavmRecord>,
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner = reader.group_content_end(&group);
            collect(reader, inner, out)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type != b"NAVM" {
            reader.skip_record(&header);
            continue;
        }
        let subs = reader.read_sub_records(&header)?;
        out.push(parse_navm(header.form_id, &subs));
    }
    Ok(())
}
