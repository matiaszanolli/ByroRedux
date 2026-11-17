//! The AVM2 path adapter (M48 Phase 2, GUI-PATH-001): the get / set / call callbacks of the path
//! service, added to the `BGSCodeObj` adapter and registered by its installer once the movie root
//! is stored. AS3 has no `eval`, so a shared walker splits the dotted path and indexes from the
//! root one segment at a time, checking presence first. In ActionScript 3 terms:
//!
//! ```text
//! walk(p, drop)  parts = p.split("."); n = parts.length - drop; o = root
//!                for i in 0..n: typeof o not "object"/"function", or o == null -> {found: false}
//!                               k = parts[i]
//!                               o = (k in o) ? o[k]
//!                                 : ("getChildByName" in o) ? o.getChildByName(k) ?? absent
//!                                 : absent
//!                {found: true, o: o, key: parts[n]}
//! get(p)         w = walk(p, 0); !w.found -> w; {found: true, type: typeof w.o, value: primitive w.o}
//! set(p, v)      w = walk(p, 1); !w.found or !(w.key in w.o) -> absent; w.o[w.key] = v; {found: true}
//! call(p, args)  w = walk(p, 1); f = w.o[w.key]; typeof f != "function" -> absent
//!                res = f.apply(w.o, args); {found: true, type: typeof res, value: primitive res}
//! ```
//!
//! The reply is the AVM1 adapter's shape (`crate::path`). A value crosses back only when it is a
//! number, string or boolean: a display object would be converted member by member.

use swf::avm2::types::{Index, Method, MethodFlags, MethodParam, Multiname, Op};

use super::{add_multiname, add_string, method_trait, qname, AdapterMethods, AdapterNames, AdapterPools};
use crate::path::{PATH_CALL_CALLBACK, PATH_GET_CALLBACK, PATH_SET_CALLBACK};

/// The installer's view of the path helpers: each callback name and the helper it registers.
pub(super) struct PathHelpers {
    pub(super) registrations: [(Index<String>, Index<Multiname>); 3],
}

/// Pool entries the path methods name.
struct PathNames {
    root: Index<Multiname>,
    /// A runtime-named property (`o[k]`) in the public namespace.
    dynamic: Index<Multiname>,
    walker: Index<Multiname>,
    found: Index<Multiname>,
    value: Index<Multiname>,
    kind: Index<Multiname>,
    object: Index<Multiname>,
    key: Index<Multiname>,
    split: Index<Multiname>,
    length: Index<Multiname>,
    apply: Index<Multiname>,
    get_child_by_name: Index<Multiname>,
    get_child_by_name_string: Index<String>,
    found_string: Index<String>,
    type_string: Index<String>,
    object_string: Index<String>,
    key_string: Index<String>,
    dot: Index<String>,
    number: Index<String>,
    string: Index<String>,
    boolean: Index<String>,
    object_type: Index<String>,
    function_type: Index<String>,
}

/// One assembler item: an op, a branch to a label, or a label.
enum Item {
    Op(Op),
    Branch(fn(i32) -> Op, usize),
    Label(usize),
}

/// Encoded size of a branch: opcode + s24 offset.
const BRANCH_LEN: usize = 4;

/// Resolve labels and encode; a branch offset is relative to the end of the branch.
fn assemble(items: &[Item]) -> std::io::Result<Vec<u8>> {
    let mut labels = std::collections::HashMap::new();
    let mut position = 0usize;
    for item in items {
        match item {
            Item::Op(op) => position += super::write_ops(std::slice::from_ref(op))?.len(),
            Item::Branch(..) => position += BRANCH_LEN,
            Item::Label(label) => {
                labels.insert(*label, position);
            }
        }
    }
    let mut ops = Vec::with_capacity(items.len());
    let mut position = 0usize;
    for item in items {
        match item {
            Item::Op(op) => {
                position += super::write_ops(std::slice::from_ref(op))?.len();
                ops.push(op.clone());
            }
            Item::Branch(branch, label) => {
                position += BRANCH_LEN;
                ops.push(branch(labels[label] as i32 - position as i32));
            }
            Item::Label(_) => {}
        }
    }
    super::write_ops(&ops)
}

fn op(op: Op) -> Item {
    Item::Op(op)
}

