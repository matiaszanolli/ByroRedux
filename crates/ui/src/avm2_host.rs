//! Fallout 4's AVM2 native-object adapter.
//!
//! Vanilla menus create `root.BGSCodeObj` themselves. The game populates that
//! dynamic object with native functions, then calls `onCodeObjCreate` on the
//! root. Ruffle intentionally keeps its AVM2 object model private, so the
//! adapter is injected as ordinary ABC bytecode before Ruffle parses the SWF.
//! The lifecycle class constructor is patched immediately after it initializes
//! `BGSCodeObj`; that bootstrap fills the object and forwards each call through
//! ExternalInterface without relying on Ruffle's private AVM2 object model.

#[cfg(test)]
use std::collections::BTreeSet;

use swf::avm2::read::Reader;
use swf::avm2::types::{
    AbcFile, ConstantPool, Index, Method, MethodBody, MethodFlags, MethodParam, Multiname,
    Namespace, Op, Script, Trait, TraitKind,
};
use swf::avm2::write::Writer;
use swf::extensions::ReadSwfExt;
use swf::{decompress_swf, parse_swf, write_swf, DoAbc2, DoAbc2Flag, SwfStr, Tag};

use crate::ScaleformHostCatalog;

const HELPER_PREFIX: &str = "__byro_fallout4_host_";
pub(crate) const INSTALL_HELPER: &str = "__byro_fallout4_install";
const READY_HELPER: &str = "__byro_fallout4_ready";
const DESTROY_HELPER: &str = "__byro_fallout4_destroy";
pub(crate) const READY_CALLBACK: &str = "__byroBGSCodeObjReady";
pub(crate) const LOADED_CALLBACK: &str = "__byroBGSAdapterLoaded";
pub(crate) const DESTROY_CALLBACK: &str = "__byroBGSCodeObjDestroy";
pub(crate) const DESTROYED_EVENT: &str = "__byroBGSCodeObjDestroyed";
const ADAPTER_NAME: &str = "byroredux.fallout4.BGSCodeObj";

/// Preparation state for a profile's native ActionScript host object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleformHostObjectState {
    /// The selected profile communicates without an injected root object.
    NotRequired,
    /// This AVM2 movie does not declare Fallout 4's root-object contract.
    NotPresent,
    /// A callable adapter was injected and will install on the next AVM2 turn.
    AdapterInjected,
}

pub(crate) fn inject_host_object_adapter(
    swf_data: &[u8],
    catalog: ScaleformHostCatalog,
) -> Result<(Vec<u8>, ScaleformHostObjectState), String> {
    if catalog.host_object().is_none() {
        return Ok((swf_data.to_vec(), ScaleformHostObjectState::NotRequired));
    }

    let decompressed =
        decompress_swf(swf_data).map_err(|error| format!("decompressing SWF: {error}"))?;
    let mut movie = parse_swf(&decompressed).map_err(|error| format!("parsing SWF: {error}"))?;
    let declares_contract = movie.tags.iter().any(|tag| {
        let abc = match tag {
            Tag::DoAbc(data) => Some(*data),
            Tag::DoAbc2(do_abc) => Some(do_abc.data),
            _ => None,
        };
        abc.is_some_and(|abc| {
            contains_bytes(abc, b"BGSCodeObj") && contains_bytes(abc, b"onCodeObjCreate")
        })
    });
    if !declares_contract {
        return Ok((swf_data.to_vec(), ScaleformHostObjectState::NotPresent));
    }

    let object = catalog
        .host_object()
        .expect("contract scan requires a host-object profile");
    let mut root_abc_index = None;
    let mut root_class = None;
    for (index, tag) in movie.tags.iter().enumerate() {
        let data = match tag {
            Tag::DoAbc(data) => Some(*data),
            Tag::DoAbc2(do_abc) => Some(do_abc.data),
            _ => None,
        };
        let Some(data) = data else {
            continue;
        };
        let abc = Reader::new(data)
            .read()
            .map_err(|error| format!("parsing root ABC candidate: {error}"))?;
        let contract_class = abc.instances.iter().find(|instance| {
            [object.property, object.on_create, object.on_destroy]
                .into_iter()
                .all(|required| {
                    instance.traits.iter().any(|trait_| {
                        multiname_local_name(&abc, trait_.name).as_deref()
                            == Some(required.as_bytes())
                    })
                })
        });
        if let Some(contract_class) = contract_class {
            root_abc_index = Some(index);
            root_class = qualified_name(&abc, contract_class.name);
            break;
        }
    }
    let root_abc_index = root_abc_index
        .ok_or_else(|| "Fallout 4 BGSCodeObj lifecycle class was not found".to_string())?;
    let root_class =
        root_class.ok_or_else(|| "Fallout 4 lifecycle class has no qualified name".to_string())?;
    let root_abc = match &movie.tags[root_abc_index] {
        Tag::DoAbc(data) => *data,
        Tag::DoAbc2(do_abc) => do_abc.data,
        _ => unreachable!("root ABC index must reference an ABC tag"),
    };
    let patched_root_abc = patch_root_constructor(root_abc, &root_class)?;

    let adapter = build_adapter_abc(catalog)
        .map_err(|error| format!("building Fallout 4 AVM2 adapter: {error}"))?;
    let replacement = match &movie.tags[root_abc_index] {
        Tag::DoAbc(_) => Tag::DoAbc(&patched_root_abc),
        Tag::DoAbc2(do_abc) => Tag::DoAbc2(DoAbc2 {
            flags: do_abc.flags,
            name: do_abc.name,
            data: &patched_root_abc,
        }),
        _ => unreachable!("root ABC index must reference an ABC tag"),
    };
    movie.tags[root_abc_index] = replacement;
    let tag = Tag::DoAbc2(DoAbc2 {
        flags: DoAbc2Flag::empty(),
        name: SwfStr::from_utf8_str(ADAPTER_NAME),
        data: &adapter,
    });
    movie.tags.insert(root_abc_index, tag);

    let mut patched = Vec::new();
    write_swf(movie.header.swf_header(), &movie.tags, &mut patched)
        .map_err(|error| format!("serializing patched SWF: {error}"))?;
    Ok((patched, ScaleformHostObjectState::AdapterInjected))
}

