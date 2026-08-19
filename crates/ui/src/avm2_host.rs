//! Fallout 4's AVM2 native-object adapter.
//!
//! Vanilla menus create `root.BGSCodeObj` themselves. The game populates that
//! dynamic object with native functions, then calls `onCodeObjCreate` on the
//! root. Ruffle intentionally keeps its AVM2 object model private, so the
//! adapter is injected as ordinary ABC bytecode before Ruffle parses the SWF.
//! The lifecycle class constructor is patched immediately after it initializes
//! `BGSCodeObj`; that bootstrap fills the object and forwards each call through
//! ExternalInterface without relying on Ruffle's private AVM2 object model.

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
        abc_payload(tag).is_some_and(|abc| {
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
    // #2963 — only `property` + `on_create` are required to identify the
    // lifecycle class. `on_destroy` (`onCodeObjDestruction`) is real, shipped
    // ABC's own choice to omit: DialogueMenu, MultiActivateMenu, SPECIALMenu
    // and Terminal all declare `BGSCodeObj` + `onCodeObjCreate` but never
    // their own `onCodeObjDestruction` trait. Requiring it here used to make
    // the whole menu fail to load (`Err` propagates through `SwfPlayer::new`)
    // over a hook nothing but the adapter's own optional destroy callback
    // ever calls. `has_destroy_trait` records whether this movie's class
    // carries the hook, so `build_adapter_abc` can wire (or skip) it.
    let mut has_destroy_trait = false;
    for (index, tag) in movie.tags.iter().enumerate() {
        let Some(data) = abc_payload(tag) else {
            continue;
        };
        let abc = Reader::new(data)
            .read()
            .map_err(|error| format!("parsing root ABC candidate: {error}"))?;
        let contract_class = abc.instances.iter().find(|instance| {
            [object.property, object.on_create]
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
            has_destroy_trait = contract_class.traits.iter().any(|trait_| {
                multiname_local_name(&abc, trait_.name).as_deref()
                    == Some(object.on_destroy.as_bytes())
            });
            break;
        }
    }
    let root_abc_index = root_abc_index
        .ok_or_else(|| "Fallout 4 BGSCodeObj lifecycle class was not found".to_string())?;
    let root_class =
        root_class.ok_or_else(|| "Fallout 4 lifecycle class has no qualified name".to_string())?;
    let root_abc = abc_payload(&movie.tags[root_abc_index])
        .ok_or_else(|| "Fallout 4 root ABC index does not reference an ABC tag".to_string())?;
    let patched_root_abc = patch_root_constructor(root_abc, &root_class)?;

    // #2718 (SAFEUI-04) — the catalog is a hand-transcribed inventory of a
    // third-party reconstruction of the vanilla sources, and it is the ONLY
    // thing that used to decide which properties exist on `BGSCodeObj`. A
    // method the catalog missed resolves to an absent property on a dynamic
    // object, which AVM2 reports as a call on `undefined` (Error #1006) — it
    // aborts the executing frame handler, so the menu renders and then stops
    // responding, with the true cause (a missing string in a Rust table)
    // invisible from the failure. Take the union with what this movie
    // actually calls, so an uncataloged method still gets a forwarder and
    // degrades into a recorded `ScaleformHostDispatch::Unknown` returning
    // null instead of throwing — which is also what makes the bridge's
    // `unknown_methods()` diagnostic reachable at all.
    //
    // A scan failure is deliberately non-fatal: the inventory is a bonus on
    // top of the catalog, and refusing to load a menu because its bytecode
    // has a shape the scanner cannot walk would be strictly worse than the
    // pre-fix behaviour.
    let referenced = match referenced_host_methods_in_tags(&movie.tags) {
        Ok(referenced) => referenced,
        Err(error) => {
            log::warn!(
                "Fallout 4 host-call inventory scan failed ({error}); installing the \
                 catalog's {} forwarders only",
                catalog.len()
            );
            BTreeSet::new()
        }
    };
    let uncataloged = uncataloged_host_methods(catalog, &referenced);
    if !uncataloged.is_empty() {
        log::info!(
            "Fallout 4 menu calls {} BGSCodeObj method(s) outside the {}-entry catalog; \
             forwarding them too (#2718): {:?}",
            uncataloged.len(),
            catalog.len(),
            uncataloged,
        );
    }

    let adapter = build_adapter_abc(catalog, &uncataloged, has_destroy_trait)
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

/// Every `BGSCodeObj.<method>(…)` call site a movie's bytecode contains, from
/// its raw (still compressed) bytes — the corpus sweeps' entry point.
/// Injection itself scans the movie it has already parsed, through
/// [`referenced_host_methods_in_tags`].
#[cfg(test)]
pub(crate) fn referenced_host_methods(swf_data: &[u8]) -> Result<BTreeSet<String>, String> {
    let decompressed =
        decompress_swf(swf_data).map_err(|error| format!("decompressing SWF: {error}"))?;
    let movie = parse_swf(&decompressed).map_err(|error| format!("parsing SWF: {error}"))?;
    referenced_host_methods_in_tags(&movie.tags)
}

/// [`referenced_host_methods`] over already-parsed tags, so the injection path
/// scans the movie it has in hand instead of decompressing it a second time.
fn referenced_host_methods_in_tags(tags: &[Tag<'_>]) -> Result<BTreeSet<String>, String> {
    let mut methods = BTreeSet::new();

    for tag in tags {
        let Some(data) = abc_payload(tag) else {
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

/// Members every AVM2 `Object` already answers. A forwarder installed over one
/// of these would shadow the real thing for any menu that legitimately calls
/// it on `BGSCodeObj`, so a call site naming one is never taken as evidence of
/// a host method — unlike a genuinely uncataloged name, whose only possible
/// meaning is "the game filled this in natively".
const OBJECT_PROTOTYPE_MEMBERS: &[&str] = &[
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "setPropertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
];

/// Referenced host methods the catalog does not already cover, in the order
/// their forwarders get installed. Split out (pure, `&str`-only) so the union
/// rule can be pinned without building an ABC.
fn uncataloged_host_methods(
    catalog: ScaleformHostCatalog,
    referenced: &BTreeSet<String>,
) -> Vec<String> {
    referenced
        .iter()
        .filter(|method| !catalog.contains(method))
        .filter(|method| !OBJECT_PROTOTYPE_MEMBERS.contains(&method.as_str()))
        .cloned()
        .collect()
}

#[derive(Clone)]
enum InventoryStackValue {
    BgsCodeObj,
    String(Vec<u8>),
    Other,
}

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
    let public_namespace = add_namespace(
        &mut abc.constant_pool.namespaces,
        Namespace::Package(empty_string),
    );
    let install_string = add_string(
        &mut abc.constant_pool.strings,
        INSTALL_HELPER.as_bytes().to_vec(),
    );
    let install = add_multiname(
        &mut abc.constant_pool.multinames,
        qname(public_namespace, install_string),
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

/// #2728 — the single place that knows which SWF tags carry ABC bytecode.
/// Both `DoAbc` (SWF 9) and `DoAbc2` (SWF 10+, the form Fallout 4 ships) hold
/// a raw ABC payload; every reader in this module wants that payload and
/// nothing else. Keeping one match means a future ABC-bearing tag is added
/// here, not in four places two of which used to `unreachable!()`.
fn abc_payload<'a>(tag: &Tag<'a>) -> Option<&'a [u8]> {
    match tag {
        Tag::DoAbc(data) => Some(data),
        Tag::DoAbc2(do_abc) => Some(do_abc.data),
        _ => None,
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn helper_name(index: usize) -> String {
    format!("{HELPER_PREFIX}{index}")
}

/// Names the adapter installs on the host object: the catalog, plus whatever
/// #2718's per-movie scan found that the catalog missed. One flat list because
/// each installed method's trait carries a sequential `disp_id`.
fn installed_method_names<'a>(
    catalog: ScaleformHostCatalog,
    uncataloged: &'a [String],
) -> Vec<&'a str>
where
    'static: 'a,
{
    catalog
        .methods()
        .iter()
        .map(|method| method.name)
        .chain(uncataloged.iter().map(String::as_str))
        .collect()
}

fn build_adapter_abc(
    catalog: ScaleformHostCatalog,
    uncataloged: &[String],
    has_destroy_trait: bool,
) -> std::io::Result<Vec<u8>> {
    let object = catalog
        .host_object()
        .expect("AVM2 adapter requires a host-object profile");
    // #2724 / #2725 — every constant-pool entry is *derived* from the push that
    // creates it, never transcribed as a positional `Index::new(N)` literal. The
    // pools are 1-based (ABC index 0 is the "any" sentinel), so an off-by-one or
    // a mid-pool insertion used to produce a valid-but-wrong ABC that still
    // parsed and still passed every count-based test, while forwarding calls
    // under the wrong names. Only `Index::new(0)` — the sentinel itself — is
    // written literally below.
    let mut strings = Vec::new();
    let empty_string = add_string(&mut strings, Vec::new());
    let flash_external_string = add_string(&mut strings, b"flash.external".to_vec());
    let external_interface_string = add_string(&mut strings, b"ExternalInterface".to_vec());
    let call_string = add_string(&mut strings, b"call".to_vec());
    let apply_string = add_string(&mut strings, b"apply".to_vec());
    let unshift_string = add_string(&mut strings, b"unshift".to_vec());
    let code_object_string = add_string(&mut strings, object.property.as_bytes().to_vec());
    let on_create_string = add_string(&mut strings, object.on_create.as_bytes().to_vec());
    let install_helper_string = add_string(&mut strings, INSTALL_HELPER.as_bytes().to_vec());
    let add_callback_string = add_string(&mut strings, b"addCallback".to_vec());
    let ready_helper_string = add_string(&mut strings, READY_HELPER.as_bytes().to_vec());
    let ready_callback_string = add_string(&mut strings, READY_CALLBACK.as_bytes().to_vec());
    let loaded_callback_string = add_string(&mut strings, LOADED_CALLBACK.as_bytes().to_vec());
    // #2963 — these four only feed the destroy helper below. A movie whose
    // lifecycle class never declared `onCodeObjDestruction` gets no destroy
    // machinery at all: emitting a `CallPropVoid` against a property the
    // class doesn't have would just move the old "menu fails to load"
    // failure into a runtime AVM2 error on close instead of fixing it.
    let destroy_strings = has_destroy_trait.then(|| {
        (
            add_string(&mut strings, object.on_destroy.as_bytes().to_vec()),
            add_string(&mut strings, DESTROY_HELPER.as_bytes().to_vec()),
            add_string(&mut strings, DESTROY_CALLBACK.as_bytes().to_vec()),
            add_string(&mut strings, DESTROYED_EVENT.as_bytes().to_vec()),
        )
    });
    let root_slot_string = add_string(&mut strings, b"__byro_fallout4_root".to_vec());

    // Only two namespaces survive: the public (empty-name) package every helper
    // lives in, and `flash.external` for `ExternalInterface`. The pool used to
    // also declare `flash.display` and `flash.utils` for the abandoned
    // `LoaderInfo.getLoaderInfoByDefinition` / `setTimeout` install strategy the
    // constructor patch superseded (#2725).
    let mut namespaces = Vec::new();
    let public_namespace = add_namespace(&mut namespaces, Namespace::Package(empty_string));
    let flash_external_namespace =
        add_namespace(&mut namespaces, Namespace::Package(flash_external_string));

    let mut multinames = Vec::new();
    let external_interface = add_multiname(
        &mut multinames,
        qname(flash_external_namespace, external_interface_string),
    );
    let call = add_multiname(&mut multinames, qname(public_namespace, call_string));
    let apply = add_multiname(&mut multinames, qname(public_namespace, apply_string));
    let unshift = add_multiname(&mut multinames, qname(public_namespace, unshift_string));
    let code_object = add_multiname(&mut multinames, qname(public_namespace, code_object_string));
    let on_create = add_multiname(&mut multinames, qname(public_namespace, on_create_string));
    let install_helper = add_multiname(
        &mut multinames,
        qname(public_namespace, install_helper_string),
    );
    let add_callback = add_multiname(
        &mut multinames,
        qname(public_namespace, add_callback_string),
    );
    let ready_helper = add_multiname(
        &mut multinames,
        qname(public_namespace, ready_helper_string),
    );
    // (on_destroy multiname, destroy_helper multiname, destroy_helper name
    // string, destroy_callback string, destroyed_event string) — `None` when
    // this movie's lifecycle class has no `onCodeObjDestruction` trait.
    let destroy_names = destroy_strings.map(
        |(
            on_destroy_string,
            destroy_helper_string,
            destroy_callback_string,
            destroyed_event_string,
        )| {
            let on_destroy =
                add_multiname(&mut multinames, qname(public_namespace, on_destroy_string));
            let destroy_helper = add_multiname(
                &mut multinames,
                qname(public_namespace, destroy_helper_string),
            );
            (
                on_destroy,
                destroy_helper,
                destroy_helper_string,
                destroy_callback_string,
                destroyed_event_string,
            )
        },
    );
    let root_slot = add_multiname(&mut multinames, qname(public_namespace, root_slot_string));

    let installed = installed_method_names(catalog, uncataloged);
    let mut methods = Vec::with_capacity(installed.len() + 4);
    let mut method_bodies = Vec::with_capacity(installed.len() + 4);
    let mut traits = Vec::with_capacity(installed.len() + 4);
    let mut helper_multinames = Vec::with_capacity(installed.len());
    let mut method_property_multinames = Vec::with_capacity(installed.len());
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

    for (index, method) in installed.iter().enumerate() {
        let helper_string = add_string(&mut strings, helper_name(index));
        let transport_string = add_string(&mut strings, format!("{}.{}", object.property, method));
        let method_property_string = add_string(&mut strings, *method);

        let helper_multiname =
            add_multiname(&mut multinames, qname(public_namespace, helper_string));
        let method_property = add_multiname(
            &mut multinames,
            qname(public_namespace, method_property_string),
        );
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
        name: ready_helper_string,
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
    let mut next_disp_id = installed.len() as u32 + 1;
    traits.push(method_trait(ready_helper, ready_method, next_disp_id));
    next_disp_id += 1;

    // #2963 — only wire the destroy helper (and its `addCallback`
    // registration below) when the movie's lifecycle class actually declared
    // `onCodeObjDestruction`. Omitting it entirely leaves `DESTROY_CALLBACK`
    // unregistered, which `SwfPlayer`'s close path already treats as
    // optional (`has_callback(DESTROY_CALLBACK)`, `player.rs`) rather than
    // an error.
    let mut destroy_registration = None;
    if let Some((
        on_destroy,
        destroy_helper,
        destroy_helper_string,
        destroy_callback_string,
        destroyed_event_string,
    )) = destroy_names
    {
        let destroy_method = Index::new(methods.len() as u32);
        let destroy_body = Index::new(method_bodies.len() as u32);
        methods.push(Method {
            name: destroy_helper_string,
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
        traits.push(method_trait(destroy_helper, destroy_method, next_disp_id));
        next_disp_id += 1;
        destroy_registration = Some((destroy_helper, destroy_callback_string));
    }

    let installer_method = Index::new(methods.len() as u32);
    let installer_body = Index::new(method_bodies.len() as u32);
    methods.push(Method {
        name: install_helper_string,
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
    ]);
    // #2963 — only register the destroy callback when this movie's lifecycle
    // class carries the trait the helper above was built to call.
    if let Some((destroy_helper, destroy_callback_string)) = destroy_registration {
        install_ops.extend([
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
        ]);
    }
    install_ops.push(Op::ReturnVoid);
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
    traits.push(method_trait(install_helper, installer_method, next_disp_id));

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
            namespaces,
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

fn qname(namespace: Index<Namespace>, name: Index<String>) -> Multiname {
    Multiname::QName { namespace, name }
}

fn add_string(strings: &mut Vec<Vec<u8>>, value: impl Into<Vec<u8>>) -> Index<String> {
    strings.push(value.into());
    Index::new(strings.len() as u32)
}

fn add_namespace(namespaces: &mut Vec<Namespace>, value: Namespace) -> Index<Namespace> {
    namespaces.push(value);
    Index::new(namespaces.len() as u32)
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
        referenced_host_methods, uncataloged_host_methods, write_ops, DESTROYED_EVENT,
        DESTROY_CALLBACK, LOADED_CALLBACK,
    };
    use crate::{ScaleformHostCatalog, ScaleformProfile};

    /// FO4 interface archive used by the corpus sweeps, or `None` when the
    /// game isn't installed on this machine (the corpus is multi-gigabyte
    /// game data that can't ship with the repo, so the sweeps self-skip
    /// rather than failing — see #2717).
    fn fallout4_interface_archive(test_name: &str) -> Option<Ba2Archive> {
        let path = std::env::var("BYROREDUX_FO4_DATA").unwrap_or_else(|_| {
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data".to_string()
        });
        let archive_path = std::path::Path::new(&path).join("Fallout4 - Interface.ba2");
        match Ba2Archive::open(&archive_path) {
            Ok(archive) => Some(archive),
            Err(_) => {
                eprintln!(
                    "skipping {test_name}: {archive_path:?} not available \
                     (set BYROREDUX_FO4_DATA to override)"
                );
                None
            }
        }
    }

    fn fallout4_swf_paths(archive: &Ba2Archive) -> Vec<String> {
        let mut swf_paths: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|path| path.ends_with(".swf"))
            .map(str::to_string)
            .collect();
        swf_paths.sort();
        assert!(
            !swf_paths.is_empty(),
            "expected .swf files in the FO4 interface archive"
        );
        swf_paths
    }

    #[test]
    fn generated_adapter_is_valid_abc_with_one_helper_per_method() {
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let bytes = build_adapter_abc(catalog, &[], true).unwrap();
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
        // #2724 — assert the *identity* of the pushed callback name, not the
        // raw pool position it happens to occupy. The old `== [22]` could only
        // report "expected 22, got 23" for a whole-pool shift, and said nothing
        // about the other sixteen indices.
        assert_eq!(callback_names.len(), 1);
        let pushed = &abc.constant_pool.strings[callback_names[0] as usize - 1];
        assert_eq!(pushed.as_slice(), LOADED_CALLBACK.as_bytes());
    }

    /// #2963 — DialogueMenu / MultiActivateMenu / SPECIALMenu / Terminal all
    /// declare `BGSCodeObj` + `onCodeObjCreate` but never their own
    /// `onCodeObjDestruction` trait. `inject_host_object_adapter` now injects
    /// for these movies anyway (`has_destroy_trait: false`); this pins that
    /// `build_adapter_abc` responds by dropping the whole destroy helper —
    /// one fewer method/trait than the `true` case above, no reference to
    /// `DESTROYED_EVENT`/`DESTROY_CALLBACK` anywhere in the pool, and no
    /// `addCallback` registration for it in the installer — rather than
    /// emitting a `CallPropVoid` against a property this movie's class
    /// doesn't have.
    #[test]
    fn generated_adapter_omits_destroy_helper_when_class_lacks_the_trait() {
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let bytes = build_adapter_abc(catalog, &[], false).unwrap();
        let abc = Reader::new(&bytes).read().unwrap();

        // One fewer method/body/trait than the has-destroy-trait case: no
        // destroy helper.
        assert_eq!(abc.scripts[0].traits.len(), catalog.len() + 3);
        assert_eq!(abc.methods.len(), catalog.len() + 3);
        assert_eq!(abc.method_bodies.len(), catalog.len() + 3);

        for absent in [DESTROYED_EVENT, DESTROY_CALLBACK] {
            assert!(
                !abc.constant_pool
                    .strings
                    .iter()
                    .any(|string| string.as_slice() == absent.as_bytes()),
                "adapter pool still carries {absent:?} with no destroy trait to call"
            );
        }

        // The installer (now at `catalog.len() + 1`, since the destroy
        // helper that used to sit before it is gone) registers exactly one
        // callback — `READY_CALLBACK` — not two.
        let installer = &abc.method_bodies[catalog.len() + 1];
        let mut reader = Reader::new(&installer.code);
        let mut add_callback_calls = 0;
        while !reader.as_slice().is_empty() {
            if matches!(
                reader.read_op().unwrap(),
                swf::avm2::types::Op::CallPropVoid { .. }
            ) {
                add_callback_calls += 1;
            }
        }
        // on_create + addCallback(ready) — no addCallback(destroy).
        assert_eq!(add_callback_calls, 2);
    }

    /// Regression for #2725: the adapter's constant pool once carried the
    /// vocabulary of a superseded install strategy — a `LoaderInfo` /
    /// `getLoaderInfoByDefinition` / `addEventListener` / `complete` root
    /// lookup and a `flash.utils::setTimeout` deferral — that no emitted op
    /// ever referenced, shipped inside every FO4 menu the engine patches. The
    /// adapter reaches the root through the patched lifecycle constructor
    /// instead, so none of it is reachable. Exact pool sizes are pinned too:
    /// the dead entries accumulated silently precisely because nothing counted.
    #[test]
    fn generated_adapter_pool_carries_no_abandoned_loader_strategy() {
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let bytes = build_adapter_abc(catalog, &[], true).unwrap();
        let abc = Reader::new(&bytes).read().unwrap();

        for abandoned in [
            "flash.display",
            "flash.utils",
            "LoaderInfo",
            "getLoaderInfoByDefinition",
            "addEventListener",
            "setTimeout",
            "target",
            "content",
            "complete",
        ] {
            assert!(
                !abc.constant_pool
                    .strings
                    .iter()
                    .any(|string| string.as_slice() == abandoned.as_bytes()),
                "adapter constant pool still ships the abandoned {abandoned:?} entry"
            );
        }

        // 18 fixed strings + 3 per catalog method (helper name, `BGSCodeObj.X`
        // transport name, bare method name); 12 fixed multinames + 2 per method
        // (helper, installed property); public + `flash.external` namespaces.
        assert_eq!(abc.constant_pool.strings.len(), 18 + catalog.len() * 3);
        assert_eq!(abc.constant_pool.multinames.len(), 12 + catalog.len() * 2);
        assert_eq!(abc.constant_pool.namespaces.len(), 2);
    }

    /// #2718 (SAFEUI-04) — the corpus sweep the pre-fix version of this test
    /// could not be: it inspected **three** movies out of 311 and was
    /// `#[ignore]`d, so 308 shipped menus were unverified against the catalog
    /// and nothing ran in a normal `cargo test`. It now walks every `.swf` in
    /// the interface archive, and self-skips (like its `all_installed_…`
    /// sibling) instead of being permanently opted out.
    ///
    /// The assertion also changed shape with the fix. `referenced ⊆ catalog`
    /// is no longer the safety property — an uncataloged method is no longer
    /// fatal, because injection installs a forwarder for the *union* of the
    /// catalog and this movie's own call sites. What must hold now is that
    /// the scan those forwarders come from actually succeeds on every real
    /// menu: a scan failure silently degrades injection back to catalog-only
    /// and re-arms the Error #1006 the fix removes. The catalog gap itself is
    /// reported rather than asserted — it is a to-do list for the catalog's
    /// `kind`/response metadata, not a load-blocker.
    #[test]
    fn installed_fallout4_host_calls_are_all_forwarded() {
        let Some(archive) =
            fallout4_interface_archive("installed_fallout4_host_calls_are_all_forwarded")
        else {
            return;
        };
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let swf_paths = fallout4_swf_paths(&archive);

        let mut referenced = std::collections::BTreeSet::new();
        let mut uncataloged = std::collections::BTreeSet::new();
        let mut failures = Vec::new();
        let mut scanned = 0usize;
        for movie in &swf_paths {
            let swf = match archive.extract(movie) {
                Ok(swf) => swf,
                Err(error) => {
                    failures.push(format!("{movie}: extract failed: {error}"));
                    continue;
                }
            };
            match referenced_host_methods(&swf) {
                Ok(methods) => {
                    scanned += 1;
                    uncataloged.extend(uncataloged_host_methods(catalog, &methods));
                    referenced.extend(methods);
                }
                Err(error) => failures.push(format!("{movie}: host-call scan failed: {error}")),
            }
        }

        assert!(
            failures.is_empty(),
            "{} of {} FO4 SWFs could not be scanned for BGSCodeObj call sites — injection \
             falls back to catalog-only forwarders for these, which is exactly the \
             Error #1006 exposure #2718 removes:\n{}",
            failures.len(),
            swf_paths.len(),
            failures.join("\n"),
        );

        if !uncataloged.is_empty() {
            eprintln!(
                "note: {} BGSCodeObj method(s) called by the {scanned}-movie corpus are \
                 outside the {}-entry catalog. These are forwarded (and land as \
                 `ScaleformHostDispatch::Unknown`) since #2718, but carry no catalog \
                 `kind`: {uncataloged:?}",
                uncataloged.len(),
                catalog.len(),
            );
        }

        // #2727 — the reverse direction. `referenced ⊆ catalog` alone can never
        // fail on a *bogus* catalog entry, which is how the mangled
        // `functiononGPSModeButtonClicked` (#2726) survived into a checked-in
        // 138-method catalog. Still a warning list rather than an assertion:
        // menus outside this archive (and dynamically-dispatched call sites the
        // scanner cannot see) legitimately leave entries unreferenced.
        let unreferenced = catalog
            .methods()
            .iter()
            .map(|method| method.name)
            .filter(|name| !referenced.contains(*name))
            .collect::<Vec<_>>();
        if !unreferenced.is_empty() {
            eprintln!(
                "note: {} of {} catalog entries are unreferenced by the whole {scanned}-movie \
                 corpus (scan for malformed names): {unreferenced:?}",
                unreferenced.len(),
                catalog.len()
            );
        }
    }

    /// #2718 — the union rule itself. A method the movie calls but the catalog
    /// lacks must be installed; one the catalog already has must not be
    /// installed twice (each forwarder's trait carries a sequential `disp_id`);
    /// and a plain `Object` member must never be shadowed by a forwarder that
    /// would send `toString()` to the engine.
    #[test]
    fn uncataloged_methods_are_the_union_minus_catalog_and_object_members() {
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let referenced = ["PlaySound", "SomeUnknownFO4Method", "toString", "valueOf"]
            .into_iter()
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(catalog.contains("PlaySound"), "test premise");

        assert_eq!(
            uncataloged_host_methods(catalog, &referenced),
            vec!["SomeUnknownFO4Method".to_string()]
        );
    }

    /// #2718 — and the union really reaches the emitted bytecode: an extra
    /// method adds one more forwarder, one more installed property, and its
    /// name to the transport strings, without disturbing the fixed tail
    /// (ready / destroy / installer / script init).
    #[test]
    fn an_uncataloged_method_gets_its_own_installed_forwarder() {
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let extra = vec!["SomeUnknownFO4Method".to_string()];
        let bytes = build_adapter_abc(catalog, &extra, true).unwrap();
        let abc = Reader::new(&bytes).read().unwrap();
        let installed = catalog.len() + extra.len();

        assert_eq!(abc.methods.len(), installed + 4);
        assert_eq!(abc.method_bodies.len(), installed + 4);
        assert_eq!(abc.scripts[0].traits.len(), installed + 4);

        let installer = &abc.method_bodies[installed + 2];
        let mut reader = Reader::new(&installer.code);
        let mut installed_properties = 0;
        while !reader.as_slice().is_empty() {
            if matches!(
                reader.read_op().unwrap(),
                swf::avm2::types::Op::SetProperty { .. }
            ) {
                installed_properties += 1;
            }
        }
        // One per installed method, plus the root-slot store.
        assert_eq!(installed_properties, installed + 1);

        for expected in ["SomeUnknownFO4Method", "BGSCodeObj.SomeUnknownFO4Method"] {
            assert!(
                abc.constant_pool
                    .strings
                    .iter()
                    .any(|string| string.as_slice() == expected.as_bytes()),
                "adapter pool is missing {expected:?}"
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
    /// #2963 — `DialogueMenu`, `MultiActivateMenu`, `SPECIALMenu` and
    /// `Terminal` used to be excluded here (`KNOWN_MISSING_ON_DESTROY_TRAIT`):
    /// their lifecycle class declares `BGSCodeObj` + `onCodeObjCreate` but no
    /// `onCodeObjDestruction` trait of its own — legal, real, shipped ABC —
    /// and the old three-trait class predicate made that a hard `Err` that
    /// every caller (`SwfPlayer::new` / `new_with_profile` /
    /// `from_resource_provider`) propagated, so these 4 real Fallout 4 menus
    /// failed to load entirely. `inject_host_object_adapter` now only
    /// requires `BGSCodeObj` + `onCodeObjCreate` to find the lifecycle class,
    /// and skips wiring the destroy helper when the class lacks the trait —
    /// so all four now inject with no exclusion needed. If this assertion
    /// starts failing on a real corpus, that is a new, real regression, not
    /// a resurfaced on-destroy gap.
    #[test]
    fn all_installed_fallout4_swfs_round_trip_through_injection() {
        let Some(archive) =
            fallout4_interface_archive("all_installed_fallout4_swfs_round_trip_through_injection")
        else {
            return;
        };
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let swf_paths = fallout4_swf_paths(&archive);

        let mut injected_count = 0usize;
        let mut failures = Vec::new();
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
                    failures.push(format!("{movie_path}: injection failed: {error}"));
                }
            }
        }

        assert!(
            failures.is_empty(),
            "{} of {} FO4 SWFs failed the injection round-trip ({injected_count} actually \
             went through AVM2 adapter injection):\n{}",
            failures.len(),
            swf_paths.len(),
            failures.join("\n"),
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