fn local(index: u32) -> Item {
    op(Op::GetLocal { index })
}

fn store(index: u32) -> Item {
    op(Op::SetLocal { index })
}

fn string(value: Index<String>) -> Item {
    op(Op::PushString { value })
}

fn jump(label: usize) -> Item {
    Item::Branch(|offset| Op::Jump { offset }, label)
}

fn if_ge(label: usize) -> Item {
    Item::Branch(|offset| Op::IfGe { offset }, label)
}

fn if_false(label: usize) -> Item {
    Item::Branch(|offset| Op::IfFalse { offset }, label)
}

fn if_true(label: usize) -> Item {
    Item::Branch(|offset| Op::IfTrue { offset }, label)
}

fn if_strict_eq(label: usize) -> Item {
    Item::Branch(|offset| Op::IfStrictEq { offset }, label)
}

fn if_strict_ne(label: usize) -> Item {
    Item::Branch(|offset| Op::IfStrictNe { offset }, label)
}

/// The prologue every adapter method opens with.
fn prologue() -> Vec<Item> {
    vec![local(0), op(Op::PushScope)]
}

/// `return {found: false};`
fn return_absent(names: &PathNames) -> Vec<Item> {
    vec![string(names.found_string), op(Op::PushFalse), op(Op::NewObject { num_args: 1 }), op(Op::ReturnValue)]
}

const LOOP: usize = 1;
const DONE: usize = 2;
const ABSENT: usize = 3;
const TAKE: usize = 4;
const IS_OBJECT: usize = 5;
const PRIMITIVE: usize = 6;
const STORED: usize = 7;

/// `walk(p, drop)`: locals 1 p, 2 drop, 3 parts, 4 n, 5 o, 6 i, 7 k, 8 type.
fn walker(names: &PathNames) -> std::io::Result<Vec<u8>> {
    let mut items = prologue();
    items.extend([
        local(1),
        string(names.dot),
        op(Op::CallProperty { index: names.split, num_args: 1 }),
        store(3),
        local(3),
        op(Op::GetProperty { index: names.length }),
        local(2),
        op(Op::Subtract),
        op(Op::ConvertI),
        store(4),
        op(Op::GetLex { index: names.root }),
        store(5),
        op(Op::PushByte { value: 0 }),
        store(6),
        Item::Label(LOOP),
        op(Op::Label),
        local(6),
        local(4),
        if_ge(DONE),
        // typeof o must be "object" (not null) or "function".
        local(5),
        op(Op::TypeOf),
        store(8),
        local(8),
        string(names.object_type),
        if_strict_eq(IS_OBJECT),
        local(8),
        string(names.function_type),
        if_strict_ne(ABSENT),
        Item::Label(IS_OBJECT),
        local(5),
        op(Op::PushNull),
        if_strict_eq(ABSENT),
        // k = parts[i]
        local(3),
        local(6),
        op(Op::GetProperty { index: names.dynamic }),
        store(7),
        local(7),
        local(5),
        op(Op::In),
        if_true(TAKE),
        // A display container's child that is not a declared variable.
        string(names.get_child_by_name_string),
        local(5),
        op(Op::In),
        if_false(ABSENT),
        local(5),
        local(7),
        op(Op::CallProperty { index: names.get_child_by_name, num_args: 1 }),
        op(Op::Dup),
        store(5),
        op(Op::PushNull),
        if_strict_eq(ABSENT),
        op(Op::IncLocalI { index: 6 }),
        jump(LOOP),
        Item::Label(TAKE),
        local(5),
        local(7),
        op(Op::GetProperty { index: names.dynamic }),
        store(5),
        op(Op::IncLocalI { index: 6 }),
        jump(LOOP),
        Item::Label(DONE),
        string(names.found_string),
        op(Op::PushTrue),
        string(names.object_string),
        local(5),
        string(names.key_string),
        local(3),
        local(4),
        op(Op::GetProperty { index: names.dynamic }),
        op(Op::NewObject { num_args: 3 }),
        op(Op::ReturnValue),
        Item::Label(ABSENT),
    ]);
    items.extend(return_absent(names));
    assemble(&items)
}

