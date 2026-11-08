//! Phase 0 (#6643, GUI-HUD-003 / GUI-HUD-005) measurement tool: reads a vanilla
//! Scaleform HUD movie out of the installed game archive and reports which
//! members the movie's OWN bytecode defines, calls, assigns and reads — the
//! difference being the surface the original engine (native code) feeds.
//! Read-only; opens no window, no GPU.
//!
//! ```text
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- skyrim listing            # archive entries
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- skyrim dump  [swf-name]   # AVM1 pseudo-source
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- skyrim members [swf-name] # per-member table (TSV)
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- skyrim tree  [swf-name]   # timeline instance tree
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- fo4 dump                  # AVM2 class dump
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- fo4 members               # per-member table (TSV)
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- skyrim fed | fo4 fed       # declared but never called/written by movie code
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- fo4 strings <swf> <needle>  # constant-pool strings
//! cargo run -q -p byroredux-ui --example hud_feed_paths -- fo4 refs <swf> <needle>     # every get/set/call site of a member name
//! ```
//!
//! The AVM1 "pseudo-source" is a symbolic stack walk (no control-flow
//! structuring): every `SetMember`/`SetVariable`/`CallMethod`/`CallFunction`/
//! `GetMember` is rendered as `receiver.member`, with registers substituted
//! by the expression stored in them. It is a reading aid; the `members`
//! table is built from the same events.

use std::collections::{BTreeMap, BTreeSet};

use byroredux_bsa::{Ba2Archive, BsaArchive};
use swf::avm1::read::Reader as Avm1Reader;
use swf::avm1::types::{Action, FunctionFlags, Value};
use swf::extensions::ReadSwfExt;
use swf::{decompress_swf, parse_swf, PlaceObjectAction, Tag};

const SKYRIM_BSA: &str =
    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Interface.bsa";
const FO4_BA2: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/Fallout4 - Interface.ba2";

// ───────────────────────────── AVM1 ─────────────────────────────

#[derive(Clone, Debug)]
struct Ex {
    s: String,
    lit: bool,
}

