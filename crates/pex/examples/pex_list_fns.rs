//! List every function of one `.pex` file with its typed parameters — a quick way to read an event
//! signature table (e.g. `Quest.pex`'s `OnStory*` events) straight from the game's compiled scripts.
//!
//! ```bash
//! cargo run -p byroredux-pex --example pex_list_fns -- quest.pex OnStory
//! ```

use byroredux_pex::parse;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).expect("read pex");
    let filter = args.get(2).map(String::as_str).unwrap_or("");
    let pex = parse(&bytes).expect("parse pex");
    for object in &pex.objects {
        for state in &object.states {
            for function in &state.functions {
                if !function.name.contains(filter) {
                    continue;
                }
                let params: Vec<String> = function.params.iter().map(|p| format!("{} {}", p.type_name, p.name)).collect();
                println!("{}({})", function.name, params.join(", "));
            }
        }
    }
}