/// `w = walk(p, drop)` into local `into`; `!w.found -> return w`. `p` is local 1.
fn walk_into(names: &PathNames, drop: u8, into: u32) -> Vec<Item> {
    vec![
        op(Op::GetLex { index: names.walker }),
        op(Op::PushNull),
        local(1),
        op(Op::PushByte { value: drop }),
        op(Op::Call { num_args: 2 }),
        store(into),
        local(into),
        op(Op::GetProperty { index: names.found }),
        if_true(DONE),
        local(into),
        op(Op::ReturnValue),
        Item::Label(DONE),
    ]
}

/// `reply = {found: true, type: typeof <value>}; primitive -> reply.value = <value>; return reply`.
/// `value` and `reply` are locals.
fn found_reply(names: &PathNames, value: u32, reply: u32) -> Vec<Item> {
    let mut items = vec![
        string(names.found_string),
        op(Op::PushTrue),
        string(names.type_string),
        local(value),
        op(Op::TypeOf),
        op(Op::NewObject { num_args: 2 }),
        store(reply),
    ];
    for primitive in [names.number, names.string, names.boolean] {
        items.extend([
            local(reply),
            op(Op::GetProperty { index: names.kind }),
            string(primitive),
            if_strict_eq(PRIMITIVE),
        ]);
    }
    items.extend([
        jump(STORED),
        Item::Label(PRIMITIVE),
        local(reply),
        local(value),
        op(Op::SetProperty { index: names.value }),
        Item::Label(STORED),
        local(reply),
        op(Op::ReturnValue),
    ]);
    items
}

/// `get(p)`: locals 1 p, 2 w, 3 value, 4 reply.
fn get(names: &PathNames) -> std::io::Result<Vec<u8>> {
    let mut items = prologue();
    items.extend(walk_into(names, 0, 2));
    items.extend([local(2), op(Op::GetProperty { index: names.object }), store(3)]);
    items.extend(found_reply(names, 3, 4));
    assemble(&items)
}

/// `set(p, v)`: locals 1 p, 2 v, 3 w, 4 o, 5 key, 6 type.
fn set(names: &PathNames) -> std::io::Result<Vec<u8>> {
    let mut items = prologue();
    items.extend(walk_into(names, 1, 3));
    items.extend([
        local(3),
        op(Op::GetProperty { index: names.object }),
        store(4),
        local(3),
        op(Op::GetProperty { index: names.key }),
        store(5),
        // The parent is an object and has the member.
        local(4),
        op(Op::TypeOf),
        store(6),
        local(6),
        string(names.object_type),
        if_strict_eq(IS_OBJECT),
        local(6),
        string(names.function_type),
        if_strict_ne(ABSENT),
        Item::Label(IS_OBJECT),
        local(4),
        op(Op::PushNull),
        if_strict_eq(ABSENT),
        local(5),
        local(4),
        op(Op::In),
        if_false(ABSENT),
        local(4),
        local(5),
        local(2),
        op(Op::SetProperty { index: names.dynamic }),
        string(names.found_string),
        op(Op::PushTrue),
        op(Op::NewObject { num_args: 1 }),
        op(Op::ReturnValue),
        Item::Label(ABSENT),
    ]);
    items.extend(return_absent(names));
    assemble(&items)
}

/// `call(p, args)`: locals 1 p, 2 args, 3 w, 4 o, 5 f, 6 result, 7 reply.
fn call(names: &PathNames) -> std::io::Result<Vec<u8>> {
    let mut items = prologue();
    items.extend(walk_into(names, 1, 3));
    items.extend([
        local(3),
        op(Op::GetProperty { index: names.object }),
        store(4),
        // f = o[key]: the walk only checked the parent, so check the member here.
        local(4),
        op(Op::PushNull),
        if_strict_eq(ABSENT),
        local(3),
        op(Op::GetProperty { index: names.key }),
        local(4),
        op(Op::In),
        if_false(ABSENT),
        local(4),
        local(3),
        op(Op::GetProperty { index: names.key }),
        op(Op::GetProperty { index: names.dynamic }),
        store(5),
        local(5),
        op(Op::TypeOf),
        string(names.function_type),
        if_strict_ne(ABSENT),
        local(5),
        local(4),
        local(2),
        op(Op::CallProperty { index: names.apply, num_args: 2 }),
        store(6),
    ]);
    items.extend(found_reply(names, 6, 7));
    items.push(Item::Label(ABSENT));
    items.extend(return_absent(names));
    assemble(&items)
}

