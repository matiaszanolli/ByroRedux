//! Phase 0 (#6643, GUI-HOST-001) host-call census: prints, for every
//! host method an installed Scaleform interface archive calls, which movies
//! call it and with what arity. Read-only; opens no window, no GPU.
//!
//! ```text
//! cargo run -q -p byroredux-ui --example host_call_sites -- skyrim > out.tsv
//! cargo run -q -p byroredux-ui --example host_call_sites -- fo4    > out.tsv
//! cargo run -q -p byroredux-ui --example host_call_sites -- fo4dlc > out.tsv   # DLC *- Main.ba2 holotape movies
//! cargo run -q -p byroredux-ui --example host_call_sites -- dump skyrim "" <outdir>   # decompressed SWF bodies for `strings`
//! ```
//!
//! Output TSV: `method  movie  min_args  max_args  sites  result_used`
//! (`result_used` is FO4-only: call sites whose return value is not popped
//! straight away). The Skyrim arity counts GameDelegate.call's own argument
//! count (method name + scope + [callback...]); the FO4 arity is the number of
//! host arguments (method name excluded). Unresolved Skyrim sites (runtime
//! names) are printed as method `<unresolved>`.

use std::collections::BTreeMap;

use byroredux_bsa::{Ba2Archive, BsaArchive};
use swf::avm2::read::Reader;
use swf::extensions::ReadSwfExt;
use swf::avm2::types::{AbcFile, Index, Multiname, Op};
use swf::{decompress_swf, parse_swf, Tag};

#[derive(Default, Clone)]
struct Site {
    min: usize,
    max: usize,
    sites: usize,
    used: usize,
}

fn local_name(abc: &AbcFile, index: Index<Multiname>) -> Option<Vec<u8>> {
    let multiname = abc
        .constant_pool
        .multinames
        .get(index.as_u30().checked_sub(1)? as usize)?;
    let name = match multiname {
        Multiname::QName { name, .. }
        | Multiname::QNameA { name, .. }
        | Multiname::RTQName { name }
        | Multiname::RTQNameA { name }
        | Multiname::Multiname { name, .. }
        | Multiname::MultinameA { name, .. } => *name,
        _ => return None,
    };
    abc.constant_pool
        .strings
        .get(name.as_u30().checked_sub(1)? as usize)
        .cloned()
}

#[derive(Clone)]
enum V {
    Bgs,
    Str(Vec<u8>),
    Other,
}

/// Mirror of `avm2_host::method_called_with_bgs_receiver`, additionally
/// returning (method, host-arg-count, call-op-index).
fn call_after(abc: &AbcFile, ops: &[Op]) -> Option<(Vec<u8>, usize, usize)> {
    for mut stack in [vec![V::Bgs], vec![V::Other, V::Bgs]] {
        for (i, op) in ops.iter().take(32).enumerate() {
            match op {
                Op::PushString { value } => {
                    let v = abc
                        .constant_pool
                        .strings
                        .get(value.as_u30().checked_sub(1)? as usize)?
                        .clone();
                    stack.push(V::Str(v));
                }
                Op::GetLocal { .. }
                | Op::GetLex { .. }
                | Op::FindPropStrict { .. }
                | Op::GetGlobalScope
                | Op::GetScopeObject { .. }
                | Op::PushByte { .. }
                | Op::PushShort { .. }
                | Op::PushInt { .. }
                | Op::PushUint { .. }
                | Op::PushDouble { .. }
                | Op::PushTrue
                | Op::PushFalse
                | Op::PushNull
                | Op::PushUndefined
                | Op::PushNaN => stack.push(V::Other),
                Op::GetProperty { .. } | Op::GetSlot { .. } => {
                    stack.pop()?;
                    stack.push(V::Other);
                }
                Op::SetLocal { .. } | Op::Pop => {
                    stack.pop()?;
                }
                Op::Dup => stack.push(stack.last()?.clone()),
                Op::Swap => {
                    let n = stack.len();
                    if n < 2 {
                        break;
                    }
                    stack.swap(n - 1, n - 2);
                }
                Op::CallProperty { index, num_args } | Op::CallPropVoid { index, num_args } => {
                    let num_args = *num_args as usize;
                    if stack.len() < num_args + 1 {
                        break;
                    }
                    let args = stack.split_off(stack.len() - num_args);
                    let receiver = stack.pop()?;
                    let name = local_name(abc, *index)?;
                    if matches!(receiver, V::Bgs) && name != b"call" {
                        return Some((name, num_args, i));
                    }
                    if name == b"call" && matches!(args.first(), Some(V::Bgs)) {
                        if let Some(V::Str(m)) = args.get(1) {
                            return Some((m.clone(), num_args.saturating_sub(2), i));
                        }
                    }
                    if matches!(op, Op::CallProperty { .. }) {
                        stack.push(V::Other);
                    }
                }
                Op::Coerce { .. }
                | Op::CoerceA
                | Op::CoerceB
                | Op::CoerceD
                | Op::CoerceI
                | Op::CoerceO
                | Op::CoerceS
                | Op::CoerceU
                | Op::ConvertB
                | Op::ConvertD
                | Op::ConvertI
                | Op::ConvertO
                | Op::ConvertS
                | Op::ConvertU
                | Op::AsType { .. } => {}
                _ => break,
            }
        }
    }
    None
}

