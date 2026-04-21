//! byro-dbg — standalone CLI debugger for ByroRedux.
//!
//! Connects to the engine's debug server over TCP and provides an
//! interactive REPL for inspecting and modifying ECS state.

mod display;

use byroredux_debug_protocol::{wire, DebugRequest, DebugResponse, DEFAULT_PORT};
use std::io::{self, BufRead, Write};
use std::net::TcpStream;

fn main() {
    let host = std::env::var("BYRO_DEBUG_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("BYRO_DEBUG_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let addr = format!("{}:{}", host, port);

    eprintln!("Connecting to ByroRedux at {}...", addr);
    let mut stream = match TcpStream::connect(&addr) {
        Ok(s) => {
            eprintln!("Connected.\n");
            s
        }
        Err(e) => {
            eprintln!("Failed to connect: {}", e);
            eprintln!("Is the engine running with debug-server enabled?");
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("byro> ");
        stdout.flush().unwrap();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap() == 0 {
            break; // EOF
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Client-side commands. Bare `quit` / `exit` / `q` close the
        // REPL same as the dotted forms — more discoverable than
        // requiring the user to know the `.` prefix. The dotted forms
        // stay supported in case a future command with the same name
        // ships on the engine side (#518 SIBLING).
        match line {
            ".quit" | ".exit" | ".q" | "quit" | "exit" | "q" => break,
            ".help" => {
                display::print_help();
                continue;
            }
            _ => {}
        }

        // Map shorthand commands to protocol requests
        let request = match parse_shorthand(line) {
            Some(req) => req,
            None => DebugRequest::Eval {
                expr: line.to_string(),
            },
        };

        if let Err(e) = wire::send(&mut stream, &request) {
            eprintln!("Send error: {}", e);
            break;
        }

        match wire::decode::<DebugResponse>(&mut stream) {
            Ok(response) => display::print_response(&response),
            Err(e) => {
                eprintln!("Receive error: {}", e);
                break;
            }
        }
    }
}

/// Recognize shorthand commands that map to specific protocol requests.
fn parse_shorthand(input: &str) -> Option<DebugRequest> {
    let lower = input.to_ascii_lowercase();
    match lower.as_str() {
        "ping" => Some(DebugRequest::Ping),
        "stats" => Some(DebugRequest::Stats),
        "components" => Some(DebugRequest::ListComponents),
        "systems" => Some(DebugRequest::ListSystems),
        "screenshot" => Some(DebugRequest::Screenshot { path: None }),
        _ => {
            if lower.starts_with("screenshot ") {
                let path = input["screenshot".len()..].trim().to_string();
                return Some(DebugRequest::Screenshot {
                    path: if path.is_empty() { None } else { Some(path) },
                });
            }
            if lower.starts_with("entities") {
                let arg = input["entities".len()..].trim();
                let component = if arg.is_empty() {
                    None
                } else {
                    // Strip optional parens: "entities(Transform)" or "entities Transform"
                    let arg = arg.trim_start_matches('(').trim_end_matches(')').trim();
                    Some(arg.to_string())
                };
                Some(DebugRequest::ListEntities { component })
            } else {
                None
            }
        }
    }
}