fn qualified_name(abc: &AbcFile, index: Index<Multiname>) -> Option<Vec<u8>> {
    let multiname = abc
        .constant_pool
        .multinames
        .get(index.as_u30().checked_sub(1)? as usize)?;
    let (namespace, name) = match multiname {
        Multiname::QName { namespace, name } | Multiname::QNameA { namespace, name } => {
            (*namespace, *name)
        }
        _ => return None,
    };
    let name = abc
        .constant_pool
        .strings
        .get(name.as_u30().checked_sub(1)? as usize)?;
    let namespace = abc
        .constant_pool
        .namespaces
        .get(namespace.as_u30().checked_sub(1)? as usize)?;
    let namespace = match namespace {
        Namespace::Namespace(name)
        | Namespace::Package(name)
        | Namespace::PackageInternal(name)
        | Namespace::Protected(name)
        | Namespace::Explicit(name)
        | Namespace::StaticProtected(name)
        | Namespace::Private(name) => abc
            .constant_pool
            .strings
            .get(name.as_u30().checked_sub(1)? as usize)?,
    };

    if namespace.is_empty() {
        return Some(name.clone());
    }
    let mut qualified = namespace.clone();
    qualified.push(b'.');
    qualified.extend_from_slice(name);
    Some(qualified)
}