fn scan_avm2(swf_data: &[u8]) -> Result<BTreeMap<String, Site>, String> {
    let dec = decompress_swf(swf_data).map_err(|e| e.to_string())?;
    let movie = parse_swf(&dec).map_err(|e| e.to_string())?;
    let mut out: BTreeMap<String, Site> = BTreeMap::new();
    for tag in &movie.tags {
        let data = match tag {
            Tag::DoAbc(d) => *d,
            Tag::DoAbc2(d) => d.data,
            _ => continue,
        };
        let abc = Reader::new(data).read().map_err(|e| e.to_string())?;
        for body in &abc.method_bodies {
            let mut reader = Reader::new(&body.code);
            let mut ops = Vec::new();
            while !reader.as_slice().is_empty() {
                ops.push(reader.read_op().map_err(|e| e.to_string())?);
            }
            for (idx, op) in ops.iter().enumerate() {
                let recv = match op {
                    Op::GetProperty { index }
                    | Op::GetLex { index }
                    | Op::FindPropStrict { index } => Some(*index),
                    _ => None,
                };
                if !recv.is_some_and(|r| local_name(&abc, r).as_deref() == Some(b"BGSCodeObj")) {
                    continue;
                }
                let Some((method, nargs, rel)) = call_after(&abc, &ops[idx + 1..]) else {
                    continue;
                };
                let Ok(method) = String::from_utf8(method) else {
                    continue;
                };
                let call_op = &ops[idx + 1 + rel];
                let used = matches!(call_op, Op::CallProperty { .. })
                    && !matches!(ops.get(idx + 2 + rel), Some(Op::Pop));
                let s = out.entry(method).or_insert(Site {
                    min: usize::MAX,
                    ..Default::default()
                });
                s.min = s.min.min(nargs);
                s.max = s.max.max(nargs);
                s.sites += 1;
                s.used += used as usize;
            }
        }
    }
    Ok(out)
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    println!("method\tmovie\tmin_args\tmax_args\tsites\tresult_used");
    match which.as_str() {
        "skyrim" => {
            let p = "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Interface.bsa";
            let a = BsaArchive::open(std::path::Path::new(p)).expect("open bsa");
            let mut files: Vec<String> = a
                .list_files()
                .into_iter()
                .filter(|f| f.to_ascii_lowercase().ends_with(".swf"))
                .map(str::to_string)
                .collect();
            files.sort();
            for f in files {
                let swf = a.extract(&f).expect("extract");
                let inv = byroredux_ui::avm1_host::referenced_host_methods(&swf).expect("scan");
                for m in &inv.methods {
                    let (lo, hi) = inv.arg_counts.get(m).copied().unwrap_or((0, 0));
                    println!("{m}\t{f}\t{lo}\t{hi}\t-\t-");
                }
                if inv.unresolved > 0 {
                    println!("<unresolved>\t{f}\t-\t-\t{}\t-", inv.unresolved);
                }
            }
        }
        "fo4" | "fo4dlc" => {
            let dir = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/";
            let archives: Vec<String> = if which == "fo4" {
                vec![format!("{dir}Fallout4 - Interface.ba2")]
            } else {
                ["DLCCoast", "DLCNukaWorld", "DLCRobot", "DLCworkshop01", "DLCworkshop02", "DLCworkshop03"]
                    .iter()
                    .map(|n| format!("{dir}{n} - Main.ba2"))
                    .collect()
            };
            for p in archives {
            let a = Ba2Archive::open(std::path::Path::new(&p)).expect("open ba2");
            let mut files: Vec<String> = a
                .list_files()
                .into_iter()
                .filter(|f| f.ends_with(".swf"))
                .map(str::to_string)
                .collect();
            files.sort();
            for f in files {
                let swf = a.extract(&f).expect("extract");
                match scan_avm2(&swf) {
                    Ok(m) => {
                        for (name, s) in m {
                            println!("{name}\t{f}\t{}\t{}\t{}\t{}", s.min, s.max, s.sites, s.used);
                        }
                    }
                    Err(e) => eprintln!("{f}: {e}"),
                }
            }
            }
        }
        // dump <skyrim|fo4> <path-substring> <outdir>: write the decompressed
        // SWFs whose path contains the substring, for `strings`-level review.
        "dump" => {
            let game = std::env::args().nth(2).unwrap_or_default();
            let needle = std::env::args().nth(3).unwrap_or_default().to_ascii_lowercase();
            let out = std::env::args().nth(4).unwrap_or_default();
            let dir = "/mnt/data/SteamLibrary/steamapps/common/";
            let (names, extract): (Vec<String>, Box<dyn Fn(&str) -> Vec<u8>>) = if game == "skyrim" {
                let a = BsaArchive::open(std::path::Path::new(&format!("{dir}Skyrim Special Edition/Data/Skyrim - Interface.bsa"))).expect("bsa");
                let n = a.list_files().into_iter().map(str::to_string).collect();
                (n, Box::new(move |f| a.extract(f).expect("extract")))
            } else {
                let a = Ba2Archive::open(std::path::Path::new(&format!("{dir}Fallout 4/Data/Fallout4 - Interface.ba2"))).expect("ba2");
                let n = a.list_files().into_iter().map(str::to_string).collect();
                (n, Box::new(move |f| a.extract(f).expect("extract")))
            };
            for f in names {
                let lf = f.to_ascii_lowercase();
                if lf.ends_with(".swf") && lf.contains(&needle) {
                    let raw = extract(&f);
                    let dec = decompress_swf(&raw[..]).expect("decompress");
                    let name = lf.replace(['\\', '/'], "__");
                    std::fs::write(format!("{out}/{name}"), &dec.data).unwrap();
                    println!("{name}");
                }
            }
        }
        _ => eprintln!("usage: host_call_sites skyrim|fo4|fo4dlc|dump"),
    }
}
