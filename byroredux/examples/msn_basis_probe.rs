//! #3922 evidence harness — per-vertex correlation of a mesh's `_msn`
//! model-space normal map against its own imported (renderer Y-up) vertex
//! normals, under candidate source→renderer basis transforms.
//!
//! For every vertex with a UV, nearest-neighbour-samples the mesh's own
//! `_msn` map (BC1 / BC3 / uncompressed RGBA8; BC7 is reported and skipped),
//! decodes the texel, applies each candidate transform, and reports the mean
//! cosine against that vertex's imported normal. The candidate that wins by
//! a wide margin is the basis Bethesda's exporter wrote — the quantity the
//! shader's `MAT_FLAG_MODEL_SPACE_NORMALS` branch must correct for.
//!
//! Skyrim meshes resolve their normal map from the NIF texture set (the
//! `model_space_normals` shader flag); FO4 meshes resolve it from the
//! BGSM material file, so pass the Materials BA2 as the third argument.
//!
//! Usage:
//! ```bash
//! cargo run --release -p byroredux --example msn_basis_probe -- \
//!     <mesh-archive> <resource-archive> [<resource-archive>...] -- \
//!     <mesh-path> [...]
//! ```

use byroredux_bsa::{Ba2Archive, BsaArchive};

struct Archives(Vec<AnyArchive>);

impl Archives {
    fn extract(&self, path: &str) -> Option<Vec<u8>> {
        self.0.iter().find_map(|a| a.extract(path))
    }
}

enum AnyArchive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl AnyArchive {
    fn open(path: &str) -> AnyArchive {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".ba2") {
            AnyArchive::Ba2(Ba2Archive::open(path).expect("open BA2"))
        } else {
            AnyArchive::Bsa(BsaArchive::open(path).expect("open BSA"))
        }
    }

    fn extract(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            AnyArchive::Bsa(a) => a.extract(path).ok(),
            AnyArchive::Ba2(a) => a.extract(path).ok(),
        }
    }
}

/// Minimal DDS mip-0 decoder for the formats `_msn` maps ship in:
/// uncompressed RGBA8, BC1 (DXT1), BC3 (DXT5). Returns RGB as f32 triples
/// in 0..1.
fn decode_dds_rgb(bytes: &[u8]) -> Result<(u32, u32, Vec<[f32; 3]>), String> {
    if bytes.len() < 128 || &bytes[0..4] != b"DDS " {
        return Err("not a DDS".into());
    }
    let h = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let height = h(12);
    let width = h(16);
    let mut fourcc = bytes[84..88].to_vec();
    let has_fourcc = h(80) & 0x4 != 0;
    let mut data_start = 128usize;
    let mut uncompressed_bgra = false;
    if has_fourcc && fourcc == b"DX10" {
        // FO4 tags even classic formats with the DX10 extension: dxgi
        // format at +0, pixel data at +20.
        let dxgi = h(128);
        data_start = 148;
        match dxgi {
            // BC1 family (71=TYPELESS, 72=UNORM, 73=UNORM_SRGB)
            71 | 72 | 73 => fourcc = b"DXT1".to_vec(),
            // BC3 family (77..79)
            77 | 78 | 79 => fourcc = b"DXT5".to_vec(),
            // B8G8R8A8_UNORM (87) / _SRGB (91): uncompressed, BGRA order
            87 | 91 => uncompressed_bgra = true,
            // BC2 (74..76) unhandled (not in the _msn corpus);
            // BC6H/BC7 (95..100) unsupported.
            _ => return Err(format!("DX10 format {dxgi} unsupported")),
        }
    }
    if fourcc == b"DXT1" || fourcc == b"BC1" {
        let (w, ht) = (width.max(4), height.max(4));
        let bw = w / 4;
        let bh = ht / 4;
        let mut out = vec![[0f32; 3]; (w * ht) as usize];
        let mut off = data_start;
        for by in 0..bh {
            for bx in 0..bw {
                let block = &bytes[off..off + 8];
                off += 8;
                decode_bc1_block_into(block, bx * 4, by * 4, w, &mut out);
            }
        }
        return Ok((width, height, out));
    }
    if fourcc == b"DXT3" || fourcc == b"DXT5" || fourcc == b"BC3" {
        let (w, ht) = (width.max(4), height.max(4));
        let bw = w / 4;
        let bh = ht / 4;
        let mut out = vec![[0f32; 3]; (w * ht) as usize];
        let mut off = data_start;
        for by in 0..bh {
            for bx in 0..bw {
                let block = &bytes[off..off + 16];
                off += 16;
                decode_bc1_block_into(&block[8..16], bx * 4, by * 4, w, &mut out);
            }
        }
        return Ok((width, height, out));
    }
    if !has_fourcc || uncompressed_bgra {
        let bit_count = h(88);
        if bit_count != 32 {
            return Err(format!("uncompressed DDS with {bit_count} bits unsupported"));
        }
        let mut out = Vec::with_capacity((width * height) as usize);
        let mut off = data_start;
        for _ in 0..width * height {
            let b = &bytes[off..off + 4];
            off += 4;
            let (r, g, bl) = if uncompressed_bgra {
                (b[2], b[1], b[0])
            } else {
                (b[0], b[1], b[2])
            };
            out.push([r as f32 / 255.0, g as f32 / 255.0, bl as f32 / 255.0]);
        }
        return Ok((width, height, out));
    }
    Err(format!(
        "unsupported DDS (fourcc={fourcc:?}, bits={})",
        h(88)
    ))
}