fn multiname_local_name(abc: &AbcFile, index: Index<Multiname>) -> Option<Vec<u8>> {
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

#[cfg(test)]
pub(crate) fn referenced_host_methods(swf_data: &[u8]) -> Result<BTreeSet<String>, String> {
    let decompressed =
        decompress_swf(swf_data).map_err(|error| format!("decompressing SWF: {error}"))?;
    let movie = parse_swf(&decompressed).map_err(|error| format!("parsing SWF: {error}"))?;
    let mut methods = BTreeSet::new();

    for tag in &movie.tags {
        let data = match tag {
            Tag::DoAbc(data) => Some(*data),
            Tag::DoAbc2(do_abc) => Some(do_abc.data),
            _ => None,
        };
        let Some(data) = data else {
            continue;
        };
        let abc = Reader::new(data)
            .read()
            .map_err(|error| format!("parsing AVM2 host-call inventory: {error}"))?;
        for body in &abc.method_bodies {
            let mut reader = Reader::new(&body.code);
            let mut ops = Vec::new();
            while !reader.as_slice().is_empty() {
                ops.push(
                    reader
                        .read_op()
                        .map_err(|error| format!("reading AVM2 host-call inventory: {error}"))?,
                );
            }
            for (index, op) in ops.iter().enumerate() {
                let receiver = match op {
                    Op::GetProperty { index }
                    | Op::GetLex { index }
                    | Op::FindPropStrict { index } => Some(*index),
                    _ => None,
                };
                if !receiver.is_some_and(|receiver| {
                    multiname_local_name(&abc, receiver).as_deref() == Some(b"BGSCodeObj")
                }) {
                    continue;
                }

                let method = method_called_with_bgs_receiver(&abc, &ops[index + 1..]);
                let Some(method) = method else {
                    continue;
                };
                if matches!(method.as_slice(), b"InitCodeObj" | b"ReleaseCodeObj") {
                    continue;
                }
                let method = String::from_utf8(method)
                    .map_err(|_| "non-UTF-8 BGSCodeObj method name".to_string())?;
                methods.insert(method);
            }
        }
    }
    Ok(methods)
}

#[cfg(test)]
#[derive(Clone)]
enum InventoryStackValue {
    BgsCodeObj,
    String(Vec<u8>),
    Other,
}

#[cfg(test)]
fn method_called_with_bgs_receiver(abc: &AbcFile, ops: &[Op]) -> Option<Vec<u8>> {
    for mut stack in [
        vec![InventoryStackValue::BgsCodeObj],
        vec![InventoryStackValue::Other, InventoryStackValue::BgsCodeObj],
    ] {
        for op in ops.iter().take(32) {
            match op {
                Op::PushString { value } => {
                    let value = abc
                        .constant_pool
                        .strings
                        .get(value.as_u30().checked_sub(1)? as usize)?
                        .clone();
                    stack.push(InventoryStackValue::String(value));
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
                | Op::PushNaN => stack.push(InventoryStackValue::Other),
                Op::GetProperty { .. } | Op::GetSlot { .. } => {
                    stack.pop()?;
                    stack.push(InventoryStackValue::Other);
                }
                Op::SetLocal { .. } | Op::Pop => {
                    stack.pop()?;
                }
                Op::Dup => stack.push(stack.last()?.clone()),
                Op::Swap => {
                    let length = stack.len();
                    if length < 2 {
                        break;
                    }
                    stack.swap(length - 1, length - 2);
                }
                Op::CallProperty { index, num_args } | Op::CallPropVoid { index, num_args } => {
                    let num_args = *num_args as usize;
                    if stack.len() < num_args + 1 {
                        break;
                    }
                    let arguments = stack.split_off(stack.len() - num_args);
                    let receiver = stack.pop()?;
                    let call_name = multiname_local_name(abc, *index)?;
                    if matches!(receiver, InventoryStackValue::BgsCodeObj) && call_name != b"call" {
                        return Some(call_name);
                    }
                    if call_name == b"call"
                        && matches!(arguments.first(), Some(InventoryStackValue::BgsCodeObj))
                    {
                        if let Some(InventoryStackValue::String(method)) = arguments.get(1) {
                            return Some(method.clone());
                        }
                    }
                    if matches!(op, Op::CallProperty { .. }) {
                        stack.push(InventoryStackValue::Other);
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

/// #2716 (SAFEUI-02) — the injected 3-op bootstrap (`FindPropStrict` +1,
/// `GetLocal` +1 -> peak +2, `CallPropVoid`) needs two slots ABOVE
/// whatever depth `D` the operand stack is already at when it splices in,
/// i.e. `max_stack` must become at least `D + 2`. The pre-fix
/// `existing_max_stack.max(2)` only guarantees `max_stack >= 2`, which
/// every real constructor already satisfies on its own (an `InitProperty`
/// alone consumes three operands) — a no-op that happened to be correct
/// only because BGSCodeObj's init is emitted at statement level (`D == 0`)
/// by the compiler in every corpus example seen so far. A constructor
/// that initializes BGSCodeObj inside an expression (legal ABC, `D > 0`)
/// would overflow the AVM2 operand stack — a Rust index-out-of-bounds
/// panic in Ruffle's interpreter, since Ruffle's verifier does not
/// reconcile declared `max_stack` against actual (`core/src/avm2/verify.rs`
/// has no reference to `max_stack`; the frame is sized once from it in
/// `Stack::get_stack_frame`). `saturating_add` is correct for any `D`:
/// `existing_max_stack` already reflects the deepest point BEFORE the
/// splice, so adding the injection's own peak growth on top bounds the
/// deepest point AFTER it, regardless of where in the body that peak was.
/// Split out (pure, no `MethodBody`) so unit tests can pin the arithmetic
/// without reconstructing a full ABC.
fn max_stack_with_injected_bootstrap_headroom(existing_max_stack: u32) -> u32 {
    existing_max_stack.saturating_add(2)
}

fn patch_root_constructor(abc_data: &[u8], root_class: &[u8]) -> Result<Vec<u8>, String> {
    let mut abc = Reader::new(abc_data)
        .read()
        .map_err(|error| format!("parsing Fallout 4 root ABC: {error}"))?;
    let instance = abc
        .instances
        .iter()
        .find(|instance| qualified_name(&abc, instance.name).as_deref() == Some(root_class))
        .ok_or_else(|| "root class disappeared while patching its ABC".to_string())?;
    let init_method = instance.init_method.as_u30() as usize;
    let body_index = abc
        .methods
        .get(init_method)
        .and_then(|method| method.body)
        .ok_or_else(|| "Fallout 4 root constructor has no method body".to_string())?
        .as_u30() as usize;
    let insertion_offset = {
        let body = abc
            .method_bodies
            .get(body_index)
            .ok_or_else(|| "Fallout 4 root constructor body index is invalid".to_string())?;
        let mut reader = Reader::new(&body.code);
        let mut insertion_offset = None;
        while !reader.as_slice().is_empty() {
            let op = reader
                .read_op()
                .map_err(|error| format!("reading Fallout 4 root constructor: {error}"))?;
            let property = match op {
                Op::InitProperty { index } | Op::SetProperty { index } => Some(index),
                _ => None,
            };
            if property.is_some_and(|property| {
                multiname_local_name(&abc, property).as_deref() == Some(b"BGSCodeObj")
            }) {
                insertion_offset = Some(body.code.len() - reader.as_slice().len());
                break;
            }
        }
        insertion_offset.ok_or_else(|| {
            "Fallout 4 root constructor does not initialize BGSCodeObj".to_string()
        })?
    };
    {
        let body = &abc.method_bodies[body_index];
        let mut reader = Reader::new(&body.code);
        while !reader.as_slice().is_empty() {
            let start = body.code.len() - reader.as_slice().len();
            let op = reader
                .read_op()
                .map_err(|error| format!("validating Fallout 4 root constructor: {error}"))?;
            let end = body.code.len() - reader.as_slice().len();
            let crosses_insertion = |target: i64| {
                (start < insertion_offset && target >= insertion_offset as i64)
                    || (start >= insertion_offset && target < insertion_offset as i64)
            };
            let crosses = match op {
                Op::IfEq { offset }
                | Op::IfFalse { offset }
                | Op::IfGe { offset }
                | Op::IfGt { offset }
                | Op::IfLe { offset }
                | Op::IfLt { offset }
                | Op::IfNe { offset }
                | Op::IfNge { offset }
                | Op::IfNgt { offset }
                | Op::IfNle { offset }
                | Op::IfNlt { offset }
                | Op::IfStrictEq { offset }
                | Op::IfStrictNe { offset }
                | Op::IfTrue { offset }
                | Op::Jump { offset } => crosses_insertion(end as i64 + offset as i64),
                Op::LookupSwitch(switch) => {
                    crosses_insertion(start as i64 + switch.default_offset as i64)
                        || switch
                            .case_offsets
                            .iter()
                            .any(|offset| crosses_insertion(start as i64 + *offset as i64))
                }
                _ => false,
            };
            if crosses {
                return Err(
                    "Fallout 4 root constructor branches across BGSCodeObj initialization"
                        .to_string(),
                );
            }
        }
    }

    let empty_string = add_string(&mut abc.constant_pool.strings, Vec::new());
    abc.constant_pool
        .namespaces
        .push(Namespace::Package(empty_string));
    let public_namespace = Index::new(abc.constant_pool.namespaces.len() as u32);
    let install_string = add_string(
        &mut abc.constant_pool.strings,
        INSTALL_HELPER.as_bytes().to_vec(),
    );
    let install = add_multiname(
        &mut abc.constant_pool.multinames,
        Multiname::QName {
            namespace: public_namespace,
            name: install_string,
        },
    );

    let body = abc
        .method_bodies
        .get_mut(body_index)
        .ok_or_else(|| "Fallout 4 root constructor body index is invalid".to_string())?;
    let injection = write_ops(&[
        Op::FindPropStrict { index: install },
        Op::GetLocal { index: 0 },
        Op::CallPropVoid {
            index: install,
            num_args: 1,
        },
    ])
    .map_err(|error| format!("writing Fallout 4 root bootstrap: {error}"))?;
    body.code.splice(
        insertion_offset..insertion_offset,
        injection.iter().copied(),
    );
    body.max_stack = max_stack_with_injected_bootstrap_headroom(body.max_stack);
    let inserted = injection.len() as u32;
    let insertion_offset = insertion_offset as u32;
    for exception in &mut body.exceptions {
        if exception.from_offset >= insertion_offset {
            exception.from_offset += inserted;
        }
        if exception.to_offset >= insertion_offset {
            exception.to_offset += inserted;
        }
        if exception.target_offset >= insertion_offset {
            exception.target_offset += inserted;
        }
    }

    let mut bytes = Vec::new();
    Writer::new(&mut bytes)
        .write(abc)
        .map_err(|error| format!("serializing patched Fallout 4 root ABC: {error}"))?;
    Ok(bytes)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn helper_name(index: usize) -> String {
    format!("{HELPER_PREFIX}{index}")
}

fn build_adapter_abc(catalog: ScaleformHostCatalog) -> std::io::Result<Vec<u8>> {
    let object = catalog
        .host_object()
        .expect("AVM2 adapter requires a host-object profile");
    let mut strings = vec![
        Vec::new(),
        b"flash.external".to_vec(),
        b"flash.display".to_vec(),
        b"ExternalInterface".to_vec(),
        b"LoaderInfo".to_vec(),
        b"call".to_vec(),
        b"apply".to_vec(),
        b"unshift".to_vec(),
        b"getLoaderInfoByDefinition".to_vec(),
        b"addEventListener".to_vec(),
        b"target".to_vec(),
        b"content".to_vec(),
        b"complete".to_vec(),
        object.property.as_bytes().to_vec(),
        object.on_create.as_bytes().to_vec(),
        INSTALL_HELPER.as_bytes().to_vec(),
        b"addCallback".to_vec(),
        READY_HELPER.as_bytes().to_vec(),
        READY_CALLBACK.as_bytes().to_vec(),
        b"flash.utils".to_vec(),
        b"setTimeout".to_vec(),
        LOADED_CALLBACK.as_bytes().to_vec(),
        object.on_destroy.as_bytes().to_vec(),
        DESTROY_HELPER.as_bytes().to_vec(),
        DESTROY_CALLBACK.as_bytes().to_vec(),
        DESTROYED_EVENT.as_bytes().to_vec(),
        b"__byro_fallout4_root".to_vec(),
    ];
    let mut multinames = vec![
        qname(2, 4),  // flash.external::ExternalInterface
        qname(3, 5),  // flash.display::LoaderInfo
        qname(1, 6),  // call
        qname(1, 7),  // apply
        qname(1, 8),  // unshift
        qname(1, 9),  // getLoaderInfoByDefinition
        qname(1, 10), // addEventListener
        qname(1, 11), // target
        qname(1, 12), // content
        qname(1, 14), // BGSCodeObj
        qname(1, 15), // onCodeObjCreate
        qname(1, 16), // install helper
        qname(1, 17), // addCallback
        qname(1, 18), // ready helper
        qname(4, 21), // flash.utils::setTimeout
        qname(1, 23), // onCodeObjDestruction
        qname(1, 24), // destroy helper
    ];
    let external_interface = Index::new(1);
    let call = Index::new(3);
    let apply = Index::new(4);
    let unshift = Index::new(5);
    let code_object = Index::new(10);
    let on_create = Index::new(11);
    let install_helper = Index::new(12);
    let add_callback = Index::new(13);
    let ready_helper = Index::new(14);
    let on_destroy = Index::new(16);
    let destroy_helper = Index::new(17);
    let ready_callback_string = Index::new(19);
    let loaded_callback_string = Index::new(22);
    let destroy_callback_string = Index::new(25);
    let destroyed_event_string = Index::new(26);
    let root_slot_string: Index<String> = Index::new(27);
    let root_slot = add_multiname(&mut multinames, qname(1, root_slot_string.0));

    let mut methods = Vec::with_capacity(catalog.len() + 4);
    let mut method_bodies = Vec::with_capacity(catalog.len() + 4);
    let mut traits = Vec::with_capacity(catalog.len() + 4);
    let mut helper_multinames = Vec::with_capacity(catalog.len());
    let mut method_property_multinames = Vec::with_capacity(catalog.len());
    traits.push(Trait {
        name: root_slot,
        kind: TraitKind::Slot {
            slot_id: 1,
            type_name: Index::new(0),
            value: None,
        },
        metadata: Vec::new(),
        is_final: false,
        is_override: false,
    });

    for (index, method) in catalog.methods().iter().enumerate() {
        let helper_string = add_string(&mut strings, helper_name(index));
        let transport_string =
            add_string(&mut strings, format!("{}.{}", object.property, method.name));
        let method_property_string = add_string(&mut strings, method.name);

        let helper_multiname = add_multiname(&mut multinames, qname(1, helper_string.0));
        let method_property = add_multiname(&mut multinames, qname(1, method_property_string.0));
        helper_multinames.push(helper_multiname);
        method_property_multinames.push(method_property);

        let method_index = Index::new(methods.len() as u32);
        let body_index = Index::new(method_bodies.len() as u32);
        methods.push(Method {
            name: helper_string,
            params: Vec::new(),
            return_type: Index::new(0),
            flags: MethodFlags::NEED_REST,
            body: Some(body_index),
        });
        method_bodies.push(MethodBody {
            method: method_index,
            max_stack: 3,
            num_locals: 2,
            init_scope_depth: 1,
            max_scope_depth: 2,
            code: write_ops(&[
                Op::GetLocal { index: 0 },
                Op::PushScope,
                Op::GetLocal { index: 1 },
                Op::PushString {
                    value: transport_string,
                },
                Op::CallPropVoid {
                    index: unshift,
                    num_args: 1,
                },
                Op::GetLex {
                    index: external_interface,
                },
                Op::GetProperty { index: call },
                Op::PushNull,
                Op::GetLocal { index: 1 },
                Op::CallProperty {
                    index: apply,
                    num_args: 2,
                },
                Op::ReturnValue,
            ])?,
            exceptions: Vec::new(),
            traits: Vec::new(),
        });
        traits.push(method_trait(
            helper_multiname,
            method_index,
            index as u32 + 1,
        ));
    }

    let ready_method = Index::new(methods.len() as u32);
    let ready_body = Index::new(method_bodies.len() as u32);
    methods.push(Method {
        name: Index::new(18),
        params: Vec::new(),
        return_type: Index::new(0),
        flags: MethodFlags::empty(),
        body: Some(ready_body),
    });
    method_bodies.push(MethodBody {
        method: ready_method,
        max_stack: 1,
        num_locals: 1,
        init_scope_depth: 1,
        max_scope_depth: 2,
        code: write_ops(&[
            Op::GetLocal { index: 0 },
            Op::PushScope,
            Op::PushTrue,
            Op::ReturnValue,
        ])?,
        exceptions: Vec::new(),
        traits: Vec::new(),
    });
    traits.push(method_trait(
        ready_helper,
        ready_method,
        catalog.len() as u32 + 1,
    ));

    let destroy_method = Index::new(methods.len() as u32);
    let destroy_body = Index::new(method_bodies.len() as u32);
    methods.push(Method {
        name: Index::new(24),
        params: Vec::new(),
        return_type: Index::new(0),
        flags: MethodFlags::empty(),
        body: Some(destroy_body),
    });
    method_bodies.push(MethodBody {
        method: destroy_method,
        max_stack: 2,
        num_locals: 1,
        init_scope_depth: 1,
        max_scope_depth: 2,
        code: write_ops(&[
            Op::GetLocal { index: 0 },
            Op::PushScope,
            Op::GetLex { index: root_slot },
            Op::CallPropVoid {
                index: on_destroy,
                num_args: 0,
            },
            Op::GetLex {
                index: external_interface,
            },
            Op::PushString {
                value: destroyed_event_string,
            },
            Op::CallPropVoid {
                index: call,
                num_args: 1,
            },
            Op::ReturnVoid,
        ])?,
        exceptions: Vec::new(),
        traits: Vec::new(),
    });
    traits.push(method_trait(
        destroy_helper,
        destroy_method,
        catalog.len() as u32 + 2,
    ));

    let installer_method = Index::new(methods.len() as u32);
    let installer_body = Index::new(method_bodies.len() as u32);
    methods.push(Method {
        name: Index::new(16),
        params: vec![MethodParam {
            name: None,
            kind: Index::new(0),
            default_value: None,
        }],
        return_type: Index::new(0),
        flags: MethodFlags::empty(),
        body: Some(installer_body),
    });
    let mut install_ops = vec![
        Op::GetLocal { index: 0 },
        Op::PushScope,
        Op::GetGlobalScope,
        Op::GetLocal { index: 1 },
        Op::SetProperty { index: root_slot },
        Op::GetLocal { index: 1 },
        Op::GetProperty { index: code_object },
        Op::SetLocal { index: 2 },
    ];
    for (helper, property) in helper_multinames.iter().zip(&method_property_multinames) {
        install_ops.extend([
            Op::GetLocal { index: 2 },
            Op::GetLex { index: *helper },
            Op::SetProperty { index: *property },
        ]);
    }
    install_ops.extend([
        Op::GetLocal { index: 1 },
        Op::CallPropVoid {
            index: on_create,
            num_args: 0,
        },
        Op::GetLex {
            index: external_interface,
        },
        Op::PushString {
            value: ready_callback_string,
        },
        Op::GetLex {
            index: ready_helper,
        },
        Op::CallPropVoid {
            index: add_callback,
            num_args: 2,
        },
        Op::GetLex {
            index: external_interface,
        },
        Op::PushString {
            value: destroy_callback_string,
        },
        Op::GetLex {
            index: destroy_helper,
        },
        Op::CallPropVoid {
            index: add_callback,
            num_args: 2,
        },
        Op::ReturnVoid,
    ]);
    method_bodies.push(MethodBody {
        method: installer_method,
        max_stack: 3,
        num_locals: 3,
        init_scope_depth: 1,
        max_scope_depth: 2,
        code: write_ops(&install_ops)?,
        exceptions: Vec::new(),
        traits: Vec::new(),
    });
    traits.push(method_trait(
        install_helper,
        installer_method,
        catalog.len() as u32 + 3,
    ));

    let init_method = Index::new(methods.len() as u32);
    let init_body = Index::new(method_bodies.len() as u32);
    methods.push(Method {
        name: Index::new(0),
        params: Vec::new(),
        return_type: Index::new(0),
        flags: MethodFlags::empty(),
        body: Some(init_body),
    });
    method_bodies.push(MethodBody {
        method: init_method,
        max_stack: 3,
        num_locals: 1,
        init_scope_depth: 1,
        max_scope_depth: 2,
        code: write_ops(&[
            Op::GetLocal { index: 0 },
            Op::PushScope,
            Op::GetLex {
                index: external_interface,
            },
            Op::PushString {
                value: loaded_callback_string,
            },
            Op::GetLex {
                index: ready_helper,
            },
            Op::CallPropVoid {
                index: add_callback,
                num_args: 2,
            },
            Op::ReturnVoid,
        ])?,
        exceptions: Vec::new(),
        traits: Vec::new(),
    });

    let abc = AbcFile {
        major_version: 46,
        minor_version: 16,
        constant_pool: ConstantPool {
            ints: Vec::new(),
            uints: Vec::new(),
            doubles: Vec::new(),
            strings,
            namespaces: vec![
                Namespace::Package(Index::new(1)),
                Namespace::Package(Index::new(2)),
                Namespace::Package(Index::new(3)),
                Namespace::Package(Index::new(20)),
            ],
            namespace_sets: Vec::new(),
            multinames,
        },
        methods,
        metadata: Vec::new(),
        instances: Vec::new(),
        classes: Vec::new(),
        scripts: vec![Script {
            init_method,
            traits,
        }],
        method_bodies,
    };

    let mut bytes = Vec::new();
    Writer::new(&mut bytes).write(abc)?;
    Ok(bytes)
}

fn qname(namespace: u32, name: u32) -> Multiname {
    Multiname::QName {
        namespace: Index::new(namespace),
        name: Index::new(name),
    }
}

fn add_string(strings: &mut Vec<Vec<u8>>, value: impl Into<Vec<u8>>) -> Index<String> {
    strings.push(value.into());
    Index::new(strings.len() as u32)
}

fn add_multiname(multinames: &mut Vec<Multiname>, value: Multiname) -> Index<Multiname> {
    multinames.push(value);
    Index::new(multinames.len() as u32)
}

fn method_trait(name: Index<Multiname>, method: Index<Method>, disp_id: u32) -> Trait {
    Trait {
        name,
        kind: TraitKind::Method { disp_id, method },
        metadata: Vec::new(),
        is_final: false,
        is_override: false,
    }
}

fn write_ops(ops: &[Op]) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes);
    for op in ops {
        writer.write_op(op)?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use byroredux_bsa::Ba2Archive;
    use ruffle_core::swf::avm2::read::Reader;
    use swf::avm2::types::{AbcFile, ConstantPool, Index, Method, MethodBody, MethodFlags, Script};
    use swf::avm2::write::Writer;
    use swf::extensions::ReadSwfExt;
    use swf::{DoAbc2, DoAbc2Flag, FileAttributes, SwfStr, Tag};

    use super::{
        build_adapter_abc, inject_host_object_adapter, max_stack_with_injected_bootstrap_headroom,
        referenced_host_methods, write_ops, DESTROYED_EVENT,
    };
    use crate::{ScaleformHostCatalog, ScaleformProfile};

    #[test]
    fn generated_adapter_is_valid_abc_with_one_helper_per_method() {
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let bytes = build_adapter_abc(catalog).unwrap();
        let abc = Reader::new(&bytes).read().unwrap();

        assert_eq!(abc.scripts.len(), 1);
        assert_eq!(abc.scripts[0].traits.len(), catalog.len() + 4);
        assert_eq!(abc.methods.len(), catalog.len() + 4);
        assert_eq!(abc.method_bodies.len(), catalog.len() + 4);

        let installer = &abc.method_bodies[catalog.len() + 2];
        let mut reader = Reader::new(&installer.code);
        let mut installed_properties = 0;
        let mut void_calls = 0;
        while !reader.as_slice().is_empty() {
            match reader.read_op().unwrap() {
                swf::avm2::types::Op::SetProperty { .. } => installed_properties += 1,
                swf::avm2::types::Op::CallPropVoid { .. } => void_calls += 1,
                _ => {}
            }
        }
        assert_eq!(installed_properties, catalog.len() + 1);
        assert_eq!(void_calls, 3);

        let destroy = &abc.method_bodies[catalog.len() + 1];
        let mut reader = Reader::new(&destroy.code);
        let mut void_calls = 0;
        while !reader.as_slice().is_empty() {
            if matches!(
                reader.read_op().unwrap(),
                swf::avm2::types::Op::CallPropVoid { .. }
            ) {
                void_calls += 1;
            }
        }
        assert_eq!(void_calls, 2);
        assert!(abc
            .constant_pool
            .strings
            .iter()
            .any(|string| string.as_slice() == DESTROYED_EVENT.as_bytes()));

        let init = &abc.method_bodies[catalog.len() + 3];
        let mut reader = Reader::new(&init.code);
        let mut callback_names = Vec::new();
        while !reader.as_slice().is_empty() {
            if let swf::avm2::types::Op::PushString { value } = reader.read_op().unwrap() {
                callback_names.push(value.0);
            }
        }
        assert_eq!(callback_names, [22]);
    }

    #[test]
    #[ignore = "requires an installed Fallout 4 corpus"]
    fn installed_fallout4_host_calls_are_cataloged() {
        let path = std::env::var("BYROREDUX_FO4_DATA").unwrap_or_else(|_| {
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data".to_string()
        });
        let archive =
            Ba2Archive::open(std::path::Path::new(&path).join("Fallout4 - Interface.ba2")).unwrap();
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);

        for movie in [
            "interface\\hudmenu.swf",
            "interface\\pipboymenu.swf",
            "programs\\atomiccommand.swf",
        ] {
            let swf = archive.extract(movie).unwrap();
            let methods = referenced_host_methods(&swf).unwrap();
            let unknown = methods
                .iter()
                .filter(|method| !catalog.contains(method))
                .cloned()
                .collect::<Vec<_>>();
            assert!(
                unknown.is_empty(),
                "{movie} references uncataloged BGSCodeObj methods: {unknown:?}; all={methods:?}"
            );
        }
    }

    /// Regression for #2717 / SAFEUI-03: `inject_host_object_adapter`
    /// fully parses and re-serializes the ENTIRE movie (every font,
    /// bitmap, sprite, sound, shape tag decoded and re-encoded, not
    /// copied) rather than the raw-tag-splice strategy its sibling
    /// `navigator.rs::prepare_import_asset_swf` uses. Before this test the
    /// only real-data evidence for "lossless on FO4 content" was 2 of 311
    /// menus, checked by an `#[ignore]`d test that never runs in CI. This
    /// sweeps every `.swf` in the FO4 interface archive through
    /// injection and asserts the output re-parses — NOT gated behind
    /// `#[ignore]` (so the corpus is actually swept whenever it's
    /// present), but self-skips (rather than failing) when the archive
    /// isn't available, since the corpus is multi-gigabyte game data that
    /// can't ship with the repo.
    ///
    /// Four menus (`KNOWN_MISSING_ON_DESTROY_TRAIT` below) are excluded
    /// from the "must succeed" assertion: they were discovered by an
    /// earlier run of *this* test to fail the *unrelated*, pre-existing
    /// lifecycle-class search — a separate bug from what #2717 is about.
    /// `Shared.ScaleformHostCatalog`'s Fallout 4 profile requires a class
    /// whose OWN traits include the host-object property AND both the
    /// on-create AND on-destroy hooks; these four menus' lifecycle class
    /// (`DialogueMenu`, `MultiActivateMenu`, `SPECIALMenu`, `Terminal`)
    /// declares the property and on-create hook but never an
    /// `onCodeObjDestruction` trait of its own — legal, real, shipped ABC.
    /// The lookup falls through to "lifecycle class not found",
    /// `inject_host_object_adapter` returns `Err`, and every caller
    /// propagates that `Err` (`SwfPlayer::new` / `new_with_profile` /
    /// `from_resource_provider`, `player.rs`) — so today these 4 real
    /// Fallout 4 menus fail to load entirely. That is a real, separate
    /// defect worth its own issue (loosen the on-destroy requirement, or
    /// search inherited traits) — NOT something #2717's round-trip
    /// serialization safety fix should touch, and not something this test
    /// should silently paper over either, hence naming it explicitly
    /// instead of a bare allow-list.
    #[test]
    fn all_installed_fallout4_swfs_round_trip_through_injection() {
        const KNOWN_MISSING_ON_DESTROY_TRAIT: &[&str] = &[
            "interface\\dialoguemenu.swf",
            "interface\\multiactivatemenu.swf",
            "interface\\specialmenu.swf",
            "interface\\terminalmenu.swf",
        ];

        let path = std::env::var("BYROREDUX_FO4_DATA").unwrap_or_else(|_| {
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data".to_string()
        });
        let archive_path = std::path::Path::new(&path).join("Fallout4 - Interface.ba2");
        let Ok(archive) = Ba2Archive::open(&archive_path) else {
            eprintln!(
                "skipping all_installed_fallout4_swfs_round_trip_through_injection: \
                 {archive_path:?} not available (set BYROREDUX_FO4_DATA to override)"
            );
            return;
        };
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);

        let mut swf_paths: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|path| path.ends_with(".swf"))
            .map(str::to_string)
            .collect();
        swf_paths.sort();
        assert!(
            !swf_paths.is_empty(),
            "expected .swf files in the FO4 interface archive at {archive_path:?}"
        );

        let mut injected_count = 0usize;
        let mut failures = Vec::new();
        let mut still_missing_on_destroy = Vec::new();
        for movie_path in &swf_paths {
            let swf = match archive.extract(movie_path) {
                Ok(data) => data,
                Err(error) => {
                    failures.push(format!("{movie_path}: extract failed: {error}"));
                    continue;
                }
            };
            match inject_host_object_adapter(&swf, catalog) {
                Ok((patched, state)) => {
                    if state == super::ScaleformHostObjectState::AdapterInjected {
                        injected_count += 1;
                        // The structural sanity check #2717 asks for: the
                        // fully re-serialized movie must still be a valid
                        // SWF. This does NOT prove byte-for-byte losslessness
                        // (that needs the raw-tag-splice rewrite the issue's
                        // primary suggested fix describes) — it proves the
                        // `swf` crate's write path didn't produce a movie
                        // that can no longer be parsed at all, which is the
                        // failure mode a corrupted re-encode would produce.
                        let reparsed = swf::decompress_swf(patched.as_slice())
                            .map_err(|error| error.to_string())
                            .and_then(|decompressed| {
                                swf::parse_swf(&decompressed)
                                    .map_err(|error| error.to_string())
                                    .map(|_movie| ())
                            });
                        if let Err(error) = reparsed {
                            failures.push(format!(
                                "{movie_path}: patched output failed to re-parse: {error}"
                            ));
                        }
                    }
                }
                Err(error) => {
                    if KNOWN_MISSING_ON_DESTROY_TRAIT.contains(&movie_path.as_str())
                        && error.contains("lifecycle class was not found")
                    {
                        still_missing_on_destroy.push(movie_path.clone());
                    } else {
                        failures.push(format!("{movie_path}: injection failed: {error}"));
                    }
                }
            }
        }

        assert!(
            failures.is_empty(),
            "{} of {} FO4 SWFs failed the injection round-trip ({injected_count} actually \
             went through AVM2 adapter injection; {} excluded as the known, unrelated \
             on-destroy-trait gap):\n{}",
            failures.len(),
            swf_paths.len(),
            still_missing_on_destroy.len(),
            failures.join("\n"),
        );
        // If this list has shrunk, the on-destroy gap got fixed (or the
        // menu changed) — update KNOWN_MISSING_ON_DESTROY_TRAIT and the
        // doc comment above rather than leaving a stale exclusion. If it
        // has GROWN, a new menu just started hitting the same gap.
        assert_eq!(
            still_missing_on_destroy,
            KNOWN_MISSING_ON_DESTROY_TRAIT,
            "the set of menus hitting the known on-destroy-trait gap changed — \
             update KNOWN_MISSING_ON_DESTROY_TRAIT (and file/link the follow-up \
             issue this comment references) rather than leaving this stale"
        );
    }

    #[test]
    fn marker_without_a_lifecycle_class_is_rejected() {
        let marker_abc = marker_abc();
        let mut header = swf::Header::default_with_swf_version(15);
        header.num_frames = 1;
        header.frame_rate = swf::Fixed8::from_f32(30.0);
        let tags = [
            Tag::FileAttributes(FileAttributes::IS_ACTION_SCRIPT_3),
            Tag::DoAbc2(DoAbc2 {
                flags: DoAbc2Flag::empty(),
                name: SwfStr::from_utf8_str("contract-marker"),
                data: &marker_abc,
            }),
            Tag::ShowFrame,
        ];
        let mut source = Vec::new();
        swf::write_swf(&header, &tags, &mut source).unwrap();

        let error = inject_host_object_adapter(
            &source,
            ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2),
        )
        .unwrap_err();
        assert!(error.contains("lifecycle class"));
    }

    fn marker_abc() -> Vec<u8> {
        let method = Method {
            name: Index::new(0),
            params: Vec::new(),
            return_type: Index::new(0),
            flags: MethodFlags::empty(),
            body: Some(Index::new(0)),
        };
        let abc = AbcFile {
            major_version: 46,
            minor_version: 16,
            constant_pool: ConstantPool {
                ints: Vec::new(),
                uints: Vec::new(),
                doubles: Vec::new(),
                strings: vec![b"BGSCodeObj".to_vec(), b"onCodeObjCreate".to_vec()],
                namespaces: Vec::new(),
                namespace_sets: Vec::new(),
                multinames: Vec::new(),
            },
            methods: vec![method],
            metadata: Vec::new(),
            instances: Vec::new(),
            classes: Vec::new(),
            scripts: vec![Script {
                init_method: Index::new(0),
                traits: Vec::new(),
            }],
            method_bodies: vec![MethodBody {
                method: Index::new(0),
                max_stack: 1,
                num_locals: 1,
                init_scope_depth: 1,
                max_scope_depth: 2,
                code: write_ops(&[
                    swf::avm2::types::Op::GetLocal { index: 0 },
                    swf::avm2::types::Op::PushScope,
                    swf::avm2::types::Op::ReturnVoid,
                ])
                .unwrap(),
                exceptions: Vec::new(),
                traits: Vec::new(),
            }],
        };
        let mut bytes = Vec::new();
        Writer::new(&mut bytes).write(abc).unwrap();
        bytes
    }

    /// Regression for #2716 / SAFEUI-02: the injected bootstrap needs
    /// `existing_max_stack + 2` headroom for ANY existing depth, not just
    /// `max(existing_max_stack, 2)`. The pre-fix formula was a no-op for
    /// every constructor that already declares `max_stack >= 2` — which is
    /// every real one (an `InitProperty` alone needs three operands) — so
    /// pin the case that formula silently got wrong: a constructor whose
    /// declared depth already exceeds 2 must still grow by exactly 2, not
    /// stay unchanged.
    #[test]
    fn injected_bootstrap_headroom_adds_two_regardless_of_existing_depth() {
        for existing in [0u32, 1, 2, 3, 5, 10] {
            let fixed = max_stack_with_injected_bootstrap_headroom(existing);
            assert_eq!(
                fixed,
                existing + 2,
                "existing_max_stack={existing}: bootstrap needs exactly 2 \
                 slots of headroom above whatever depth the constructor \
                 already reaches"
            );

            // The retired `.max(2)` formula this replaces: correct only by
            // coincidence at existing <= 2, and silently wrong (stays flat
            // instead of growing) for every existing depth above that —
            // exactly the BGSCodeObj-inside-an-expression (D > 0) shape
            // #2716 describes.
            let retired_formula = existing.max(2);
            if existing > 2 {
                assert_ne!(
                    fixed, retired_formula,
                    "existing_max_stack={existing}: the fixed formula must \
                     diverge from the retired one here, or this test isn't \
                     actually exercising the bug #2716 fixes"
                );
            }
        }
    }

    /// `saturating_add` must not wrap even at the type's ceiling — ABC
    /// `max_stack` is a `u30` in the format (so real inputs never
    /// approach `u32::MAX`), but the Rust field is a plain `u32` and the
    /// arithmetic operator must stay a deliberate, safe choice rather
    /// than an overflow-panicking `+`.
    #[test]
    fn injected_bootstrap_headroom_saturates_instead_of_wrapping() {
        assert_eq!(
            max_stack_with_injected_bootstrap_headroom(u32::MAX),
            u32::MAX
        );
        assert_eq!(
            max_stack_with_injected_bootstrap_headroom(u32::MAX - 1),
            u32::MAX
        );
    }
}
