//! The AVM1 path adapter (M48 Phase 2, GUI-PATH-001; Q2 answered by
//! `tests/avm1_path_access_probe.rs`).
//!
//! A `DoAction` spliced into frame 1 of every AVM1 movie registers the three path callbacks with
//! `flash.external.ExternalInterface.addCallback` (the class is reached by its full path; a bare
//! `ExternalInterface` is compile-time sugar). In ActionScript 2 terms:
//!
//! ```text
//! get(p)        r = {}; v = eval(p); r.type = typeof v
//!               r.found = r.type != "undefined"; primitive v -> r.value = v; return r
//! set(p, v)     resolve(p) -> parent, key; parent missing -> {found: false}
//!               r.created = typeof parent[key] == "undefined"; parent[key] = v; r.found = true
//! call(p, args) resolve(p) -> parent, key; f = parent[key]; typeof f != "function" -> {found: false}
//!               res = f.apply(parent, args); r.found = true; primitive res -> r.value = res
//! resolve(p)    i = p.lastIndexOf("."); i < 0 -> (_root, p)
//!               else (eval(p.substring(0, i)), p.substring(i + 1))
//! ```
//!
//! `eval` on a dotted path string is `GetVariable`, which resolves it. A reply carries `value`
//! only for a number, string or boolean: a clip or object would be converted member by member
//! across the ExternalInterface boundary, and those graphs can be cyclic. A present member whose
//! value is `undefined` reads as absent — AS2 cannot tell the two apart (the probe's last test).
//! The splice runs on the stock decode's output, so a loose AVM1 movie is still never tag-parsed
//! (#5479).

use swf::avm1::types::{Action, DefineFunction, If, Jump, Push, Value};
use swf::SwfStr;

use crate::path::{PATH_CALL_CALLBACK, PATH_GET_CALLBACK, PATH_SET_CALLBACK};