impl Ex {
    fn lit(s: &str) -> Self {
        Ex { s: s.to_string(), lit: true }
    }
    fn ex(s: impl Into<String>) -> Self {
        Ex { s: s.into(), lit: false }
    }
    fn shown(&self) -> String {
        if self.lit {
            format!("{:?}", self.s)
        } else {
            self.s.clone()
        }
    }
    /// Bare identifier for variable/member names.
    fn name(&self) -> String {
        self.s.clone()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Set,
    Call,
    Get,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // `block` is kept for ad-hoc debugging of an event's origin
struct Event {
    kind: Kind,
    func: usize, // 0 = top-level block
    obj: String,
    member: String,
    value: String,
    block: String,
}

#[derive(Default)]
struct Func {
    name: String, // as declared in DefineFunction (may be empty)
    label: String, // best name learned from an assignment
    params: Vec<String>,
    block: String,
    lines: Vec<String>,
}

#[derive(Default)]
struct Ctx {
    funcs: Vec<Func>, // index 0 reserved for top level
    events: Vec<Event>,
    top_lines: Vec<(String, Vec<String>)>,
}

fn unq(e: &Ex) -> String {
    e.name()
}

fn walk(
    ctx: &mut Ctx,
    code: &[u8],
    version: u8,
    pool: &[String],
    func: usize,
    block: &str,
    regs_init: Vec<(usize, String)>,
    lines: &mut Vec<String>,
    depth: usize,
) {
    if depth > 24 {
        return;
    }
    let mut pool = pool.to_vec();
    let mut stack: Vec<Ex> = Vec::new();
    let mut regs: Vec<Option<Ex>> = vec![None; 256];
    for (r, n) in regs_init {
        regs[r] = Some(Ex::ex(n));
    }
    let mut reader = Avm1Reader::new(code, version);
    let total = code.len();
    macro_rules! pop {
        () => {
            stack.pop().unwrap_or_else(|| Ex::ex("<?>"))
        };
    }
    loop {
        let pos = total - reader.as_slice().len();
        let Ok(action) = reader.read_action() else { break };
        let emit = |lines: &mut Vec<String>, s: String| lines.push(format!("{pos:04x}: {s}"));
        match action {
            Action::End => break,
            Action::ConstantPool(c) => {
                pool = c.strings.iter().map(|e| e.to_string_lossy(swf::UTF_8)).collect();
            }
            Action::Push(p) => {
                for v in &p.values {
                    stack.push(match v {
                        Value::Str(t) => Ex::lit(&t.to_string_lossy(swf::UTF_8)),
                        Value::ConstantPool(i) => match pool.get(*i as usize) {
                            Some(s) => Ex::lit(s),
                            None => Ex::ex(format!("<pool#{i}>")),
                        },
                        Value::Int(i) => Ex::ex(i.to_string()),
                        Value::Float(f) => Ex::ex(f.to_string()),
                        Value::Double(f) => Ex::ex(f.to_string()),
                        Value::Bool(b) => Ex::ex(b.to_string()),
                        Value::Null => Ex::ex("null"),
                        Value::Undefined => Ex::ex("undefined"),
                        Value::Register(r) => regs[*r as usize].clone().unwrap_or_else(|| Ex::ex(format!("r{r}"))),
                    });
                }
            }
            Action::GetVariable => {
                let n = pop!();
                let v = unq(&n);
                ctx.events.push(Event { kind: Kind::Get, func, obj: String::new(), member: v.clone(), value: String::new(), block: block.into() });
                stack.push(Ex::ex(v));
            }
            Action::GetMember => {
                let m = pop!();
                let o = pop!();
                let (os, ms) = (o.shown_obj(), unq(&m));
                ctx.events.push(Event { kind: Kind::Get, func, obj: os.clone(), member: ms.clone(), value: String::new(), block: block.into() });
                stack.push(Ex::ex(if m.lit { format!("{os}.{ms}") } else { format!("{os}[{ms}]") }));
            }
            Action::SetMember => {
                let v = pop!();
                let m = pop!();
                let o = pop!();
                let (os, ms, vs) = (o.shown_obj(), unq(&m), v.shown());
                ctx.events.push(Event { kind: Kind::Set, func, obj: os.clone(), member: ms.clone(), value: vs.clone(), block: block.into() });
                if let Some(id) = vs.strip_prefix("<fn#").and_then(|r| r.strip_suffix('>')).and_then(|r| r.parse::<usize>().ok()) {
                    if ctx.funcs[id].label.is_empty() {
                        ctx.funcs[id].label = format!("{os}.{ms}");
                    }
                }
                emit(lines, format!("{os}.{ms} = {vs}"));
            }
            Action::SetVariable => {
                let v = pop!();
                let n = pop!();
                let (ns, vs) = (unq(&n), v.shown());
                ctx.events.push(Event { kind: Kind::Set, func, obj: String::new(), member: ns.clone(), value: vs.clone(), block: block.into() });
                if let Some(id) = vs.strip_prefix("<fn#").and_then(|r| r.strip_suffix('>')).and_then(|r| r.parse::<usize>().ok()) {
                    if ctx.funcs[id].label.is_empty() {
                        ctx.funcs[id].label = ns.clone();
                    }
                }
                emit(lines, format!("{ns} = {vs}   [SetVariable]"));
            }
            Action::DefineLocal => {
                let v = pop!();
                let n = pop!();
                emit(lines, format!("var {} = {}", unq(&n), v.shown()));
            }
            Action::DefineLocal2 => {
                let n = pop!();
                emit(lines, format!("var {}", unq(&n)));
            }
            Action::SetProperty => {
                let v = pop!();
                let i = pop!();
                let t = pop!();
                emit(lines, format!("setProperty({}, {}, {})", t.shown(), i.shown(), v.shown()));
            }
            Action::GetProperty => {
                let i = pop!();
                let t = pop!();
                stack.push(Ex::ex(format!("getProperty({}, {})", t.shown(), i.shown())));
            }
            Action::StoreRegister(s) => {
                if let Some(top) = stack.last() {
                    regs[s.register as usize] = Some(top.clone());
                }
            }
            Action::PushDuplicate => {
                let t = stack.last().cloned().unwrap_or_else(|| Ex::ex("<?>"));
                stack.push(t);
            }
            Action::StackSwap => {
                let n = stack.len();
                if n >= 2 {
                    stack.swap(n - 1, n - 2);
                }
            }
            Action::Pop => {
                if let Some(t) = stack.pop() {
                    if t.s.contains('(') && !t.lit {
                        emit(lines, format!("{}", t.s));
                    }
                }
            }
            Action::CallMethod => {
                let m = pop!();
                let o = pop!();
                let n = pop!();
                let n: usize = n.s.parse().unwrap_or(0).min(stack.len());
                let args = stack.split_off(stack.len() - n);
                let args_s: Vec<String> = args.iter().rev().map(Ex::shown).collect();
                let (os, ms) = (o.shown_obj(), unq(&m));
                ctx.events.push(Event { kind: Kind::Call, func, obj: os.clone(), member: ms.clone(), value: args_s.join(", "), block: block.into() });
                stack.push(Ex::ex(format!("{os}.{ms}({})", args_s.join(", "))));
            }
            Action::CallFunction => {
                let f = pop!();
                let n = pop!();
                let n: usize = n.s.parse().unwrap_or(0).min(stack.len());
                let args = stack.split_off(stack.len() - n);
                let args_s: Vec<String> = args.iter().rev().map(Ex::shown).collect();
                ctx.events.push(Event { kind: Kind::Call, func, obj: String::new(), member: unq(&f), value: args_s.join(", "), block: block.into() });
                stack.push(Ex::ex(format!("{}({})", unq(&f), args_s.join(", "))));
            }
            Action::NewObject => {
                let c = pop!();
                let n = pop!();
                let n: usize = n.s.parse().unwrap_or(0).min(stack.len());
                let args = stack.split_off(stack.len() - n);
                let args_s: Vec<String> = args.iter().rev().map(Ex::shown).collect();
                stack.push(Ex::ex(format!("new {}({})", unq(&c), args_s.join(", "))));
            }
            Action::NewMethod => {
                let m = pop!();
                let o = pop!();
                let n = pop!();
                let n: usize = n.s.parse().unwrap_or(0).min(stack.len());
                let args = stack.split_off(stack.len() - n);
                let args_s: Vec<String> = args.iter().rev().map(Ex::shown).collect();
                stack.push(Ex::ex(format!("new {}.{}({})", o.shown_obj(), unq(&m), args_s.join(", "))));
            }
            Action::InitArray => {
                let n = pop!();
                let n: usize = n.s.parse().unwrap_or(0).min(stack.len());
                let args = stack.split_off(stack.len() - n);
                let args_s: Vec<String> = args.iter().rev().map(Ex::shown).collect();
                stack.push(Ex::ex(format!("[{}]", args_s.join(", "))));
            }
            Action::InitObject => {
                let n = pop!();
                let n: usize = n.s.parse().unwrap_or(0).min(stack.len() / 2);
                let mut parts = Vec::new();
                for _ in 0..n {
                    let v = pop!();
                    let k = pop!();
                    parts.push(format!("{}: {}", unq(&k), v.shown()));
                }
                stack.push(Ex::ex(format!("{{{}}}", parts.join(", "))));
            }
            Action::Return => {
                let v = pop!();
                emit(lines, format!("return {}", v.shown()));
            }
            Action::Throw => {
                let v = pop!();
                emit(lines, format!("throw {}", v.shown()));
            }
            Action::Trace => {
                let v = pop!();
                emit(lines, format!("trace({})", v.shown()));
            }
            Action::Not => {
                let v = pop!();
                stack.push(Ex::ex(format!("!({})", v.shown())));
            }
            Action::ToNumber | Action::ToString | Action::ToInteger | Action::TypeOf | Action::Increment | Action::Decrement => {
                let v = pop!();
                stack.push(Ex::ex(format!("{}({})", un_name(&action), v.shown())));
            }
            Action::Add | Action::Add2 | Action::And | Action::BitAnd | Action::BitLShift | Action::BitOr | Action::BitRShift
            | Action::BitURShift | Action::BitXor | Action::Divide | Action::Equals | Action::Equals2 | Action::Greater
            | Action::Less | Action::Less2 | Action::Modulo | Action::Multiply | Action::Or | Action::StrictEquals
            | Action::StringAdd | Action::StringEquals | Action::StringGreater | Action::StringLess | Action::Subtract
            | Action::InstanceOf => {
                let r = pop!();
                let l = pop!();
                stack.push(Ex::ex(format!("({} {} {})", l.shown(), bin_name(&action), r.shown())));
            }
            Action::Delete => {
                let n = pop!();
                let o = pop!();
                emit(lines, format!("delete {}.{}", o.shown_obj(), unq(&n)));
            }
            Action::Delete2 => {
                let n = pop!();
                emit(lines, format!("delete {}", unq(&n)));
            }
            Action::Extends => {
                let sup = pop!();
                let sub = pop!();
                emit(lines, format!("{} extends {}", sub.shown_obj(), sup.shown_obj()));
            }
            Action::ImplementsOp => {
                let n = pop!();
                let n: usize = n.s.parse().unwrap_or(0).min(stack.len());
                stack.truncate(stack.len() - n);
                let c = pop!();
                emit(lines, format!("{} implements ...", c.shown_obj()));
            }
            Action::CastOp => {
                let o = pop!();
                let c = pop!();
                stack.push(Ex::ex(format!("{}({})", c.shown_obj(), o.shown())));
            }
            Action::If(i) => {
                let c = pop!();
                let tgt = (reader.as_slice().len() as isize * -1 + total as isize) + i.offset as isize;
                emit(lines, format!("if ({}) goto L{:04x}", c.shown(), tgt));
            }
            Action::Jump(j) => {
                let tgt = (total - reader.as_slice().len()) as isize + j.offset as isize;
                emit(lines, format!("goto L{:04x}", tgt));
            }
            Action::DefineFunction(f) => {
                let id = ctx.funcs.len();
                let params: Vec<String> = f.params.iter().map(|p| p.to_string_lossy(swf::UTF_8)).collect();
                let name = f.name.to_string_lossy(swf::UTF_8);
                let mut sub = Vec::new();
                ctx.funcs.push(Func { name: name.clone(), params: params.clone(), block: block.into(), ..Default::default() });
                let regs0 = vec![];
                walk(ctx, f.actions, version, &pool, id, block, regs0, &mut sub, depth + 1);
                ctx.funcs[id].lines = sub;
                if name.is_empty() {
                    stack.push(Ex::ex(format!("<fn#{id}>")));
                } else {
                    ctx.funcs[id].label = name.clone();
                    emit(lines, format!("function {name}(...) = <fn#{id}>"));
                    ctx.events.push(Event { kind: Kind::Set, func, obj: String::new(), member: name, value: format!("<fn#{id}>"), block: block.into() });
                }
            }
            Action::DefineFunction2(f) => {
                let id = ctx.funcs.len();
                let params: Vec<String> = f.params.iter().map(|p| p.name.to_string_lossy(swf::UTF_8)).collect();
                let name = f.name.to_string_lossy(swf::UTF_8);
                let mut regs0: Vec<(usize, String)> = Vec::new();
                let mut next = 1usize;
                for (flag, nm) in [
                    (FunctionFlags::PRELOAD_THIS, "this"),
                    (FunctionFlags::PRELOAD_ARGUMENTS, "arguments"),
                    (FunctionFlags::PRELOAD_SUPER, "super"),
                    (FunctionFlags::PRELOAD_ROOT, "_root"),
                    (FunctionFlags::PRELOAD_PARENT, "_parent"),
                    (FunctionFlags::PRELOAD_GLOBAL, "_global"),
                ] {
                    if f.flags.contains(flag) {
                        regs0.push((next, nm.to_string()));
                        next += 1;
                    }
                }
                for p in &f.params {
                    if let Some(r) = p.register_index {
                        regs0.push((r.get() as usize, p.name.to_string_lossy(swf::UTF_8)));
                    }
                }
                let mut sub = Vec::new();
                ctx.funcs.push(Func { name: name.clone(), params: params.clone(), block: block.into(), ..Default::default() });
                walk(ctx, f.actions, version, &pool, id, block, regs0, &mut sub, depth + 1);
                ctx.funcs[id].lines = sub;
                if name.is_empty() {
                    stack.push(Ex::ex(format!("<fn#{id}>")));
                } else {
                    ctx.funcs[id].label = name.clone();
                    emit(lines, format!("function {name}(...) = <fn#{id}>"));
                    ctx.events.push(Event { kind: Kind::Set, func, obj: String::new(), member: name, value: format!("<fn#{id}>"), block: block.into() });
                }
            }
            Action::With(w) => {
                let o = pop!();
                emit(lines, format!("with ({}) {{", o.shown_obj()));
                let mut sub = Vec::new();
                walk(ctx, w.actions, version, &pool, func, block, vec![], &mut sub, depth + 1);
                for l in sub {
                    lines.push(format!("    {l}"));
                }
                emit(lines, "}".into());
            }
            Action::Try(t) => {
                emit(lines, "try {".into());
                let mut sub = Vec::new();
                walk(ctx, t.try_body, version, &pool, func, block, vec![], &mut sub, depth + 1);
                for l in sub {
                    lines.push(format!("    {l}"));
                }
                emit(lines, "}".into());
            }
            Action::SetTarget(t) => emit(lines, format!("setTarget({:?})", t.target.to_string_lossy(swf::UTF_8))),
            Action::SetTarget2 => {
                let t = pop!();
                emit(lines, format!("setTarget({})", t.shown()));
            }
            Action::GotoFrame(g) => emit(lines, format!("gotoFrame({})", g.frame)),
            Action::GotoLabel(g) => emit(lines, format!("gotoLabel({:?})", g.label.to_string_lossy(swf::UTF_8))),
            Action::GotoFrame2(_) => {
                let t = pop!();
                emit(lines, format!("gotoFrame2({})", t.shown()));
            }
            Action::Play => emit(lines, "play()".into()),
            Action::Stop => emit(lines, "stop()".into()),
            Action::NextFrame => emit(lines, "nextFrame()".into()),
            Action::PreviousFrame => emit(lines, "prevFrame()".into()),
            Action::GetUrl(g) => emit(lines, format!("getURL({:?}, {:?})", g.url.to_string_lossy(swf::UTF_8), g.target.to_string_lossy(swf::UTF_8))),
            Action::GetUrl2(_) => {
                let t = pop!();
                let u = pop!();
                emit(lines, format!("getURL2({}, {})", u.shown(), t.shown()));
            }
            Action::Enumerate | Action::Enumerate2 => {
                let o = pop!();
                emit(lines, format!("enumerate({})", o.shown()));
                stack.push(Ex::ex("<enum>"));
            }
            Action::TargetPath => {
                let o = pop!();
                stack.push(Ex::ex(format!("targetPath({})", o.shown())));
            }
            Action::GetTime => stack.push(Ex::ex("getTimer()")),
            Action::RandomNumber => {
                let o = pop!();
                stack.push(Ex::ex(format!("random({})", o.shown())));
            }
            Action::StringLength | Action::MBStringLength | Action::CharToAscii | Action::AsciiToChar => {
                let o = pop!();
                stack.push(Ex::ex(format!("len/char({})", o.shown())));
            }
            Action::StringExtract | Action::MBStringExtract => {
                let _ = (pop!(), pop!(), pop!());
                stack.push(Ex::ex("substring(...)"));
            }
            Action::RemoveSprite | Action::Call | Action::StartDrag | Action::CloneSprite => {
                stack.clear();
            }
            _ => {}
        }
    }
}

trait ShownObj {
    fn shown_obj(&self) -> String;
}
impl ShownObj for Ex {
    fn shown_obj(&self) -> String {
        if self.lit {
            self.s.clone()
        } else {
            self.s.clone()
        }
    }
}

fn un_name(a: &Action) -> &'static str {
    match a {
        Action::ToNumber => "Number",
        Action::ToString => "String",
        Action::ToInteger => "int",
        Action::TypeOf => "typeof",
        Action::Increment => "inc",
        Action::Decrement => "dec",
        _ => "?",
    }
}

fn bin_name(a: &Action) -> &'static str {
    match a {
        Action::Add | Action::Add2 | Action::StringAdd => "+",
        Action::And => "&&",
        Action::BitAnd => "&",
        Action::BitLShift => "<<",
        Action::BitOr => "|",
        Action::BitRShift => ">>",
        Action::BitURShift => ">>>",
        Action::BitXor => "^",
        Action::Divide => "/",
        Action::Equals | Action::Equals2 | Action::StringEquals => "==",
        Action::Greater | Action::StringGreater => ">",
        Action::Less | Action::Less2 | Action::StringLess => "<",
        Action::Modulo => "%",
        Action::Multiply => "*",
        Action::Or => "||",
        Action::StrictEquals => "===",
        Action::Subtract => "-",
        Action::InstanceOf => "instanceof",
        _ => "?",
    }
}

