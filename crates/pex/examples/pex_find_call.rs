//! List every call site of one function name across all `.pex` in a BSA/BA2 — answers "who calls
//! `ShowRaceMenu`?" without decompiling the whole corpus by hand.
//!
//! ```bash
//! cargo run --release -p byroredux-pex --example pex_find_call -- \
//!     "<Skyrim Special Edition>/Data/Skyrim - Misc.bsa" ShowRaceMenu
//! ```
//!
//! An optional third argument limits the scan to archive entries whose path contains that substring
//! (case-insensitive); the function name `-` lists the matching `.pex` paths instead.
//!
//! Prints `script  object  scope  instr#  target::function  argc`, one line per call site. Matching
//! is case-insensitive on the function name; archive entries that fail to parse are counted on stderr.

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_pex::{parse, CallScope, CallTarget};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: pex_find_call <archive> <function>");
    let want = args.next().expect("usage: pex_find_call <archive> <function>").to_ascii_lowercase();
    let only = args.next().map(|s| s.to_ascii_lowercase());
    let (files, extract): (Vec<String>, Box<dyn Fn(&str) -> std::io::Result<Vec<u8>>>) =
        if path.to_ascii_lowercase().ends_with(".ba2") {
            let a = Ba2Archive::open(&path).unwrap();
            let f = a.list_files().into_iter().map(String::from).collect();
            (f, Box::new(move |p| a.extract(p)))
        } else {
            let a = BsaArchive::open(&path).unwrap();
            let f = a.list_files().into_iter().map(String::from).collect();
            (f, Box::new(move |p| a.extract(p)))
        };
    let (mut scanned, mut failed) = (0usize, 0usize);
    for f in files.iter().filter(|f| f.to_ascii_lowercase().ends_with(".pex")) {
        if only.as_ref().is_some_and(|o| !f.to_ascii_lowercase().contains(o)) {
            continue;
        }
        if want == "-" {
            println!("{f}");
            continue;
        }
        let Ok(bytes) = extract(f) else { failed += 1; continue };
        let Ok(pex) = parse(&bytes) else { failed += 1; continue };
        scanned += 1;
        for c in pex.call_sites().calls {
            if c.function.to_ascii_lowercase() != want {
                continue;
            }
            let scope = match &c.scope {
                CallScope::StateFunction { state, function } => format!("state:{state}/fn:{function}"),
                CallScope::PropertyGetter { property } => format!("get:{property}"),
                CallScope::PropertySetter { property } => format!("set:{property}"),
            };
            let target = match &c.target {
                CallTarget::StaticType(t) => format!("static {t}"),
                CallTarget::Receiver(r) => format!("recv {}", r.as_deref().unwrap_or("?")),
                CallTarget::ParentType(t) => format!("parent {t}"),
            };
            println!("{}\t{}\t{}\t#{}\t{}::{}\targc={}", c.source_file, c.object, scope, c.instruction_index, target, c.function, c.argument_count);
        }
    }
    eprintln!("scanned {scanned} pex, {failed} failed");
}