fn unpack565(v: u16) -> [u8; 3] {
    let r = ((v >> 11) & 0x1f) as u32;
    let g = ((v >> 5) & 0x3f) as u32;
    let b = (v & 0x1f) as u32;
    [
        ((r * 255 + 15) / 31) as u8,
        ((g * 255 + 31) / 63) as u8,
        ((b * 255 + 15) / 31) as u8,
    ]
}

fn decode_bc1_block_into(block: &[u8], x0: u32, y0: u32, width: u32, out: &mut [[f32; 3]]) {
    let c0 = unpack565(u16::from_le_bytes([block[0], block[1]]));
    let c1 = unpack565(u16::from_le_bytes([block[2], block[3]]));
    let mut palette = [[0u8; 3]; 4];
    palette[0] = c0;
    palette[1] = c1;
    let mix = |a: [u8; 3], b: [u8; 3], wa: u32, wb: u32| {
        [
            ((a[0] as u32 * wa + b[0] as u32 * wb) / (wa + wb)) as u8,
            ((a[1] as u32 * wa + b[1] as u32 * wb) / (wa + wb)) as u8,
            ((a[2] as u32 * wa + b[2] as u32 * wb) / (wa + wb)) as u8,
        ]
    };
    if u16::from_le_bytes([block[0], block[1]]) > u16::from_le_bytes([block[2], block[3]]) {
        palette[2] = mix(c0, c1, 2, 1);
        palette[3] = mix(c0, c1, 1, 2);
    } else {
        palette[2] = mix(c0, c1, 1, 1);
        palette[3] = [0, 0, 0];
    }
    let bits = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    for py in 0..4u32 {
        for px in 0..4u32 {
            let idx = ((bits >> (2 * (py * 4 + px))) & 0x3) as usize;
            let c = palette[idx];
            out[((y0 + py) * width + x0 + px) as usize] =
                [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
        }
    }
}

/// Candidate source→renderer transforms. The renderer's vertex import is
/// `(x, y, z) → (x, z, −y)` (`zup_point_to_yup`); the question #3922 asks is
/// which mapping the *texel* needs.
fn candidates() -> [(&'static str, [f32; 3]); 4] {
    [
        ("identity (shader today)", [1.0, 1.0, 1.0]),
        ("(x, y, -z)", [1.0, 1.0, -1.0]),
        ("(x, z, -y)", [1.0, -1.0, 1.0]), // applied as (x,z,-y): sign slot maps below
        ("(-x, -y, -z)", [-1.0, -1.0, -1.0]),
    ]
}

fn apply(name: &str, t: [f32; 3], v: [f32; 3]) -> [f32; 3] {
    match name {
        "(x, z, -y)" => [v[0], v[2], -v[1]],
        _ => [t[0] * v[0], t[1] * v[1], t[2] * v[2]],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(sep) = args.iter().position(|a| a == "--") else {
        eprintln!(
            "usage: msn_basis_probe <mesh-archive> <resource-archive> \
             [<resource-archive>...] -- <mesh-path> [...]"
        );
        std::process::exit(2);
    };
    if sep < 2 || args.len() < sep + 2 {
        eprintln!("need at least one mesh archive, one resource archive, and one mesh");
        std::process::exit(2);
    }
    let mesh_arch = AnyArchive::open(&args[0]);
    let resources = Archives(
        args[1..sep]
            .iter()
            .map(|p| AnyArchive::open(p))
            .collect::<Vec<_>>(),
    );
    let mesh_paths = &args[sep + 1..];

    let mut pool = byroredux_core::string::StringPool::new();
    for path in mesh_paths {
        println!("=== {path}");
        let Some(bytes) = mesh_arch.extract(path) else {
            println!("  !! mesh not found in archive");
            continue;
        };
        let scene = match byroredux_nif::parse_nif(&bytes) {
            Ok(s) => s,
            Err(e) => {
                println!("  !! parse failed: {e}");
                continue;
            }
        };
        let imported = byroredux_nif::import::import_nif(&scene, &mut pool);
        for mesh in imported.iter() {
            let name = mesh.name.as_deref().unwrap_or("(unnamed)");
            // Resolve the normal-map path: NIF texture set first, then the
            // BGSM when the mesh is FO4 (material-driven).
            let mut normal_path: Option<String> = mesh
                .material
                .textures
                .normal
                .as_ref()
                .and_then(|f| pool.resolve(*f).map(str::to_string));
            let mut msn = mesh.material.model_space_normals;
            if normal_path.is_none() {
                if let Some(mp) = mesh.material.material_path {
                    if let Some(bgsm_bytes) = resources.extract(
                        &pool.resolve(mp).map(str::to_string).unwrap_or_default(),
                    ) {
                        if let Ok(bgsm) = byroredux_bgsm::parse_bgsm(&bgsm_bytes) {
                            msn = msn || bgsm.model_space_normals;
                            if !bgsm.normal_texture.is_empty() {
                                normal_path = Some(bgsm.normal_texture.replace('/', "\\"));
                            }
                        }
                    }
                }
            }
            let Some(normal_path) = normal_path else {
                println!("  {name}: no normal map — skipped");
                continue;
            };
            let name_msn = normal_path.to_lowercase().contains("_msn");
            if !msn && !name_msn {
                println!("  {name}: normal map not model-space — skipped ({normal_path})");
                continue;
            }
            if mesh.uvs.len() != mesh.normals.len() || mesh.uvs.is_empty() {
                println!("  {name}: no UVs — skipped");
                continue;
            }
            let tex_bytes = resources
                .extract(&normal_path)
                .or_else(|| {
                    normal_path
                        .strip_prefix("data\\")
                        .map(str::to_string)
                        .and_then(|p| resources.extract(&p))
                })
                .or_else(|| {
                    let p = normal_path
                        .strip_prefix("data\\textures\\")
                        .unwrap_or(normal_path.strip_prefix("textures\\").unwrap_or(&normal_path));
                    resources.extract(&format!("textures\\{p}"))
                })
                .or_else(|| resources.extract(&format!("{normal_path}.dds")));
            let Some(tex_bytes) = tex_bytes else {
                println!("  {name}: texture {normal_path} not found — skipped");
                continue;
            };
            let (w, h, texels) = match decode_dds_rgb(&tex_bytes) {
                Ok(v) => v,
                Err(e) => {
                    println!("  {name}: {normal_path}: {e} — skipped");
                    continue;
                }
            };
            // Per-vertex correlation under each candidate, both V
            // orientations (a global V flip affects all candidates equally
            // and would bury the signal; the better orientation is reported).
            // Blue is treated two ways: raw (authored-Z maps) and
            // reconstructed `+sqrt(1-r²-g²)` the way the shader's
            // no-authored-Z arm does — the two `_msn` classes need
            // different simulations to be read correctly.
            let mut sums = [[0f64; 4]; 2];
            let mut recon_sums = [[0f64; 4]; 2];
            let mut counts = [0u64; 2];
            let mut channel_mean = [0f64; 3];
            let mut blue_max = 0f32;
            for (uv, n) in mesh.uvs.iter().zip(mesh.normals.iter()) {
                let nlen = n.iter().map(|c| c * c).sum::<f32>().sqrt();
                if nlen < 1e-5 {
                    continue;
                }
                for (vflip, flip) in [(0usize, 1.0f32), (1, -1.0f32)] {
                    let u = (((uv[0] - uv[0].floor()) * w as f32) as u32).min(w - 1);
                    let vv = (((uv[1] * flip - (uv[1] * flip).floor()) + 1.0).fract() * h as f32)
                        as u32;
                    let vv = vv.min(h - 1);
                    let t = texels[(vv * w + u) as usize];
                    channel_mean[0] += t[0] as f64;
                    channel_mean[1] += t[1] as f64;
                    channel_mean[2] += t[2] as f64;
                    blue_max = blue_max.max(t[2]);
                    let decoded = [t[0] * 2.0 - 1.0, t[1] * 2.0 - 1.0, t[2] * 2.0 - 1.0];
                    let reconstructed = [
                        decoded[0],
                        decoded[1],
                        (1.0 - decoded[0] * decoded[0] - decoded[1] * decoded[1])
                            .max(0.0)
                            .sqrt(),
                    ];
                    let eval = |c: [f32; 3]| {
                        let clen = c.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-5);
                        (c[0] * n[0] + c[1] * n[1] + c[2] * n[2]) as f64 / (clen * nlen) as f64
                    };
                    for (ci, (cname, csign)) in candidates().iter().enumerate() {
                        sums[vflip][ci] += eval(apply(cname, *csign, decoded));
                        recon_sums[vflip][ci] += eval(apply(cname, *csign, reconstructed));
                    }
                    counts[vflip] += 1;
                }
            }
            if counts[0] == 0 {
                println!("  {name}: no valid samples — skipped");
                continue;
            }
            let n = counts[0] as f64;
            let blue_unauthored = blue_max < 0.01;
            println!(
                "  {name}: {normal_path} ({}x{}, msn_flag={msn}, blue {})",
                w,
                h,
                if blue_unauthored { "UNAUTHORED (constant zero)" } else { "authored" }
            );
            println!(
                "    decoded channel means: ({:.3}, {:.3}, {:.3})",
                channel_mean[0] / n,
                channel_mean[1] / n,
                channel_mean[2] / n
            );
            let primary = if blue_unauthored { recon_sums } else { sums };
            for (vflip, label) in [(0usize, "v as-is"), (1usize, "v flipped")] {
                let parts: Vec<String> = candidates()
                    .iter()
                    .enumerate()
                    .map(|(ci, (cname, _))| {
                        format!("{cname}: {:+.3}", primary[vflip][ci] / counts[vflip] as f64)
                    })
                    .collect();
                println!("    [{}] {}", label, parts.join("  "));
            }
        }
    }
}