/// One movie's tag walk: runs every action block (frame scripts, init
/// actions, clip actions, sprite timelines) through [`walk`], and records
/// the timeline instance tree.
struct Movie {
    ctx: Ctx,
    tree: Vec<String>,
    exports: Vec<(u16, String)>,
}

fn run_tags(m: &mut Movie, tags: &[Tag<'_>], version: u8, owner: &str, indent: usize) {
    let mut frame = 0usize;
    for tag in tags {
        match tag {
            Tag::ShowFrame => frame += 1,
            Tag::ExportAssets(list) => {
                for e in list {
                    m.exports.push((e.id, e.name.to_string_lossy(swf::UTF_8)));
                }
            }
            Tag::DoAction(code) => {
                let block = format!("{owner} frame{frame} DoAction");
                let mut lines = Vec::new();
                walk(&mut m.ctx, code, version, &[], 0, &block, vec![], &mut lines, 0);
                m.ctx.top_lines.push((block, lines));
            }
            Tag::DoInitAction { id, action_data } => {
                let block = format!("DoInitAction(sprite {id})");
                let mut lines = Vec::new();
                walk(&mut m.ctx, action_data, version, &[], 0, &block, vec![], &mut lines, 0);
                m.ctx.top_lines.push((block, lines));
            }
            Tag::PlaceObject(po) => {
                let name = po.name.map(|n| n.to_string_lossy(swf::UTF_8));
                let ch = match po.action {
                    PlaceObjectAction::Place(id) | PlaceObjectAction::Replace(id) => Some(id),
                    PlaceObjectAction::Modify => None,
                };
                if let (Some(n), Some(c)) = (&name, ch) {
                    m.tree.push(format!("{}{}.{n}\tchar {c}\tdepth {}", "  ".repeat(indent), owner, po.depth));
                }
                if let Some(ca) = &po.clip_actions {
                    for rec in &ca.records {
                        let block = format!(
                            "{owner} PlaceObject({}) clip-action {:?}",
                            name.clone().unwrap_or_default(),
                            rec.events
                        );
                        let mut lines = Vec::new();
                        walk(&mut m.ctx, rec.action_data, version, &[], 0, &block, vec![], &mut lines, 0);
                        m.ctx.top_lines.push((block, lines));
                    }
                }
            }
            Tag::DefineSprite(s) => {
                let sname = format!("sprite{}", s.id);
                run_tags(m, &s.tags, version, &sname, indent + 1);
            }
            _ => {}
        }
    }
}

fn load_movie_avm1(data: &[u8]) -> Movie {
    let dec = decompress_swf(data).expect("decompress");
    let movie = parse_swf(&dec).expect("parse");
    let mut m = Movie { ctx: Ctx::default(), tree: vec![], exports: vec![] };
    m.ctx.funcs.push(Func::default());
    run_tags(&mut m, &movie.tags, movie.header.version(), "root", 0);
    m
}

/// Replace `<fn#N>` placeholders that name a class constructor by its
/// `_global.` label, so `<fn#245>.prototype.X` reads `HUDMenu.prototype.X`.
fn resolve(m: &Movie, s: &str) -> String {
    let mut out = s.to_string();
    for (i, f) in m.ctx.funcs.iter().enumerate().skip(1) {
        if !f.label.is_empty() && !f.label.contains("<fn#") && f.label.starts_with("_global.") {
            out = out.replace(&format!("<fn#{i}>"), f.label.trim_start_matches("_global."));
        }
    }
    out
}

fn print_dump(m: &Movie) {
    for (block, lines) in &m.ctx.top_lines {
        if lines.is_empty() {
            continue;
        }
        println!("=== {block}");
        for l in lines {
            println!("  {}", resolve(m, l));
        }
    }
    println!("\n=== FUNCTIONS");
    for (i, f) in m.ctx.funcs.iter().enumerate().skip(1) {
        println!("--- fn#{i} {}({}) [{}] in {}", resolve(m, &f.label), f.params.join(", "), f.name, f.block);
        for l in &f.lines {
            println!("    {}", resolve(m, l));
        }
    }
}

/// Per-member census (by member name, ignoring the receiver — AS2 has no
/// static types, so the receiver path is shown beside it).
fn print_members(m: &Movie, only_fed: bool) {
    #[derive(Default)]
    struct Row {
        defined_as_fn: BTreeSet<String>,
        assigned: BTreeSet<String>, // non-function assignment receivers
        called: BTreeSet<String>,   // receivers
        called_in: BTreeSet<String>, // function labels doing the calls
        read: usize,
    }
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    let flabel = |id: usize| -> String {
        if id == 0 {
            "<top>".into()
        } else {
            let f = &m.ctx.funcs[id];
            if f.label.is_empty() { format!("fn#{id}") } else { resolve(m, &f.label) }
        }
    };
    for e in &m.ctx.events {
        let e = &Event { obj: resolve(m, &e.obj), ..e.clone() };
        let r = rows.entry(e.member.clone()).or_default();
        match e.kind {
            Kind::Set => {
                if e.value.starts_with("<fn#") {
                    r.defined_as_fn.insert(format!("{}.{}", e.obj, e.member));
                } else {
                    r.assigned.insert(format!("{} [in {}]", e.obj, flabel(e.func)));
                }
            }
            Kind::Call => {
                r.called.insert(e.obj.clone());
                r.called_in.insert(flabel(e.func));
            }
            Kind::Get => r.read += 1,
        }
    }
    println!("member\tdefined_as_function_at\tassigned_non_fn_by\tcalled_on\tcalled_from\treads");
    for (k, r) in rows {
        // `fed` view: declared as a function somewhere, and no movie code
        // anywhere calls a member of that name.
        if only_fed && (r.defined_as_fn.is_empty() || !r.called.is_empty()) {
            continue;
        }
        println!(
            "{k}\t{}\t{}\t{}\t{}\t{}",
            r.defined_as_fn.iter().cloned().collect::<Vec<_>>().join(" | "),
            r.assigned.iter().cloned().collect::<Vec<_>>().join(" | "),
            r.called.iter().cloned().collect::<Vec<_>>().join(" | "),
            r.called_in.iter().cloned().collect::<Vec<_>>().join(" | "),
            r.read
        );
    }
}

// ───────────────────────────── AVM2 ─────────────────────────────

use swf::avm2::read::Reader as Avm2Reader;
use swf::avm2::types::{AbcFile, Index, Multiname, Namespace, Op, TraitKind};

fn pool_str(abc: &AbcFile, i: Index<String>) -> String {
    i.as_u30()
        .checked_sub(1)
        .and_then(|x| abc.constant_pool.strings.get(x as usize))
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

fn ns_name(abc: &AbcFile, i: Index<Namespace>) -> String {
    let Some(n) = i.as_u30().checked_sub(1).and_then(|x| abc.constant_pool.namespaces.get(x as usize)) else {
        return String::new();
    };
    match n {
        Namespace::Namespace(s)
        | Namespace::Package(s)
        | Namespace::PackageInternal(s)
        | Namespace::Protected(s)
        | Namespace::Explicit(s)
        | Namespace::StaticProtected(s)
        | Namespace::Private(s) => pool_str(abc, *s),
    }
}

/// `pkg.Name` or bare local name for a multiname.
fn mn(abc: &AbcFile, i: Index<Multiname>) -> String {
    let Some(m) = i.as_u30().checked_sub(1).and_then(|x| abc.constant_pool.multinames.get(x as usize)) else {
        return "*".into();
    };
    match m {
        Multiname::QName { namespace, name } | Multiname::QNameA { namespace, name } => {
            let ns = ns_name(abc, *namespace);
            let n = pool_str(abc, *name);
            if ns.is_empty() { n } else { format!("{ns}.{n}") }
        }
        Multiname::Multiname { name, .. } | Multiname::MultinameA { name, .. } | Multiname::RTQName { name } | Multiname::RTQNameA { name } => {
            pool_str(abc, *name)
        }
        Multiname::TypeName { base_type, .. } => format!("{}.<T>", mn(abc, *base_type)),
        _ => "<rt>".into(),
    }
}

fn local_part(s: &str) -> &str {
    s.rsplit('.').next().unwrap_or(s)
}

#[derive(Default)]
struct ClassInfo {
    name: String,
    sup: String,
    ifaces: Vec<String>,
    /// (kind, name, type/detail, is_static)
    members: Vec<(String, String, String, bool)>,
}

#[derive(Clone, Debug)]
struct Ev3 {
    kind: &'static str, // get|set|init|call|new
    owner: String,      // Class.method
    member: String,     // local member name
    via: String,        // qualified multiname
}

struct Abc3 {
    classes: Vec<ClassInfo>,
    events: Vec<Ev3>,
    strings: BTreeSet<String>,
    abc_name: String,
}

fn scan_abc(abc: &AbcFile, abc_name: &str) -> Abc3 {
    let mut out = Abc3 { classes: vec![], events: vec![], strings: BTreeSet::new(), abc_name: abc_name.into() };
    for s in &abc.constant_pool.strings {
        out.strings.insert(String::from_utf8_lossy(s).into_owned());
    }
    // method index -> owner label
    let mut owner: BTreeMap<u32, String> = BTreeMap::new();
    let trait_label = |cname: &str, tname: &str, kind: &TraitKind, stat: bool, owner: &mut BTreeMap<u32, String>| {
        let id = match kind {
            TraitKind::Method { method, .. } | TraitKind::Getter { method, .. } | TraitKind::Setter { method, .. } => Some(method.as_u30()),
            TraitKind::Function { function, .. } => Some(function.as_u30()),
            _ => None,
        };
        if let Some(id) = id {
            let pfx = match kind {
                TraitKind::Getter { .. } => "get ",
                TraitKind::Setter { .. } => "set ",
                _ => "",
            };
            owner.insert(id, format!("{cname}{}.{pfx}{tname}", if stat { "[static]" } else { "" }));
        }
    };
    for (idx, inst) in abc.instances.iter().enumerate() {
        let cname = mn(abc, inst.name);
        let mut ci = ClassInfo { name: cname.clone(), sup: mn(abc, inst.super_name), ..Default::default() };
        for i in &inst.interfaces {
            ci.ifaces.push(mn(abc, *i));
        }
        owner.insert(inst.init_method.as_u30(), format!("{cname}.<ctor>"));
        for t in &inst.traits {
            let tn = mn(abc, t.name);
            trait_label(&cname, local_part(&tn), &t.kind, false, &mut owner);
            ci.members.push(trait_desc(abc, t, &tn, false));
        }
        if let Some(cl) = abc.classes.get(idx) {
            owner.insert(cl.init_method.as_u30(), format!("{cname}.<cinit>"));
            for t in &cl.traits {
                let tn = mn(abc, t.name);
                trait_label(&cname, local_part(&tn), &t.kind, true, &mut owner);
                ci.members.push(trait_desc(abc, t, &tn, true));
            }
        }
        out.classes.push(ci);
    }
    for (si, sc) in abc.scripts.iter().enumerate() {
        owner.insert(sc.init_method.as_u30(), format!("<script{si}>"));
    }
    for body in &abc.method_bodies {
        let who = owner.get(&body.method.as_u30()).cloned().unwrap_or_else(|| format!("<anon method#{}>", body.method.as_u30()));
        let mut r = Avm2Reader::new(&body.code);
        while !r.as_slice().is_empty() {
            let Ok(op) = r.read_op() else { break };
            let (kind, idx) = match &op {
                Op::GetProperty { index } | Op::GetSuper { index } => ("get", *index),
                Op::SetProperty { index } | Op::SetSuper { index } => ("set", *index),
                Op::InitProperty { index } => ("init", *index),
                Op::CallProperty { index, .. } | Op::CallPropVoid { index, .. } | Op::CallPropLex { index, .. } | Op::CallSuper { index, .. } | Op::CallSuperVoid { index, .. } => ("call", *index),
                Op::ConstructProp { index, .. } => ("new", *index),
                Op::GetLex { index } => ("lex", *index),
                _ => continue,
            };
            let via = mn(abc, idx);
            out.events.push(Ev3 { kind, owner: who.clone(), member: local_part(&via).to_string(), via });
        }
    }
    out
}

fn trait_desc(abc: &AbcFile, t: &swf::avm2::types::Trait, tn: &str, stat: bool) -> (String, String, String, bool) {
    match &t.kind {
        TraitKind::Slot { type_name, .. } => ("var".into(), tn.into(), mn(abc, *type_name), stat),
        TraitKind::Const { type_name, .. } => ("const".into(), tn.into(), mn(abc, *type_name), stat),
        TraitKind::Method { .. } => ("method".into(), tn.into(), String::new(), stat),
        TraitKind::Getter { .. } => ("getter".into(), tn.into(), String::new(), stat),
        TraitKind::Setter { .. } => ("setter".into(), tn.into(), String::new(), stat),
        TraitKind::Class { .. } => ("class".into(), tn.into(), String::new(), stat),
        TraitKind::Function { .. } => ("function".into(), tn.into(), String::new(), stat),
    }
}

fn load_movie_avm2(data: &[u8]) -> (Vec<Abc3>, Vec<String>, Vec<(u16, String)>, Vec<String>) {
    let dec = decompress_swf(data).expect("decompress");
    let movie = parse_swf(&dec).expect("parse");
    let mut abcs = Vec::new();
    let mut imports = Vec::new();
    let mut symbols = Vec::new();
    let mut tree = Vec::new();
    fn rec(tags: &[Tag<'_>], abcs: &mut Vec<Abc3>, imports: &mut Vec<String>, symbols: &mut Vec<(u16, String)>, tree: &mut Vec<String>, owner: &str) {
        for t in tags {
            match t {
                Tag::DoAbc(d) => abcs.push(scan_abc(&Avm2Reader::new(d).read().expect("abc"), "DoAbc")),
                Tag::DoAbc2(d) => abcs.push(scan_abc(&Avm2Reader::new(d.data).read().expect("abc"), &d.name.to_string_lossy(swf::UTF_8))),
                Tag::ImportAssets { url, imports: i } => {
                    imports.push(format!("{} <- {}", i.iter().map(|e| e.name.to_string_lossy(swf::UTF_8)).collect::<Vec<_>>().join(","), url.to_string_lossy(swf::UTF_8)))
                }
                Tag::SymbolClass(l) => {
                    for e in l {
                        symbols.push((e.id, e.class_name.to_string_lossy(swf::UTF_8)));
                    }
                }
                Tag::PlaceObject(po) => {
                    if let Some(n) = po.name {
                        let ch = match po.action {
                            PlaceObjectAction::Place(id) | PlaceObjectAction::Replace(id) => id.to_string(),
                            _ => "-".into(),
                        };
                        tree.push(format!("{owner}.{}\tchar {ch}\tdepth {}", n.to_string_lossy(swf::UTF_8), po.depth));
                    }
                }
                Tag::DefineSprite(s) => rec(&s.tags, abcs, imports, symbols, tree, &format!("sprite{}", s.id)),
                _ => {}
            }
        }
    }
    rec(&movie.tags, &mut abcs, &mut imports, &mut symbols, &mut tree, "root");
    (abcs, imports, symbols, tree)
}

fn print_avm2_dump(abcs: &[Abc3], imports: &[String], symbols: &[(u16, String)], tree: &[String]) {
    println!("# imports");
    for i in imports {
        println!("{i}");
    }
    println!("# symbol classes (char id -> class)");
    for (id, n) in symbols {
        println!("{id}\t{n}");
    }
    println!("# timeline instances");
    for t in tree {
        println!("{t}");
    }
    for a in abcs {
        println!("# ABC {} : {} classes", a.abc_name, a.classes.len());
        for c in &a.classes {
            println!("class {} extends {} implements [{}]", c.name, c.sup, c.ifaces.join(", "));
            for (k, n, t, st) in &c.members {
                println!("    {}{k} {n}{}", if *st { "static " } else { "" }, if t.is_empty() { String::new() } else { format!(": {t}") });
            }
        }
    }
}

/// Per member-name census: where it is declared (class + kind), who sets it,
/// who calls it, who reads it. A declared public method/setter with no
/// in-movie call/set site is a native-entry-point candidate.
fn print_avm2_members(abcs: &[Abc3], only_fed: bool) {
    #[derive(Default)]
    struct Row {
        declared: BTreeSet<String>,
        set_by: BTreeSet<String>,
        called_by: BTreeSet<String>,
        got_by: usize,
    }
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    for a in abcs {
        for c in &a.classes {
            for (k, n, _t, st) in &c.members {
                rows.entry(local_part(n).to_string()).or_default().declared.insert(format!("{}{}::{k} {}", if *st { "static " } else { "" }, c.name, n));
            }
        }
        for e in &a.events {
            let r = rows.entry(e.member.clone()).or_default();
            match e.kind {
                "set" | "init" => {
                    r.set_by.insert(e.owner.clone());
                }
                "call" | "new" => {
                    r.called_by.insert(e.owner.clone());
                }
                _ => r.got_by += 1,
            }
        }
    }
    println!("member\tdeclared_in\tset_by\tcalled_by\tgot_by_count");
    for (k, r) in rows {
        if r.declared.is_empty() {
            continue;
        }
        if only_fed {
            // Public surface (not `_private`, not frame scripts / IDE
            // `__setProp_*` initialisers) that no movie code writes or
            // calls after construction.
            let public = !k.starts_with('_') && !k.starts_with("frame");
            let init_only = r.set_by.iter().all(|o| o.contains("__setProp_") || o.contains("<ctor>") || o.contains("<cinit>"));
            let declared_kind_ok = r.declared.iter().any(|d| d.contains("::setter") || d.contains("::var") || d.contains("::method"));
            if !(public && declared_kind_ok && init_only && r.called_by.is_empty()) {
                continue;
            }
        }
        let j = |s: &BTreeSet<String>| s.iter().cloned().collect::<Vec<_>>().join(" | ");
        println!("{k}\t{}\t{}\t{}\t{}", j(&r.declared), j(&r.set_by), j(&r.called_by), r.got_by);
    }
}

fn print_avm2_strings(abcs: &[Abc3], needle: &str) {
    let n = needle.to_ascii_lowercase();
    for a in abcs {
        for s in &a.strings {
            if s.to_ascii_lowercase().contains(&n) {
                println!("{s}");
            }
        }
    }
}

/// Every event touching a member name (substring match), with its owner.
fn print_avm2_refs(abcs: &[Abc3], needle: &str) {
    for a in abcs {
        for e in &a.events {
            if e.member.contains(needle) || e.via.contains(needle) {
                println!("{}\t{}\t{}\t{}", e.kind, e.owner, e.via, a.abc_name);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game = args.get(1).map(String::as_str).unwrap_or("");
    let cmd = args.get(2).map(String::as_str).unwrap_or("");
    let name = args.get(3).cloned();
    match game {
        "skyrim" => {
            let a = BsaArchive::open(std::path::Path::new(SKYRIM_BSA)).expect("open bsa");
            if cmd == "listing" {
                let mut f: Vec<_> = a.list_files().into_iter().map(str::to_string).collect();
                f.sort();
                for x in f {
                    println!("{x}");
                }
                return;
            }
            let name = name.unwrap_or_else(|| "interface\\hudmenu.swf".to_string());
            let data = a.extract(&name).expect("extract");
            let m = load_movie_avm1(&data);
            match cmd {
                "dump" => print_dump(&m),
                "members" => print_members(&m, false),
                "fed" => print_members(&m, true),
                "tree" => {
                    for (id, n) in &m.exports {
                        println!("export\t{id}\t{n}");
                    }
                    for t in &m.tree {
                        println!("{t}");
                    }
                }
                _ => eprintln!("usage: hud_feed_paths skyrim listing|dump|members|tree [swf]"),
            }
        }
        "fo4" => {
            let a = Ba2Archive::open(std::path::Path::new(FO4_BA2)).expect("open ba2");
            if cmd == "listing" {
                let mut f: Vec<_> = a.list_files().into_iter().map(str::to_string).collect();
                f.sort();
                for x in f {
                    println!("{x}");
                }
                return;
            }
            let name = name.unwrap_or_else(|| "interface\\hudmenu.swf".to_string());
            let data = a.extract(&name).expect("extract");
            let (abcs, imports, symbols, tree) = load_movie_avm2(&data);
            let arg4 = args.get(4).cloned().unwrap_or_default();
            match cmd {
                "dump" => print_avm2_dump(&abcs, &imports, &symbols, &tree),
                "members" => print_avm2_members(&abcs, false),
                "fed" => print_avm2_members(&abcs, true),
                "strings" => print_avm2_strings(&abcs, &arg4),
                "refs" => print_avm2_refs(&abcs, &arg4),
                _ => eprintln!("usage: hud_feed_paths fo4 listing|dump|members|strings <swf> <needle>|refs <swf> <needle>"),
            }
        }
        _ => eprintln!("usage: hud_feed_paths skyrim|fo4 <cmd>"),
    }
}
