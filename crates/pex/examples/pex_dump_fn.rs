//! Dump one function's raw `.pex` bytecode as disassembly — the
//! diagnostic companion to `pex_corpus_smoke`'s failure list (paste the
//! script name + function the smoke reports straight through).
//!
//! ```bash
//! cargo run --release -p byroredux-pex --example pex_dump_fn -- \
//!     "<Fallout 4>/Data/Fallout4 - Misc.ba2" StimBoxScript CheckTriggeringObjectAndDoOnceStatus
//! ```

use std::fmt::Write as _;

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_pex::{parse, OpCode, Value};

fn open(path: &str) -> Box<dyn DynArchive> {
    if path.to_ascii_lowercase().ends_with(".ba2") {
        Box::new(Ba2Archive::open(path).unwrap())
    } else {
        Box::new(BsaArchive::open(path).unwrap())
    }
}

trait DynArchive {
    fn files(&self) -> Vec<String>;
    fn extract(&self, path: &str) -> std::io::Result<Vec<u8>>;
}

impl DynArchive for Ba2Archive {
    fn files(&self) -> Vec<String> {
        self.list_files().into_iter().map(String::from).collect()
    }
    fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        Ba2Archive::extract(self, path)
    }
}

impl DynArchive for BsaArchive {
    fn files(&self) -> Vec<String> {
        self.list_files().into_iter().map(String::from).collect()
    }
    fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        BsaArchive::extract(self, path)
    }
}

fn val(v: &Value) -> String {
    match v {
        Value::Integer(i) => format!("{i}"),
        Value::Float(f) => format!("{f}"),
        Value::Str(s) => format!("{s:?}"),
        Value::Identifier(s) => format!("%{s}"),
        Value::None => "none".into(),
        Value::Bool(b) => format!("{b}"),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let archive_path = args.next().expect("usage: pex_dump_fn <archive> <script-stem> [function]");
    let stem = args.next().expect("usage: pex_dump_fn <archive> <script-stem> [function]");
    let want_fn = args.next();

    let archive = open(&archive_path);
    let want = format!("{}.pex", stem.to_ascii_lowercase());
    let path = archive
        .files()
        .into_iter()
        .find(|f| {
            let lower = f.to_ascii_lowercase();
            lower.ends_with(&want) || lower.ends_with(&format!("\\{want}")) || lower.ends_with(&format!("/{want}"))
        })
        .unwrap_or_else(|| panic!("no {want} in archive"));
    let pex = parse(&archive.extract(&path).unwrap()).unwrap();

    println!("script {} type {:?}", pex.header.source_file_name, pex.script_type);
    for object in &pex.objects {
        println!("object {:?} vars:", object.name);
        for (i, v) in object.variables.iter().enumerate() {
            println!("  var#{i} {} : {}", v.name, v.type_name);
        }
        for state in &object.states {
            for f in &state.functions {
                dump_function(&format!("state {:?} fn {}", state.name, f.name), &f.name, f, &want_fn);
            }
        }
        // #5883 — a property's getter / setter bodies decompile too (via
        // `build_named_function`), so a smoke failure inside one must be
        // dumpable: they are named `<prop>.Get` / `<prop>.Set`.
        for prop in &object.properties {
            for (suffix, f) in [("Get", &prop.read_function), ("Set", &prop.write_function)] {
                if let Some(f) = f {
                    let name = format!("{}.{suffix}", prop.name);
                    dump_function(&format!("property {name}"), &name, f, &want_fn);
                }
            }
        }
    }
}

/// Print one function's header and disassembly unless `want_fn` names a
/// different function (case-insensitive match on `name`).
fn dump_function(label: &str, name: &str, f: &byroredux_pex::Function, want_fn: &Option<String>) {
    if let Some(want) = want_fn {
        if !name.eq_ignore_ascii_case(want) {
            return;
        }
    }
    println!("\n== {label} ({})", f.return_type_name);
    println!("   params: {:?}", f.params.iter().map(|p| (&p.name, &p.type_name)).collect::<Vec<_>>());
    println!("   locals: {:?}", f.locals.iter().map(|l| (&l.name, &l.type_name)).collect::<Vec<_>>());
    for (i, ins) in f.instructions.iter().enumerate() {
        let mut line = format!("  {i:3} {:<18}", mnemonic(ins.op));
        for a in &ins.args {
            write!(line, " {}", val(a)).unwrap();
        }
        if !ins.var_args.is_empty() {
            line.push_str(" |");
            for a in &ins.var_args {
                write!(line, " {}", val(a)).unwrap();
            }
        }
        println!("{line}");
    }
}

fn mnemonic(op: OpCode) -> &'static str {
    op.name()
}
