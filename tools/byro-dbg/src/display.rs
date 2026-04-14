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
            draw_call_count,
        } => {
            println!(
                "FPS: {:.1} (avg {:.1}) | Frame: {:.2}ms | Entities: {} | Meshes: {} | Textures: {} | Draws: {}",
                fps, avg_fps, frame_time_ms, entity_count, mesh_count, texture_count, draw_call_count
            );
        }
        DebugResponse::Screenshot { png_base64: _, width: _, height: _ } => {
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