fn params(count: usize) -> Vec<MethodParam> {
    (0..count).map(|_| MethodParam { name: None, kind: Index::new(0), default_value: None }).collect()
}

/// How many methods (and script traits) the path service adds to the adapter: the walker and
/// get / set / call. They follow the adapter's fixed tail (ready / destroy / installer / init).
#[cfg(test)]
pub(super) const PATH_ADAPTER_METHODS: usize = 4;
/// The constant-pool entries the path service adds: 14 member / type names, the walker and three
/// helper names, the three callback names; 13 multinames (10 members plus the three helpers).
#[cfg(test)]
pub(super) const PATH_ADAPTER_STRINGS: usize = 21;
#[cfg(test)]
pub(super) const PATH_ADAPTER_MULTINAMES: usize = 13;

/// The path methods' pool entries, made before the installer (which registers the helpers) and
/// emitted after the script initialiser ([`emit`]).
pub(super) struct PathPlan {
    names: PathNames,
    walker_string: Index<String>,
    helper_strings: [Index<String>; 3],
    helpers: [Index<Multiname>; 3],
    pub(super) registrations: PathHelpers,
}

/// Add the pool entries the path methods name.
pub(super) fn prepare(pools: &mut AdapterPools, adapter: &AdapterNames) -> PathPlan {
    let public = pools.public_namespace;
    let mut name = |text: &str| add_string(&mut pools.strings, text.as_bytes().to_vec());
    let found_string = name("found");
    let type_string = name("type");
    let object_string = name("o");
    let key_string = name("key");
    let value_string = name("value");
    let split_string = name("split");
    let length_string = name("length");
    let get_child_by_name_string = name("getChildByName");
    let dot = name(".");
    let number = name("number");
    let string_type = name("string");
    let boolean = name("boolean");
    let object_type = name("object");
    let function_type = name("function");
    let walker_string = name("__byro_fallout4_path_walk");
    let helper_strings = [name("__byro_fallout4_path_get"), name("__byro_fallout4_path_set"), name("__byro_fallout4_path_call")];
    let callback_strings = [name(PATH_GET_CALLBACK), name(PATH_SET_CALLBACK), name(PATH_CALL_CALLBACK)];

    pools.namespace_sets.push(vec![public]);
    let public_set = Index::new(pools.namespace_sets.len() as u32);
    let mut multiname = |value: Multiname| add_multiname(&mut pools.multinames, value);
    let names = PathNames {
        root: adapter.root_slot,
        dynamic: multiname(Multiname::MultinameL { namespace_set: public_set }),
        walker: multiname(qname(public, walker_string)),
        found: multiname(qname(public, found_string)),
        value: multiname(qname(public, value_string)),
        kind: multiname(qname(public, type_string)),
        object: multiname(qname(public, object_string)),
        key: multiname(qname(public, key_string)),
        split: multiname(qname(public, split_string)),
        length: multiname(qname(public, length_string)),
        apply: adapter.apply,
        get_child_by_name: multiname(qname(public, get_child_by_name_string)),
        get_child_by_name_string,
        found_string,
        type_string,
        object_string,
        key_string,
        dot,
        number,
        string: string_type,
        boolean,
        object_type,
        function_type,
    };
    let helpers = helper_strings.map(|helper| multiname(qname(public, helper)));
    PathPlan {
        names,
        walker_string,
        helper_strings,
        helpers,
        registrations: PathHelpers {
            registrations: [
                (callback_strings[0], helpers[0]),
                (callback_strings[1], helpers[1]),
                (callback_strings[2], helpers[2]),
            ],
        },
    }
}