/// One assembler item: an action, a branch to a label, or a label.
enum Item {
    Action(Action<'static>),
    If(usize),
    Jump(usize),
    Label(usize),
}

/// Encoded size of an `If` / `Jump`: opcode, u16 length, i16 offset.
const BRANCH_LEN: usize = 5;

fn encode(action: &Action<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    swf::avm1::write::Writer::new(&mut bytes, 8)
        .write_action(action)
        .expect("writing an in-memory AVM1 action cannot fail");
    bytes
}

/// Resolve labels and encode. Branch offsets are relative to the end of the branch action.
fn assemble(items: &[Item]) -> Vec<u8> {
    let mut position = 0usize;
    let mut labels = std::collections::HashMap::new();
    let mut sizes = Vec::with_capacity(items.len());
    for item in items {
        let size = match item {
            Item::Action(action) => encode(action).len(),
            Item::If(_) | Item::Jump(_) => BRANCH_LEN,
            Item::Label(label) => {
                labels.insert(*label, position);
                0
            }
        };
        sizes.push(size);
        position += size;
    }
    let mut out = Vec::with_capacity(position);
    for (item, size) in items.iter().zip(sizes) {
        let offset = |label: &usize| (labels[label] as i64 - (out.len() + size) as i64) as i16;
        let bytes = match item {
            Item::Action(action) => encode(action),
            Item::If(label) => encode(&Action::If(If { offset: offset(label) })),
            Item::Jump(label) => encode(&Action::Jump(Jump { offset: offset(label) })),
            Item::Label(_) => Vec::new(),
        };
        out.extend(bytes);
    }
    out
}

fn s(text: &'static str) -> Item {
    Item::Action(Action::Push(Push { values: vec![Value::Str(SwfStr::from_utf8_str(text))] }))
}

fn int(value: i32) -> Item {
    Item::Action(Action::Push(Push { values: vec![Value::Int(value)] }))
}

fn boolean(value: bool) -> Item {
    Item::Action(Action::Push(Push { values: vec![Value::Bool(value)] }))
}

fn op(action: Action<'static>) -> Item {
    Item::Action(action)
}

/// `name` read: `GetVariable`.
fn var(name: &'static str) -> [Item; 2] {
    [s(name), op(Action::GetVariable)]
}

/// Labels of one function body.
const ABSENT: usize = 1;
const RESOLVED: usize = 2;
const ROOT_PARENT: usize = 3;
const PRIMITIVE: usize = 4;
const STORED: usize = 5;

/// `var r = {};`
fn new_reply() -> Vec<Item> {
    vec![s("r"), int(0), op(Action::InitObject), op(Action::DefineLocal)]
}

/// `r.<field> = <bool>;`
fn set_flag(field: &'static str, value: bool) -> Vec<Item> {
    let mut items: Vec<Item> = var("r").into();
    items.extend([s(field), boolean(value), op(Action::SetMember)]);
    items
}

/// `r.type = typeof <local>; if <primitive> r.value = <local>;`
fn store_primitive(local: &'static str) -> Vec<Item> {
    let mut items = vec![s("t")];
    items.extend(var(local));
    items.extend([op(Action::TypeOf), op(Action::DefineLocal)]);
    items.extend(var("r"));
    items.extend([s("type")]);
    items.extend(var("t"));
    items.push(op(Action::SetMember));
    for primitive in ["number", "string", "boolean"] {
        items.extend(var("t"));
        items.extend([s(primitive), op(Action::Equals2), Item::If(PRIMITIVE)]);
    }
    items.push(Item::Jump(STORED));
    items.push(Item::Label(PRIMITIVE));
    items.extend(var("r"));
    items.push(s("value"));
    items.extend(var(local));
    items.extend([op(Action::SetMember), Item::Label(STORED)]);
    items
}

/// `return r;` then the absent tail: `r.found = false; return r;`
fn return_and_absent_tail() -> Vec<Item> {
    let mut items: Vec<Item> = var("r").into();
    items.push(op(Action::Return));
    items.push(Item::Label(ABSENT));
    items.extend(set_flag("found", false));
    items.extend(var("r"));
    items.push(op(Action::Return));
    items
}

/// `(parent, key) = resolve(p); var r = {}; parent missing -> goto ABSENT`.
fn resolve_parent() -> Vec<Item> {
    let mut items = vec![s("i"), s("."), int(1)];
    items.extend(var("p"));
    items.extend([s("lastIndexOf"), op(Action::CallMethod), op(Action::DefineLocal)]);
    // if (i < 0) goto ROOT_PARENT
    items.extend(var("i"));
    items.extend([int(0), op(Action::Less2), Item::If(ROOT_PARENT)]);
    // parent = eval(p.substring(0, i))
    items.push(s("parent"));
    items.extend(var("i"));
    items.extend([int(0), int(2)]);
    items.extend(var("p"));
    items.extend([s("substring"), op(Action::CallMethod), op(Action::GetVariable), op(Action::DefineLocal)]);
    // key = p.substring(i + 1)
    items.push(s("key"));
    items.extend(var("i"));
    items.extend([int(1), op(Action::Add2), int(1)]);
    items.extend(var("p"));
    items.extend([s("substring"), op(Action::CallMethod), op(Action::DefineLocal), Item::Jump(RESOLVED)]);
    items.push(Item::Label(ROOT_PARENT));
    items.push(s("parent"));
    items.extend(var("_root"));
    items.push(op(Action::DefineLocal));
    items.push(s("key"));
    items.extend(var("p"));
    items.push(op(Action::DefineLocal));
    items.push(Item::Label(RESOLVED));
    items.extend(new_reply());
    for missing in ["undefined", "null"] {
        items.extend(var("parent"));
        items.extend([op(Action::TypeOf), s(missing), op(Action::Equals2), Item::If(ABSENT)]);
    }
    items
}

fn get_body() -> Vec<u8> {
    let mut items = new_reply();
    items.push(s("v"));
    items.extend(var("p"));
    items.extend([op(Action::GetVariable), op(Action::DefineLocal)]);
    items.extend(var("v"));
    items.extend([op(Action::TypeOf), s("undefined"), op(Action::Equals2), Item::If(ABSENT)]);
    items.extend(set_flag("found", true));
    items.extend(store_primitive("v"));
    items.extend(return_and_absent_tail());
    assemble(&items)
}

fn set_body() -> Vec<u8> {
    let mut items = resolve_parent();
    // r.created = typeof parent[key] == "undefined"
    items.extend(var("r"));
    items.push(s("created"));
    items.extend(var("parent"));
    items.extend(var("key"));
    items.extend([op(Action::GetMember), op(Action::TypeOf), s("undefined"), op(Action::Equals2), op(Action::SetMember)]);
    // parent[key] = v
    items.extend(var("parent"));
    items.extend(var("key"));
    items.extend(var("v"));
    items.push(op(Action::SetMember));
    items.extend(set_flag("found", true));
    items.extend(return_and_absent_tail());
    assemble(&items)
}

fn call_body() -> Vec<u8> {
    let mut items = resolve_parent();
    // var f = parent[key]; typeof f != "function" -> absent
    items.push(s("f"));
    items.extend(var("parent"));
    items.extend(var("key"));
    items.extend([op(Action::GetMember), op(Action::DefineLocal)]);
    items.extend(var("f"));
    items.extend([op(Action::TypeOf), s("function"), op(Action::Equals2), op(Action::Not), Item::If(ABSENT)]);
    // var res = f.apply(parent, args)
    items.push(s("res"));
    items.extend(var("args"));
    items.extend(var("parent"));
    items.push(int(2));
    items.extend(var("f"));
    items.extend([s("apply"), op(Action::CallMethod), op(Action::DefineLocal)]);
    items.extend(set_flag("found", true));
    items.extend(store_primitive("res"));
    items.extend(return_and_absent_tail());
    assemble(&items)
}

/// `flash.external.ExternalInterface.addCallback(name, null, <function on the stack>)`.
fn register(name: &'static str) -> Vec<Item> {
    let mut items = vec![
        Item::Action(Action::Push(Push { values: vec![Value::Null] })),
        s(name),
        int(3),
    ];
    items.extend(var("flash"));
    items.extend([
        s("external"),
        op(Action::GetMember),
        s("ExternalInterface"),
        op(Action::GetMember),
        s("addCallback"),
        op(Action::CallMethod),
        op(Action::Pop),
    ]);
    items
}

/// The adapter's frame-1 action bytes (End-terminated).
pub(crate) fn adapter_actions() -> Vec<u8> {
    let mut out = Vec::new();
    for (name, params, body) in [
        (PATH_GET_CALLBACK, vec!["p"], get_body()),
        (PATH_SET_CALLBACK, vec!["p", "v"], set_body()),
        (PATH_CALL_CALLBACK, vec!["p", "args"], call_body()),
    ] {
        out.extend(encode(&Action::DefineFunction(DefineFunction {
            name: SwfStr::from_utf8_str(""),
            params: params.into_iter().map(SwfStr::from_utf8_str).collect(),
            actions: &body,
        })));
        out.extend(assemble(&register(name)));
    }
    out.push(0); // End
    out
}

const TAG_DO_ACTION: u16 = 12;
const TAG_FILE_ATTRIBUTES: u16 = 69;

/// The full length (header included) of the tag at the start of `stream`, and its code.
fn first_tag(stream: &[u8]) -> Option<(u16, usize)> {
    let code_and_len = u16::from_le_bytes([*stream.first()?, *stream.get(1)?]);
    let code = code_and_len >> 6;
    let short = (code_and_len & 0x3F) as usize;
    if short == 0x3F {
        let len = u32::from_le_bytes(stream.get(2..6)?.try_into().ok()?) as usize;
        Some((code, 6 + len))
    } else {
        Some((code, 2 + short))
    }
}

/// An uncompressed copy of the movie whose first frame begins with the adapter's `DoAction`
/// (after a leading `FileAttributes`, which must stay the first tag). `header` and `tags` are
/// the stock decode's (`swf::decompress_swf`): the header and the tag stream after it.
pub(crate) fn inject(header: &swf::Header, tags: &[u8]) -> Result<Vec<u8>, String> {
    let mut plain = header.clone();
    plain.compression = swf::Compression::None;
    let mut out = Vec::with_capacity(tags.len() + 2048);
    swf::write_swf(&plain, &[], &mut out).map_err(|error| format!("writing the SWF header: {error}"))?;
    let header_len = super::prepare::tag_stream_start(&out).ok_or("the written SWF header is malformed")?;
    out.truncate(header_len);
    let lead = match first_tag(tags) {
        Some((TAG_FILE_ATTRIBUTES, len)) if len <= tags.len() => len,
        _ => 0,
    };
    out.extend_from_slice(&tags[..lead]);
    let actions = adapter_actions();
    out.extend_from_slice(&((TAG_DO_ACTION << 6) | 0x3F).to_le_bytes());
    out.extend_from_slice(&(actions.len() as u32).to_le_bytes());
    out.extend_from_slice(&actions);
    out.extend_from_slice(&tags[lead..]);
    let file_len = u32::try_from(out.len()).map_err(|_| "the injected SWF exceeds 4 GiB")?;
    out[4..8].copy_from_slice(&file_len.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::{decode_reply, PathOp, PathOutcome};
    use crate::ScaleformValue;
    use ruffle_core::external::{ExternalInterfaceProvider, Value as ExternalValue};
    use ruffle_core::tag_utils::SwfMovieData;
    use ruffle_core::{FloatDuration, LoadBehavior, Player, PlayerBuilder};
    use std::sync::{Arc, Mutex};

    struct Provider;

    impl ExternalInterfaceProvider for Provider {
        fn call_method(&self, _: &mut ruffle_core::context::UpdateContext<'_>, _: &str, _: &[ExternalValue]) -> ExternalValue {
            ExternalValue::Undefined
        }
        fn on_callback_available(&self, _: &str) {}
        fn get_id(&self) -> Option<String> {
            Some("avm1-path-adapter-test".into())
        }
    }

    /// Stand-in HUD state under `_root`: `_root.meter = {value: 7, bump: function (n) {
    /// this.value = n; return n + 1; }}` — `bump` writes through `this`, so a call proves the
    /// adapter binds the parent.
    fn movie() -> Vec<u8> {
        let mut bump = vec![s("this"), op(Action::GetVariable), s("value")];
        bump.extend(var("n"));
        bump.push(op(Action::SetMember));
        bump.extend(var("n"));
        bump.extend([int(1), op(Action::Add2), op(Action::Return)]);
        let bump = assemble(&bump);
        let mut frame = assemble(&[
            s("_root.meter"),
            int(0),
            op(Action::InitObject),
            op(Action::SetVariable),
            s("_root.meter.value"),
            int(7),
            op(Action::SetVariable),
            s("_root.meter.bump"),
        ]);
        frame.extend(encode(&Action::DefineFunction(DefineFunction {
            name: SwfStr::from_utf8_str(""),
            params: vec![SwfStr::from_utf8_str("n")],
            actions: &bump,
        })));
        frame.extend(encode(&Action::SetVariable));
        frame.push(0);
        let mut header = swf::Header::default_with_swf_version(8);
        header.frame_rate = swf::Fixed8::from_f32(30.0);
        header.num_frames = 1;
        let mut bytes = Vec::new();
        swf::write_swf(&header, &[swf::Tag::DoAction(&frame), swf::Tag::ShowFrame], &mut bytes).unwrap();
        bytes
    }

    /// The movie as the engine loads it — through `prepare_movie`, which injects the adapter —
    /// on a renderer-less Ruffle player (no GPU), ticked past frame 1.
    fn start() -> Arc<Mutex<Player>> {
        let prepared = crate::prepare::prepare_movie(&movie(), None, None).expect("prepare");
        let movie = SwfMovieData::from_data(&prepared.data, "file:///adapter.swf".into(), None, None).expect("parse");
        let player = PlayerBuilder::new()
            .with_external_interface(Box::new(Provider))
            .with_movie(movie)
            .with_load_behavior(LoadBehavior::Blocking)
            .with_viewport_dimensions(64, 64, 1.0)
            .build();
        player.lock().unwrap().set_is_playing(true);
        for _ in 0..3 {
            player.lock().unwrap().tick(FloatDuration::from_secs(1.0 / 30.0));
        }
        player
    }

    fn run(player: &Arc<Mutex<Player>>, op: PathOp) -> PathOutcome {
        let (callback, arguments) = op.callback();
        let reply = player
            .lock()
            .unwrap()
            .call_internal_interface(callback, arguments.into_iter().map(ExternalValue::from).collect::<Vec<_>>());
        decode_reply(Some(ScaleformValue::from(&reply)))
    }

    /// M48 A6 (GUI-PATH-001/002) — the injected AVM1 adapter reads, writes and invokes `_root`
    /// members by path, and an absent path is a typed result, not a silent no-op.
    #[test]
    fn avm1_adapter_sets_and_reads_a_root_path() {
        let player = start();
        let number = |value: f64| PathOutcome::Found(ScaleformValue::Number(value));
        assert_eq!(run(&player, PathOp::Get("_root.meter.value".into())), number(7.0));
        assert_eq!(run(&player, PathOp::Set("_root.meter.value".into(), ScaleformValue::Number(31.0))), PathOutcome::Found(ScaleformValue::Undefined));
        assert_eq!(run(&player, PathOp::Get("_root.meter.value".into())), number(31.0));
        // A call binds the parent as `this` and returns the function's value.
        assert_eq!(run(&player, PathOp::Call("_root.meter.bump".into(), vec![ScaleformValue::Number(9.0)])), number(10.0));
        assert_eq!(run(&player, PathOp::Get("_root.meter.value".into())), number(9.0));
        // A bare name resolves on `_root`.
        assert_eq!(run(&player, PathOp::Set("loose".into(), ScaleformValue::String("x".into()))), PathOutcome::Found(ScaleformValue::Undefined));
        assert_eq!(run(&player, PathOp::Get("_root.loose".into())), PathOutcome::Found(ScaleformValue::String("x".into())));
        // Absent paths: a missing member, a missing parent, a member that is not a function.
        assert_eq!(run(&player, PathOp::Get("_root.meter.nope".into())), PathOutcome::Absent);
        assert_eq!(run(&player, PathOp::Set("_root.nope.value".into(), ScaleformValue::Number(1.0))), PathOutcome::Absent);
        assert_eq!(run(&player, PathOp::Call("_root.meter.value".into(), Vec::new())), PathOutcome::Absent);
        assert_eq!(run(&player, PathOp::Call("_root.nope.go".into(), Vec::new())), PathOutcome::Absent);
        // An object is found without its members crossing the boundary.
        assert_eq!(run(&player, PathOp::Get("_root.meter".into())), PathOutcome::Found(ScaleformValue::Undefined));
    }

    /// The splice keeps a leading `FileAttributes` first and the rest of the stream intact.
    #[test]
    fn the_adapter_follows_file_attributes() {
        let mut header = swf::Header::default_with_swf_version(8);
        header.frame_rate = swf::Fixed8::from_f32(30.0);
        header.num_frames = 1;
        let mut bytes = Vec::new();
        swf::write_swf(
            &header,
            &[swf::Tag::FileAttributes(swf::FileAttributes::empty()), swf::Tag::ShowFrame],
            &mut bytes,
        )
        .unwrap();
        let decoded = swf::decompress_swf(&bytes[..]).unwrap();
        let injected = inject(decoded.header.swf_header(), &decoded.data).unwrap();
        let reparsed = swf::decompress_swf(&injected[..]).unwrap();
        let swf = swf::parse_swf(&reparsed).unwrap();
        assert!(matches!(swf.tags[0], swf::Tag::FileAttributes(_)));
        assert!(matches!(swf.tags[1], swf::Tag::DoAction(_)));
        assert!(matches!(swf.tags[2], swf::Tag::ShowFrame));
    }
}
