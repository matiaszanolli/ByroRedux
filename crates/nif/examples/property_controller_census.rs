//! Census controller chains hosted on property blocks across an archive.
//!
//! Blast-radius harness for keying property-hosted embedded animation by the
//! owning shape (WATAL W2): runtime channels bind by entity `Name`, and
//! entities are shapes/nodes, so a channel keyed by a property's own name
//! never resolved. This counts, per archive, how many property-hosted
//! controllers exist, whether their property is named, and how many owners
//! the importer now binds them to — i.e. how much previously-dead animation
//! the fix turns on.
//!
//! Usage:
//!   cargo run --release -p byroredux-nif --example property_controller_census -- <bsa|ba2> [...]

use byroredux_nif::blocks::controller::BsShaderController;
use byroredux_nif::blocks::properties::{NiMaterialProperty, NiTexturingProperty};
use byroredux_nif::blocks::shader::{BSEffectShaderProperty, BSLightingShaderProperty};
use std::collections::BTreeMap;

fn main() {
    for archive_path in std::env::args().skip(1) {
        let archive = match byroredux_bsa::BsaArchive::open(&archive_path) {
            Ok(a) => a,
            Err(e) => {
                println!("{archive_path}: open failed: {e}");
                continue;
            }
        };
        let mut files = 0usize;
        let mut hosting_files = 0usize;
        let mut named = 0usize;
        let mut unnamed = 0usize;
        let mut channels_after = 0usize;
        let mut by_host_controller: BTreeMap<String, usize> = BTreeMap::new();
        let mut examples: Vec<String> = Vec::new();

        for path in archive.list_files() {
            if !path.to_ascii_lowercase().ends_with(".nif") {
                continue;
            }
            let Ok(bytes) = archive.extract(path) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            files += 1;

            let mut hosts_here = 0usize;
            for block in &scene.blocks {
                let any = block.as_any();
                let (host, net) = if let Some(p) = any.downcast_ref::<BSEffectShaderProperty>() {
                    ("BSEffectShaderProperty", &p.net)
                } else if let Some(p) = any.downcast_ref::<BSLightingShaderProperty>() {
                    ("BSLightingShaderProperty", &p.net)
                } else if let Some(p) = any.downcast_ref::<NiMaterialProperty>() {
                    ("NiMaterialProperty", &p.net)
                } else if let Some(p) = any.downcast_ref::<NiTexturingProperty>() {
                    ("NiTexturingProperty", &p.net)
                } else {
                    continue;
                };
                let Some(ctrl_idx) = net.controller_ref.index() else {
                    continue;
                };
                hosts_here += 1;
                if net.name.as_deref().is_some_and(|n| !n.is_empty()) {
                    named += 1;
                } else {
                    unnamed += 1;
                }
                let ctrl = scene
                    .blocks
                    .get(ctrl_idx)
                    .map(|c| {
                        c.as_any()
                            .downcast_ref::<BsShaderController>()
                            .map(|s| format!("{} {:?}", s.type_name, s.kind))
                            .unwrap_or_else(|| c.block_type_name().to_string())
                    })
                    .unwrap_or_else(|| "?".into());
                *by_host_controller
                    .entry(format!("{host} ← {ctrl}"))
                    .or_default() += 1;
            }
            if hosts_here == 0 {
                continue;
            }
            hosting_files += 1;

            // Channels the production importer emits now, versus the subset
            // it could have emitted before (only from non-property hosts).
            if let Some(clip) = byroredux_nif::anim::import_embedded_animations(&scene) {
                let n = clip.float_channels.len()
                    + clip.color_channels.len()
                    + clip.bool_channels.len()
                    + clip.texture_flip_channels.len();
                channels_after += n;
                if examples.len() < 12 {
                    examples.push(format!("{path}: {n} channels"));
                }
            }
        }

        println!("=== {archive_path}");
        println!(
            "  nifs parsed {files}; nifs with property-hosted controllers {hosting_files}; \
             hosting properties named {named} / unnamed {unnamed}"
        );
        println!("  non-transform channels emitted by those nifs now: {channels_after}");
        for (k, n) in &by_host_controller {
            println!("  {n:>6}  {k}");
        }
        for e in &examples {
            println!("  e.g. {e}");
        }
    }
}