/// Append the walker and the three path helpers as script traits from `next_disp_id` on.
pub(super) fn emit(plan: &PathPlan, out: &mut AdapterMethods, next_disp_id: &mut u32) -> std::io::Result<()> {
    let names = &plan.names;
    let walker_method = out.add(plan.walker_string, params(2), MethodFlags::empty(), (8, 9), walker(names)?);
    out.traits.push(method_trait(names.walker, walker_method, *next_disp_id));
    *next_disp_id += 1;
    let bodies = [(get(names)?, 1, (6, 5)), (set(names)?, 2, (6, 7)), (call(names)?, 2, (6, 8))];
    for ((code, param_count, sizes), (helper, helper_string)) in
        bodies.into_iter().zip(plan.helpers.iter().zip(plan.helper_strings))
    {
        let method: Index<Method> = out.add(helper_string, params(param_count), MethodFlags::empty(), sizes, code);
        out.traits.push(method_trait(*helper, method, *next_disp_id));
        *next_disp_id += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{build_adapter_abc, write_ops, INSTALL_HELPER};
    use crate::path::{decode_reply, PathOp, PathOutcome};
    use crate::{ScaleformHostCatalog, ScaleformProfile, ScaleformValue};
    use ruffle_core::external::{ExternalInterfaceProvider, Value as ExternalValue};
    use ruffle_core::tag_utils::SwfMovieData;
    use ruffle_core::{FloatDuration, LoadBehavior, Player, PlayerBuilder};
    use std::sync::{Arc, Mutex};
    use swf::avm2::types::{
        AbcFile, ConstantPool, Index, Method, MethodBody, MethodFlags, MethodParam, Multiname, Namespace, Op, Script,
    };

    struct Provider;

    impl ExternalInterfaceProvider for Provider {
        fn call_method(&self, _: &mut ruffle_core::context::UpdateContext<'_>, _: &str, _: &[ExternalValue]) -> ExternalValue {
            ExternalValue::Undefined
        }
        fn on_callback_available(&self, _: &str) {}
        fn get_id(&self) -> Option<String> {
            Some("avm2-path-adapter-test".into())
        }
    }

    fn body(method: u32, max_stack: u32, num_locals: u32, ops: &[Op]) -> MethodBody {
        MethodBody {
            method: Index::new(method),
            max_stack,
            num_locals,
            init_scope_depth: 1,
            max_scope_depth: 2,
            code: write_ops(ops).unwrap(),
            exceptions: Vec::new(),
            traits: Vec::new(),
        }
    }

    fn method(params: usize, body: u32) -> Method {
        Method {
            name: Index::new(0),
            params: (0..params).map(|_| MethodParam { name: None, kind: Index::new(0), default_value: None }).collect(),
            return_type: Index::new(0),
            flags: MethodFlags::empty(),
            body: Some(Index::new(body)),
        }
    }

    /// The driver: hands the adapter's installer a stand-in movie root, as the patched lifecycle
    /// constructor does — `{BGSCodeObj: {}, onCodeObjCreate: function () {},
    /// LeftMeters_mc: {HPMeter_mc: {MeterBar_mc: {Percent: 1}}}, value: 0,
    /// bump: function (n) { this.value = n; return n + 1; }}`.
    fn driver_abc() -> Vec<u8> {
        let strings: Vec<Vec<u8>> = [
            "", "BGSCodeObj", "onCodeObjCreate", INSTALL_HELPER, "LeftMeters_mc", "HPMeter_mc", "MeterBar_mc", "Percent", "value",
            "bump",
        ]
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();
        let s = |i: u32| Index::new(i + 1);
        let public = Index::new(1);
        let install = Index::new(1);
        let value = Index::new(2);
        let abc = AbcFile {
            major_version: 46,
            minor_version: 16,
            constant_pool: ConstantPool {
                ints: Vec::new(),
                uints: Vec::new(),
                doubles: Vec::new(),
                strings,
                namespaces: vec![Namespace::Package(s(0))],
                namespace_sets: Vec::new(),
                multinames: vec![
                    Multiname::QName { namespace: public, name: s(3) },
                    Multiname::QName { namespace: public, name: s(8) },
                ],
            },
            methods: vec![method(0, 0), method(0, 1), method(1, 2)],
            metadata: Vec::new(),
            instances: Vec::new(),
            classes: Vec::new(),
            scripts: vec![Script { init_method: Index::new(0), traits: Vec::new() }],
            method_bodies: vec![
                body(0, 16, 1, &[
                    Op::GetLocal { index: 0 },
                    Op::PushScope,
                    Op::FindPropStrict { index: install },
                    Op::PushString { value: s(1) },
                    Op::NewObject { num_args: 0 },
                    Op::PushString { value: s(2) },
                    Op::NewFunction { index: Index::new(1) },
                    Op::PushString { value: s(4) },
                    Op::PushString { value: s(5) },
                    Op::PushString { value: s(6) },
                    Op::PushString { value: s(7) },
                    Op::PushByte { value: 1 },
                    Op::NewObject { num_args: 1 },
                    Op::NewObject { num_args: 1 },
                    Op::NewObject { num_args: 1 },
                    Op::PushString { value: s(8) },
                    Op::PushByte { value: 0 },
                    Op::PushString { value: s(9) },
                    Op::NewFunction { index: Index::new(2) },
                    Op::NewObject { num_args: 5 },
                    Op::CallPropVoid { index: install, num_args: 1 },
                    Op::ReturnVoid,
                ]),
                body(1, 1, 1, &[Op::ReturnVoid]),
                body(2, 3, 2, &[
                    Op::GetLocal { index: 0 },
                    Op::GetLocal { index: 1 },
                    Op::SetProperty { index: value },
                    Op::GetLocal { index: 1 },
                    Op::Increment,
                    Op::ReturnValue,
                ]),
            ],
        };
        let mut bytes = Vec::new();
        swf::avm2::write::Writer::new(&mut bytes).write(abc).unwrap();
        bytes
    }

    fn start() -> Arc<Mutex<Player>> {
        let adapter = build_adapter_abc(ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2), &[], false).unwrap();
        let driver = driver_abc();
        let mut header = swf::Header::default_with_swf_version(15);
        header.frame_rate = swf::Fixed8::from_f32(30.0);
        header.num_frames = 1;
        let abc = |name: &'static str, data: &'static [u8]| {
            swf::Tag::DoAbc2(swf::DoAbc2 { flags: swf::DoAbc2Flag::empty(), name: swf::SwfStr::from_utf8_str(name), data })
        };
        let adapter: &'static [u8] = Box::leak(adapter.into_boxed_slice());
        let driver: &'static [u8] = Box::leak(driver.into_boxed_slice());
        let mut bytes = Vec::new();
        swf::write_swf(
            &header,
            &[
                swf::Tag::FileAttributes(swf::FileAttributes::IS_ACTION_SCRIPT_3),
                abc("adapter", adapter),
                abc("driver", driver),
                swf::Tag::ShowFrame,
            ],
            &mut bytes,
        )
        .unwrap();
        let movie = SwfMovieData::from_data(&bytes, "file:///avm2-adapter.swf".into(), None, None).expect("parse");
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

    /// M48 A6 (GUI-PATH-001/002) — the AVM2 adapter walks a dotted path from the movie root it was
    /// installed with: it reads, writes and invokes members, and an absent path is a typed result.
    #[test]
    fn avm2_adapter_reports_an_absent_path() {
        let player = start();
        const PERCENT: &str = "LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent";
        let number = |value: f64| PathOutcome::Found(ScaleformValue::Number(value));
        assert_eq!(run(&player, PathOp::Get(PERCENT.into())), number(1.0));
        assert_eq!(run(&player, PathOp::Set(PERCENT.into(), ScaleformValue::Number(0.35))), PathOutcome::Found(ScaleformValue::Undefined));
        assert_eq!(run(&player, PathOp::Get(PERCENT.into())), number(0.35));
        assert_eq!(run(&player, PathOp::Call("bump".into(), vec![ScaleformValue::Number(9.0)])), number(10.0));
        assert_eq!(run(&player, PathOp::Get("value".into())), number(9.0));
        // Absent: a missing segment, a missing member to set, a member that is not a function,
        // a walk through a primitive.
        assert_eq!(run(&player, PathOp::Get("LeftMeters_mc.Nope.Percent".into())), PathOutcome::Absent);
        assert_eq!(run(&player, PathOp::Set("LeftMeters_mc.HPMeter_mc.Nope".into(), ScaleformValue::Number(1.0))), PathOutcome::Absent);
        assert_eq!(run(&player, PathOp::Call("LeftMeters_mc".into(), Vec::new())), PathOutcome::Absent);
        assert_eq!(run(&player, PathOp::Get("value.deeper".into())), PathOutcome::Absent);
        // An object is found without its members crossing the boundary.
        assert_eq!(run(&player, PathOp::Get("LeftMeters_mc".into())), PathOutcome::Found(ScaleformValue::Undefined));
    }
}
