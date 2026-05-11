//! Response pretty-printing for the debug CLI.

use byroredux_debug_protocol::DebugResponse;

pub fn print_response(response: &DebugResponse) {
    match response {
        DebugResponse::Value { data } => {
            println!("{}", serde_json::to_string_pretty(data).unwrap());
        }
        DebugResponse::EntityList { entities } => {
            if entities.is_empty() {
                println!("(no entities)");
                return;
            }
            for e in entities {
                match &e.name {
                    Some(name) => println!("  Entity {} \"{}\"", e.id, name),
                    None => println!("  Entity {}", e.id),
                }
            }
            println!("({} entities)", entities.len());
        }
        DebugResponse::ComponentList { components } => {
            for name in components {
                println!("  {}", name);
            }
            println!("({} components)", components.len());
        }
        DebugResponse::SystemList { systems } => {
            for (i, name) in systems.iter().enumerate() {
                println!("  [{}] {}", i, name);
            }
        }
        DebugResponse::Stats {
            fps,
            avg_fps,
            frame_time_ms,
            entity_count,
            mesh_count,
            texture_count,
            meshes_in_use,
            textures_in_use,
            draw_call_count,
        } => {
            // #637 / FNV-D5-02 — show registry-wide AND scene-scoped
            // counts for meshes / textures so a leak that holds the
            // last reference past cell unload is visible as
            // `<registry>` larger than `<in_use>`.
            println!(
                "FPS: {:.1} (avg {:.1}) | Frame: {:.2}ms | Entities: {} | \
                 Meshes: {}/{} | Textures: {}/{} | Draws: {} (registry/in-use)",
                fps,
                avg_fps,
                frame_time_ms,
                entity_count,
                mesh_count,
                meshes_in_use,
                texture_count,
                textures_in_use,
                draw_call_count
            );
        }
        DebugResponse::Screenshot {
            png_base64: _,
            width: _,
            height: _,
        } => {
            println!("Screenshot captured (raw data)");
        }
        DebugResponse::ScreenshotSaved { path } => {
            println!("Screenshot saved: {}", path);
        }
        DebugResponse::Ok => {
            println!("OK");
        }
        DebugResponse::Pong => {
            println!("pong");
        }
        DebugResponse::Hierarchy { nodes } => {
            if nodes.is_empty() {
                println!("(empty hierarchy)");
                return;
            }
            println!(
                "{:>5} {:>5} {:>6} {:>30} {:>30} {:>5}  name",
                "depth", "id", "parent", "GT.t", "local.t", "flags"
            );
            for n in nodes {
                let fmt_t = |t: &Option<[f32; 3]>| -> String {
                    t.map_or("                             ?".into(), |v| {
                        format!("({:8.1},{:8.1},{:8.1})", v[0], v[1], v[2])
                    })
                };
                let mut flags = String::new();
                if n.has_skinned_mesh {
                    flags.push('S');
                }
                if n.has_mesh_handle {
                    flags.push('M');
                }
                let parent = n.parent.map_or("·".to_string(), |p| p.to_string());
                let indent = "  ".repeat(n.depth as usize);
                let name = n.name.as_deref().unwrap_or("");
                println!(
                    "{:>5} {:>5} {:>6} {:>30} {:>30} {:>5}  {}{}",
                    n.depth,
                    n.id,
                    parent,
                    fmt_t(&n.gt_translation),
                    fmt_t(&n.local_translation),
                    flags,
                    indent,
                    name,
                );
            }
            println!("({} nodes)", nodes.len());
        }
        DebugResponse::SkinnedMesh {
            skeleton_root,
            bones,
            bone_names,
            bind_inverses,
            global_skin_transform: _,
            bone_world_matrices,
            palette,
        } => {
            println!("skeleton_root: {:?}", skeleton_root);
            println!(
                "{} bones (col 1 bind.t · col 2 world.t · col 3 palette.t · ✗ = identity):",
                bones.len()
            );
            // Column 4x4 matrix translation lives at indices 12/13/14.
            let t_of = |m: &[f32; 16]| (m[12], m[13], m[14]);
            let is_identity = |m: &[f32; 16]| {
                (m[0] - 1.0).abs() < 1e-5
                    && (m[5] - 1.0).abs() < 1e-5
                    && (m[10] - 1.0).abs() < 1e-5
                    && (m[15] - 1.0).abs() < 1e-5
                    && m[12].abs() < 1e-5
                    && m[13].abs() < 1e-5
                    && m[14].abs() < 1e-5
            };
            let mut dropouts = 0usize;
            for (i, ((((b, name), bind), world), pal)) in bones
                .iter()
                .zip(bone_names.iter())
                .zip(bind_inverses.iter())
                .zip(bone_world_matrices.iter())
                .zip(palette.iter())
                .enumerate()
            {
                let nm = name.as_deref().unwrap_or("?");
                let bt = t_of(bind);
                let wt = world.map(|m| t_of(&m));
                let pt = t_of(pal);
                let pal_drop = is_identity(pal);
                if pal_drop {
                    dropouts += 1;
                }
                let world_str = match wt {
                    Some((x, y, z)) => format!("({:>7.2},{:>7.2},{:>7.2})", x, y, z),
                    None => "(   none           )".to_string(),
                };
                println!(
                    "  [{:>3}] entity={:>10} name={:<24} bind=({:>7.2},{:>7.2},{:>7.2}) world={} pal=({:>7.2},{:>7.2},{:>7.2}){}",
                    i,
                    b.map(|e| e.to_string()).unwrap_or_else(|| "None".to_string()),
                    format!("{:?}", nm),
                    bt.0, bt.1, bt.2,
                    world_str,
                    pt.0, pt.1, pt.2,
                    if pal_drop { "  ✗" } else { "" }
                );
            }
            if dropouts > 0 {
                println!(
                    "({} of {} palette slots are IDENTITY — likely #841 dropout)",
                    dropouts,
                    bones.len()
                );
            }
        }
        DebugResponse::Error { message } => {
            eprintln!("Error: {}", message);
        }
    }
}

pub fn print_help() {
    println!("byro-dbg — ByroRedux Debug CLI");
    println!();
    println!("Built-in commands:");
    println!("  stats              Engine stats (FPS, entities, draws)");
    println!("  components         List inspectable component types");
    println!("  systems            List registered ECS systems");
    println!("  entities           List all entities with names");
    println!("  entities(Comp)     List entities with a specific component");
    println!("  screenshot          Capture screenshot (auto-named)");
    println!("  screenshot path    Capture screenshot to specific file");
    println!("  skin <id>          Dump SkinnedMesh palette + per-bone world (#841 diag)");
    println!("  ping               Check connection");
    println!("  .help              This help");
    println!("  .quit              Exit");
    println!();
    println!("Expression queries:");
    println!("  find(\"name\")       Find entity by name");
    println!("  42.Transform       Get component on entity 42");
    println!("  42.Transform.translation.x   Get a specific field");
    println!("  42.Transform.translation.x = 100.0   Set a field");
}
