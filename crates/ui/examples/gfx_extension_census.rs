//! M48 Phase 0 (#6643, GUI-GFX-003 / PRD §7.1) — per-game census of the Scaleform extension
//! members a vanilla interface archive references, plus the host-call inventory used for the
//! Skyrim LE vs SE diff and the Starfield `BGSCodeObj` root-object question.
//!
//! Read-only measurement tool. It does its own SWF tag walk (so it also reads Starfield movies,
//! which the stock `swf` reader rejects on `PlaceObject3`, #4470 — no PlaceObject is parsed here)
//! and prints raw tab-separated lines on stdout; the aggregation lives outside the repo.
//!
//! ```bash
//! cargo run --release -p byroredux-ui --example gfx_extension_census -- as2 "<dir>/Skyrim - Interface.bsa"
//! cargo run --release -p byroredux-ui --example gfx_extension_census -- as3 "<dir>/Fallout4 - Interface.ba2"
//! cargo run --release -p byroredux-ui --example gfx_extension_census -- strings <archive> <movie-path>
//! ```
//!
//! Line kinds: `M movie bytes status` · `H movie method min max` (host call) · `X movie
//! Class.member kind how` (extension reference) · `R movie receiver.member` (AS2 discovery of every
//! member read off Selection/Stage/Mouse/Key/_global/TextField) · `E movie flag` (AS3 root-object
//! evidence). The AVM2 host-call walk mirrors `avm2_host::method_called_with_bgs_receiver` (that
//! function is `pub(crate)`/`cfg(test)`, so it cannot be reused from an example).

use std::collections::BTreeSet;

use byroredux_bsa::{Ba2Archive, BsaArchive};
use swf::avm1::read::Reader as Avm1Reader;
use swf::avm1::types::{Action, Value};
use swf::avm2::read::Reader as Avm2Reader;
use swf::avm2::types::{AbcFile, Index, Multiname, Namespace, Op};
use swf::extensions::ReadSwfExt;

// ---- candidate lists: Autodesk Scaleform 4.x "AS2/AS3 Extensions Reference" ------------------
// Pages: https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/as2_reference/*.html
//        https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/as3_reference/*.html

/// (class, member, kind). `static` receivers are confirmed by receiver name; instance members are
/// matched by member name only (AS2 has no static types in bytecode).
const AS2_STATIC: &[(&str, &str, &str)] = &[
    ("Selection", "alwaysEnableArrowKeys", "AS2-prop"),
    ("Selection", "alwaysEnableKeyboardPress", "AS2-prop"),
    ("Selection", "captureFocus", "AS2-method"),
    ("Selection", "disableFocusAutoRelease", "AS2-prop"),
    ("Selection", "disableFocusKeys", "AS2-prop"),
    ("Selection", "disableFocusRolloverEvent", "AS2-prop"),
    ("Selection", "modalClip", "AS2-prop"),
    ("Selection", "moveFocus", "AS2-method"),
    ("Selection", "findFocus", "AS2-method"),
    ("Selection", "setModalClip", "AS2-method"),
    ("Selection", "getModalClip", "AS2-method"),
    ("Selection", "setControllerFocusGroup", "AS2-method"),
    ("Selection", "getControllerFocusGroup", "AS2-method"),
    ("Selection", "getFocusArray", "AS2-method"),
    ("Selection", "getFocusBitmask", "AS2-method"),
    ("Selection", "getControllerMaskByFocusGroup", "AS2-method"),
    ("Selection", "numFocusGroups", "AS2-prop"),
    ("Stage", "visibleRect", "AS2-prop"),
    ("Stage", "safeRect", "AS2-prop"),
    ("Stage", "originalRect", "AS2-prop"),
    ("Stage", "translateToScreen", "AS2-method"),
    ("Mouse", "getButtonsState", "AS2-method"),
    ("Mouse", "getTopMostEntity", "AS2-method"),
    ("Mouse", "getPosition", "AS2-method"),
    ("Mouse", "setCursorType", "AS2-method"),
    ("_global", "gfxExtensions", "AS2-prop"),
    ("_global", "noInvisibleAdvance", "AS2-prop"),
    ("_global", "imecommand", "AS2-method"),
    ("_global", "getIMECandidateListStyle", "AS2-method"),
    ("_global", "setIMECandidateListStyle", "AS2-method"),
];

/// Instance members: class list is documentation only; match is by member name.
const AS2_INSTANCE: &[(&str, &str, &str)] = &[
    ("MovieClip|Button|TextField", "hitTestDisable", "AS2-prop"),
    ("MovieClip|Button|TextField", "topmostLevel", "AS2-prop"),
    ("MovieClip|Button|TextField", "focusGroupMask", "AS2-prop"),
    ("MovieClip", "noAdvance", "AS2-prop"),
    ("MovieClip", "rendererString", "AS2-prop"),
    ("MovieClip", "rendererFloat", "AS2-prop"),
    ("MovieClip", "disableBatching", "AS2-prop"),
    ("TextField", "autoFit", "AS2-prop"),
    ("TextField", "appendHtml", "AS2-method"),
    ("TextField", "caretIndex", "AS2-prop"),
    ("TextField", "getCharBoundaries", "AS2-method"),
    ("TextField", "getExactCharBoundaries", "AS2-method"),
    ("TextField", "getCharIndexAtPoint", "AS2-method"),
    ("TextField", "getFirstCharInParagraph", "AS2-method"),
    ("TextField", "getLineIndexAtPoint", "AS2-method"),
    ("TextField", "getLineLength", "AS2-method"),
    ("TextField", "getLineMetrics", "AS2-method"),
    ("TextField", "getLineOffset", "AS2-method"),
    ("TextField", "getLineText", "AS2-method"),
    ("TextField", "noTranslate", "AS2-prop"),
    ("TextField", "numLines", "AS2-prop"),
    ("TextField", "setImageSubstitutions", "AS2-method"),
    ("TextField", "updateImageSubstitution", "AS2-method"),
    ("TextField", "disableIME", "AS2-prop"),
    ("TextField", "getIMECompositionStringStyle", "AS2-method"),
    ("TextField", "setIMECompositionStringStyle", "AS2-method"),
    ("TextField", "alwaysShowSelection", "AS2-prop"),
    ("TextField", "copyToClipboard", "AS2-method"),
    ("TextField", "cutToClipboard", "AS2-method"),
    ("TextField", "pasteFromClipboard", "AS2-method"),
    ("TextField", "inactiveSelectionBkgColor", "AS2-prop"),
    ("TextField", "inactiveSelectionTextColor", "AS2-prop"),
    ("TextField", "noAutoSelection", "AS2-prop"),
    ("TextField", "selectionBeginIndex", "AS2-prop"),
    ("TextField", "selectionEndIndex", "AS2-prop"),
    ("TextField", "selectionBkgColor", "AS2-prop"),
    ("TextField", "selectionTextColor", "AS2-prop"),
    ("TextField", "useRichTextClipboard", "AS2-prop"),
    ("TextField", "fontScaleFactor", "AS2-prop"),
    ("TextField", "textAutoSize", "AS2-prop"),
    ("TextField", "verticalAlign", "AS2-prop"),
    ("TextField", "verticalAutoSize", "AS2-prop"),
    ("NetStream", "audioTrack", "AS2-prop"),
    ("NetStream", "subtitleTrack", "AS2-prop"),
    ("NetStream", "setNumberOfFramePools", "AS2-method"),
    ("NetStream", "setReloadThresholdTime", "AS2-method"),
    ("String", "localeCompare", "AS2-method"),
    ("Video", "clipRect", "AS2-prop"),
];

/// AS3 `scaleform.gfx.*` classes and members (static unless noted).
const AS3_CLASSES: &[(&str, &[&str])] = &[
    ("Extensions", &["enabled", "noInvisibleAdvance", "getTopMostEntity", "getMouseTopMostEntity",
        "getMouseCursorType", "setMouseCursorType", "numControllers", "setEdgeAAMode", "getEdgeAAMode",
        "visibleRect", "isScaleform", "disableBlockingBitmapDataAPI"]),
    ("TextFieldEx", &["appendHtml", "setIMEEnabled", "setVerticalAlign", "getVerticalAlign",
        "setImageSubstitutions", "updateImageSubstitution", "setTextAutoSize", "getTextAutoSize",
        "setNoTranslate", "getNoTranslate", "setBidirectionalTextEnabled", "getBidirectionalTextEnabled",
        "setSelectionTextColor", "getSelectionTextColor", "setSelectionBkgColor", "getSelectionBkgColor",
        "setInactiveSelectionTextColor", "getInactiveSelectionTextColor", "setInactiveSelectionBkgColor",
        "setForceVector", "getForceVector", "VALIGN_NONE", "VALIGN_TOP", "VALIGN_CENTER", "VALIGN_BOTTOM",
        "TEXTAUTOSZ_NONE", "TEXTAUTOSZ_SHRINK", "TEXTAUTOSZ_FIT"]),
    ("DisplayObjectEx", &["setRendererString", "getRendererString", "setRendererFloat",
        "getRendererFloat", "disableBatching", "isBatchingDisabled", "setInvertedMask", "getInvertedMask"]),
    ("InteractiveObjectEx", &["getHitTestDisable", "setHitTestDisable", "getTopmostLevel", "setTopmostLevel"]),
    ("FocusManager", &["alwaysEnableArrowKeys", "disableFocusKeys", "moveFocus", "findFocus", "setFocus",
        "getFocus", "numFocusGroups", "setFocusGroupMask", "getFocusGroupMask", "setControllerFocusGroup",
        "getControllerFocusGroup", "setModalClip", "getModalClip", "getControllerMaskByFocusGroup"]),
    ("GamePad", &["supportsAnalogEvents"]),
    ("System", &["actionVerbose", "getStackTrace", "getCodeFileName"]),
    ("FocusEventEx", &[]),
    ("MouseEventEx", &[]),
    ("KeyboardEventEx", &[]),
    ("MouseCursorEvent", &[]),
    ("TextEventEx", &[]),
    ("GamePadAnalogEvent", &[]),
    ("IMEEventEx", &[]),
    ("SWFLoaderInfo", &[]),
];

const AS2_DISCOVERY_RECEIVERS: &[&str] = &["Selection", "Stage", "Mouse", "Key", "_global", "TextField"];

// ---- SWF container -------------------------------------------------------------------------

fn tag_list(mut data: &[u8]) -> Vec<(u16, &[u8])> {
    let mut out = Vec::new();
    while data.len() >= 2 {
        let head = u16::from_le_bytes([data[0], data[1]]);
        let code = head >> 6;
        let mut len = (head & 0x3f) as usize;
        let mut off = 2;
        if len == 0x3f {
            if data.len() < 6 {
                break;
            }
            len = u32::from_le_bytes([data[2], data[3], data[4], data[5]]) as usize;
            off = 6;
        }
        if code == 0 || data.len() < off + len {
            break;
        }
        out.push((code, &data[off..off + len]));
        data = &data[off + len..];
    }
    out
}

struct Blocks<'a> {
    actions: Vec<&'a [u8]>,
    abcs: Vec<&'a [u8]>,
}

fn collect<'a>(tags: Vec<(u16, &'a [u8])>, b: &mut Blocks<'a>, depth: usize) {
    for (code, payload) in tags {
        match code {
            12 => b.actions.push(payload),
            59 if payload.len() >= 2 => b.actions.push(&payload[2..]),
            39 if payload.len() >= 4 && depth < 16 => collect(tag_list(&payload[4..]), b, depth + 1),
            72 => b.abcs.push(payload),
            82 if payload.len() >= 5 => {
                // flags u32, NUL-terminated name, abc
                if let Some(nul) = payload[4..].iter().position(|&c| c == 0) {
                    b.abcs.push(&payload[4 + nul + 1..]);
                }
            }
            _ => {}
        }
    }
}

// ---- AS2 -----------------------------------------------------------------------------------

#[derive(Clone)]
enum V {
    Str(String),
    Obj(String),
    Num(f64),
    Op,
}

fn obj_path(v: Option<V>) -> Option<String> {
    match v {
        Some(V::Obj(p)) | Some(V::Str(p)) => Some(p),
        _ => None,
    }
}

/// (receiver path or "", member, how, arg_count)
type Ref = (String, String, &'static str, usize);

fn scan_as2(code: &[u8], version: u8, pool: &[String], out: &mut Vec<Ref>) {
    let mut pool = pool.to_vec();
    let mut st: Vec<V> = Vec::new();
    let mut regs: Vec<V> = vec![V::Op; 256];
    let mut rd = Avm1Reader::new(code, version);
    while let Ok(a) = rd.read_action() {
        match a {
            Action::End => break,
            Action::ConstantPool(c) => {
                pool = c.strings.iter().map(|s| s.to_string_lossy(swf::UTF_8)).collect()
            }
            Action::Push(p) => {
                for v in &p.values {
                    st.push(match v {
                        Value::Str(s) => V::Str(s.to_string_lossy(swf::UTF_8)),
                        Value::ConstantPool(i) => {
                            pool.get(*i as usize).map(|s| V::Str(s.clone())).unwrap_or(V::Op)
                        }
                        Value::Int(i) => V::Num(f64::from(*i)),
                        Value::Float(f) => V::Num(f64::from(*f)),
                        Value::Double(f) => V::Num(*f),
                        Value::Register(i) => regs.get(*i as usize).cloned().unwrap_or(V::Op),
                        _ => V::Op,
                    });
                }
            }
            Action::GetVariable => {
                let name = st.pop();
                match name {
                    Some(V::Str(n)) => {
                        out.push((String::new(), n.clone(), "getvar", 0));
                        st.push(V::Obj(n));
                    }
                    _ => st.push(V::Op),
                }
            }
            Action::SetVariable => {
                st.pop();
                if let Some(V::Str(n)) = st.pop() {
                    out.push((String::new(), n, "setvar", 0));
                }
            }
            Action::GetMember => {
                let m = st.pop();
                let o = st.pop();
                match m {
                    Some(V::Str(m)) => {
                        let p = obj_path(o).unwrap_or_else(|| "?".into());
                        out.push((p.clone(), m.clone(), "get", 0));
                        st.push(V::Obj(format!("{p}.{m}")));
                    }
                    _ => st.push(V::Op),
                }
            }
            Action::SetMember => {
                st.pop();
                let m = st.pop();
                let o = st.pop();
                if let Some(V::Str(m)) = m {
                    out.push((obj_path(o).unwrap_or_else(|| "?".into()), m, "set", 0));
                }
            }
            Action::CallMethod => {
                let m = st.pop();
                let r = st.pop();
                let c = st.pop();
                let Some(V::Num(n)) = c else {
                    st.clear();
                    continue;
                };
                let n = n as usize;
                if n > st.len() {
                    st.clear();
                    continue;
                }
                st.truncate(st.len() - n);
                if let Some(V::Str(m)) = m {
                    out.push((obj_path(r).unwrap_or_else(|| "?".into()), m, "call", n));
                }
                st.push(V::Op);
            }
            Action::CallFunction => {
                let name = st.pop();
                let c = st.pop();
                let Some(V::Num(n)) = c else {
                    st.clear();
                    continue;
                };
                let n = n as usize;
                if n > st.len() {
                    st.clear();
                    continue;
                }
                st.truncate(st.len() - n);
                if let Some(V::Str(name)) = name {
                    out.push((String::new(), name, "callfn", n));
                }
                st.push(V::Op);
            }
            Action::StoreRegister(s) => {
                if let Some(slot) = regs.get_mut(s.register as usize) {
                    *slot = st.last().cloned().unwrap_or(V::Op);
                }
            }
            Action::PushDuplicate => st.push(st.last().cloned().unwrap_or(V::Op)),
            Action::StackSwap => {
                let l = st.len();
                if l >= 2 {
                    st.swap(l - 1, l - 2)
                } else {
                    st.clear()
                }
            }
            Action::Pop => {
                st.pop();
            }
            Action::InitArray => {
                let Some(V::Num(n)) = st.pop() else {
                    st.clear();
                    continue;
                };
                let n = n as usize;
                if n > st.len() {
                    st.clear();
                    continue;
                }
                st.truncate(st.len() - n);
                st.push(V::Op);
            }
            Action::ToNumber | Action::ToString | Action::ToInteger | Action::Increment
            | Action::Decrement | Action::TypeOf | Action::Not | Action::StringLength => {
                st.pop();
                st.push(V::Op);
            }
            Action::Add | Action::Add2 | Action::Subtract | Action::Multiply | Action::Divide
            | Action::Modulo | Action::Equals | Action::Equals2 | Action::StrictEquals
            | Action::Less | Action::Less2 | Action::Greater | Action::And | Action::Or
            | Action::StringAdd | Action::StringEquals | Action::BitAnd | Action::BitOr
            | Action::BitXor | Action::BitLShift | Action::BitRShift | Action::BitURShift
            | Action::InstanceOf => {
                st.pop();
                st.pop();
                st.push(V::Op);
            }
            Action::If(_) => {
                st.pop();
            }
            Action::Jump(_) => {}
            Action::DefineFunction(f) => {
                scan_as2(f.actions, version, &pool, out);
                // A named DefineFunction pushes nothing; an anonymous one pushes the function.
                if f.name.is_empty() {
                    st.push(V::Op);
                }
            }
            Action::DefineFunction2(f) => {
                scan_as2(f.actions, version, &pool, out);
                if f.name.is_empty() {
                    st.push(V::Op);
                }
            }
            _ => st.clear(),
        }
    }
}

fn run_as2(movie: &str, data: &[u8]) -> Result<(), String> {
    swf_ceiling(data)?;
    let buf = swf::decompress_swf(data).map_err(|e| format!("decompress: {e}"))?;
    let version = buf.header.version();
    let mut b = Blocks { actions: vec![], abcs: vec![] };
    collect(tag_list(&buf.data), &mut b, 0);
    let mut refs = Vec::new();
    for code in &b.actions {
        scan_as2(code, version, &[], &mut refs);
    }
    // cross-check: raw string presence (upper bound), constant pool + push literals, via the same scan
    let mut seen = BTreeSet::new();
    for (recv, member, how, n) in &refs {
        let recv_last = recv.rsplit('.').next().unwrap_or("");
        if AS2_DISCOVERY_RECEIVERS.contains(&recv_last) {
            seen.insert(format!("R\t{movie}\t{recv_last}.{member}\t{how}:{n}"));
        }
        for (class, name, kind) in AS2_STATIC {
            if member == name && (recv_last == *class || (*class == "_global" && recv.is_empty())) {
                seen.insert(format!("X\t{movie}\t{class}.{name}\t{kind}\tstatic-recv:{how}"));
            }
        }
        for (class, name, kind) in AS2_INSTANCE {
            if member == name {
                let tag = if *name == "hitTest" { "n" } else { "" };
                let _ = (tag, n);
                seen.insert(format!("X\t{movie}\t{class}.{name}\t{kind}\tmember-name-only:{how}@{}", if recv.is_empty() { "-" } else { recv.as_str() }));
            }
        }
    }
    // "Standard Methods" page: optional trailing controllerIdx argument (arity-gated extension).
    const AS2_ARITY: &[(&str, &str, usize)] = &[
        ("Selection", "setFocus", 2),
        ("Selection", "getFocus", 1),
        ("Selection", "getBeginIndex", 1),
        ("Selection", "getEndIndex", 1),
        ("Selection", "setSelection", 3),
        ("Selection", "getCaretIndex", 1),
        ("Key", "getCode", 1),
        ("Key", "getAscii", 1),
        ("Key", "isDown", 1),
        ("Key", "isToggled", 1),
    ];
    for (recv, member, how, n) in &refs {
        let recv_last = recv.rsplit('.').next().unwrap_or("");
        if *how == "call" {
            for (class, name, arity) in AS2_ARITY {
                if recv_last == *class && member == name && n == arity {
                    seen.insert(format!(
                        "X\t{movie}\t{class}.{name}(controllerIdx)\tAS2-method\tarity-{arity}:call"
                    ));
                }
            }
        }
        if member == "numControllers" && recv_last == "capabilities" {
            seen.insert(format!("X\t{movie}\tSystem.capabilities.numControllers\tAS2-prop\tstatic-recv:{how}"));
        }
    }
    // hitTest with 4 arguments is the only hitTest extension form.
    for (_r, member, how, n) in &refs {
        if member == "hitTest" && *how == "call" && *n == 4 {
            seen.insert(format!("X\t{movie}\tMovieClip.hitTest(4-arg)\tAS2-method\targ-count:{how}"));
        }
    }
    // gfxExtensions can also be read as a bare variable.
    for (r, member, how, _) in &refs {
        if member == "gfxExtensions" && r.is_empty() {
            seen.insert(format!("X\t{movie}\t_global.gfxExtensions\tAS2-prop\tbare-var:{how}"));
        }
    }
    for l in seen {
        println!("{l}");
    }
    Ok(())
}

// ---- AS3 -----------------------------------------------------------------------------------

fn pool_str(abc: &AbcFile, i: Index<String>) -> Option<String> {
    let s = abc.constant_pool.strings.get(i.as_u30().checked_sub(1)? as usize)?;
    Some(String::from_utf8_lossy(s).into_owned())
}

fn ns_name(abc: &AbcFile, i: Index<Namespace>) -> Option<String> {
    let ns = abc.constant_pool.namespaces.get(i.as_u30().checked_sub(1)? as usize)?;
    let n = match ns {
        Namespace::Namespace(n)
        | Namespace::Package(n)
        | Namespace::PackageInternal(n)
        | Namespace::Protected(n)
        | Namespace::Explicit(n)
        | Namespace::StaticProtected(n)
        | Namespace::Private(n) => *n,
    };
    pool_str(abc, n)
}

/// (namespace names the multiname can resolve in, local name). A QName has one namespace; a
/// Multiname (what Flash CS emits for `import scaleform.gfx.X`) has a set.
fn mn(abc: &AbcFile, idx: Index<Multiname>) -> Option<(Vec<String>, String)> {
    let m = abc.constant_pool.multinames.get(idx.as_u30().checked_sub(1)? as usize)?;
    match m {
        Multiname::QName { namespace, name } | Multiname::QNameA { namespace, name } => {
            Some((vec![ns_name(abc, *namespace)?], pool_str(abc, *name)?))
        }
        Multiname::Multiname { namespace_set, name } | Multiname::MultinameA { namespace_set, name } => {
            let set = abc.constant_pool.namespace_sets.get(namespace_set.as_u30().checked_sub(1)? as usize)?;
            Some((set.iter().filter_map(|n| ns_name(abc, *n)).collect(), pool_str(abc, *name)?))
        }
        Multiname::RTQName { name } | Multiname::RTQNameA { name } => Some((vec![], pool_str(abc, *name)?)),
        _ => None,
    }
}

/// `scaleform.gfx` class a multiname refers to, if any.
fn gfx_class(abc: &AbcFile, idx: Index<Multiname>) -> Option<String> {
    let (nss, name) = mn(abc, idx)?;
    if nss.iter().any(|n| n == "scaleform.gfx") && AS3_CLASSES.iter().any(|(c, _)| *c == name) {
        Some(name)
    } else {
        None
    }
}

fn local(abc: &AbcFile, idx: Index<Multiname>) -> Option<String> {
    mn(abc, idx).map(|(_, n)| n)
}

#[derive(Clone)]
enum S {
    Code,
    Str(Vec<u8>),
    Other,
}

/// Mirror of `avm2_host::method_called_with_bgs_receiver`.
fn bgs_method(abc: &AbcFile, ops: &[Op]) -> Option<String> {
    for mut stack in [vec![S::Code], vec![S::Other, S::Code]] {
        for op in ops.iter().take(32) {
            match op {
                Op::PushString { value } => {
                    let v = abc.constant_pool.strings.get(value.as_u30().checked_sub(1)? as usize)?.clone();
                    stack.push(S::Str(v));
                }
                Op::GetLocal { .. } | Op::GetLex { .. } | Op::FindPropStrict { .. } | Op::GetGlobalScope
                | Op::GetScopeObject { .. } | Op::PushByte { .. } | Op::PushShort { .. } | Op::PushInt { .. }
                | Op::PushUint { .. } | Op::PushDouble { .. } | Op::PushTrue | Op::PushFalse | Op::PushNull
                | Op::PushUndefined | Op::PushNaN => stack.push(S::Other),
                Op::GetProperty { .. } | Op::GetSlot { .. } => {
                    stack.pop()?;
                    stack.push(S::Other);
                }
                Op::SetLocal { .. } | Op::Pop => {
                    stack.pop()?;
                }
                Op::Dup => stack.push(stack.last()?.clone()),
                Op::Swap => {
                    let l = stack.len();
                    if l < 2 {
                        break;
                    }
                    stack.swap(l - 1, l - 2);
                }
                Op::CallProperty { index, num_args } | Op::CallPropVoid { index, num_args } => {
                    let n = *num_args as usize;
                    if stack.len() < n + 1 {
                        break;
                    }
                    let args = stack.split_off(stack.len() - n);
                    let recv = stack.pop()?;
                    let call = local(abc, *index)?;
                    if matches!(recv, S::Code) && call != "call" {
                        return Some(call);
                    }
                    if call == "call" && matches!(args.first(), Some(S::Code)) {
                        if let Some(S::Str(m)) = args.get(1) {
                            return Some(String::from_utf8_lossy(m).into_owned());
                        }
                    }
                    if matches!(op, Op::CallProperty { .. }) {
                        stack.push(S::Other);
                    }
                }
                Op::Coerce { .. } | Op::CoerceA | Op::CoerceB | Op::CoerceD | Op::CoerceI | Op::CoerceO
                | Op::CoerceS | Op::CoerceU | Op::ConvertB | Op::ConvertD | Op::ConvertI | Op::ConvertO
                | Op::ConvertS | Op::ConvertU | Op::AsType { .. } => {}
                _ => break,
            }
        }
    }
    None
}

fn run_as3(movie: &str, data: &[u8], strings_dump: bool) -> Result<(), String> {
    swf_ceiling(data)?;
    let buf = swf::decompress_swf(data).map_err(|e| format!("decompress: {e}"))?;
    let mut b = Blocks { actions: vec![], abcs: vec![] };
    collect(tag_list(&buf.data), &mut b, 0);
    let mut lines = BTreeSet::new();
    let mut flag = |f: &str| {
        lines.insert(format!("E\t{movie}\t{f}"));
    };
    if !b.actions.is_empty() {
        flag("has-avm1-actions");
    }
    if b.abcs.is_empty() {
        flag("no-abc");
    }
    let mut host_methods: BTreeSet<String> = BTreeSet::new();
    let mut x: BTreeSet<String> = BTreeSet::new();
    for payload in &b.abcs {
        let abc = Avm2Reader::new(payload).read().map_err(|e| format!("abc: {e}"))?;
        if strings_dump {
            for s in &abc.constant_pool.strings {
                println!("S\t{}", String::from_utf8_lossy(s));
            }
        }
        let has = |needle: &str| abc.constant_pool.strings.iter().any(|s| s == needle.as_bytes());
        for (needle, f) in [
            ("BGSCodeObj", "str:BGSCodeObj"),
            ("onCodeObjCreate", "str:onCodeObjCreate"),
            ("onCodeObjDestruction", "str:onCodeObjDestruction"),
            ("BGSExternalInterface", "str:BGSExternalInterface"),
            ("flash.external", "str:flash.external"),
            ("ExternalInterface", "str:ExternalInterface"),
            ("addCallback", "str:addCallback"),
            ("SetVariable", "str:SetVariable"),
            ("fscommand", "str:fscommand"),
            ("BSUIDataManager", "str:BSUIDataManager"),
            ("UIDataShuttleConnector", "str:UIDataShuttleConnector"),
            ("FromClientDataEvent", "str:FromClientDataEvent"),
            ("DispatchEventToGame", "str:DispatchEventToGame"),
            ("BSUIEventDispatcherBackend", "str:BSUIEventDispatcherBackend"),
        ] {
            if has(needle) {
                lines.insert(format!("E\t{movie}\t{f}"));
            }
        }
        // classes this ABC itself DEFINES in scaleform.gfx (a stub would hide a native class)
        for inst in &abc.instances {
            if let Some((nss, name)) = mn(&abc, inst.name) {
                if nss.iter().any(|n| n == "scaleform.gfx") {
                    lines.insert(format!("E\t{movie}\tdefines-scaleform.gfx.{name}"));
                }
            }
        }
        // class-level references: every scaleform.gfx multiname in the pool
        for i in 1..=abc.constant_pool.multinames.len() {
            if let Some(c) = gfx_class(&abc, Index::new(i as u32)) {
                x.insert(format!("X\t{movie}\t{c}\tAS3-class\tpool-reference"));
            }
        }
        // member-level references and host calls, per method body
        for body in &abc.method_bodies {
            let mut rd = Avm2Reader::new(&body.code);
            let mut ops = Vec::new();
            while !rd.as_slice().is_empty() {
                match rd.read_op() {
                    Ok(op) => ops.push(op),
                    Err(_) => break,
                }
            }
            let mut last_class: Option<(String, usize)> = None;
            for (i, op) in ops.iter().enumerate() {
                match op {
                    Op::GetLex { index } | Op::FindPropStrict { index } | Op::FindProperty { index } => {
                        if let Some(c) = gfx_class(&abc, *index) {
                            last_class = Some((c, i));
                        }
                        if local(&abc, *index).as_deref() == Some("BGSCodeObj") {
                            if let Some(m) = bgs_method(&abc, &ops[i + 1..]) {
                                host_methods.insert(m);
                            }
                        }
                    }
                    Op::GetProperty { index } if local(&abc, *index).as_deref() == Some("BGSCodeObj") => {
                        if let Some(m) = bgs_method(&abc, &ops[i + 1..]) {
                            host_methods.insert(m);
                        }
                    }
                    Op::CallProperty { index, .. } | Op::CallPropVoid { index, .. }
                    | Op::CallPropLex { index, .. } | Op::GetProperty { index }
                    | Op::SetProperty { index } | Op::InitProperty { index } => {
                        if let (Some((c, at)), Some(name)) = (&last_class, local(&abc, *index)) {
                            let members = AS3_CLASSES.iter().find(|(cn, _)| cn == c).map(|(_, m)| *m).unwrap_or(&[]);
                            if i - at <= 64 && members.contains(&name.as_str()) {
                                let kind = match op {
                                    Op::CallProperty { .. } | Op::CallPropVoid { .. } | Op::CallPropLex { .. } => "AS3-method",
                                    _ => "AS3-prop",
                                };
                                x.insert(format!("X\t{movie}\t{c}.{name}\t{kind}\tgetlex-proximity<=64ops"));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    for m in host_methods {
        lines.insert(format!("H\t{movie}\t{m}\t-\t-"));
    }
    for l in lines.iter().chain(x.iter()) {
        println!("{l}");
    }
    Ok(())
}

/// `stubs` mode: print the body of every method of every `scaleform.gfx.*` class the movie itself
/// defines, to see whether the in-movie definition is an empty stub or does work.
fn dump_stubs(data: &[u8]) -> Result<(), String> {
    let buf = swf::decompress_swf(data).map_err(|e| format!("decompress: {e}"))?;
    let mut b = Blocks { actions: vec![], abcs: vec![] };
    collect(tag_list(&buf.data), &mut b, 0);
    for payload in &b.abcs {
        let abc = Avm2Reader::new(payload).read().map_err(|e| format!("abc: {e}"))?;
        for (ci, inst) in abc.instances.iter().enumerate() {
            let Some((nss, cname)) = mn(&abc, inst.name) else { continue };
            if !nss.iter().any(|n| n == "scaleform.gfx") {
                continue;
            }
            println!("CLASS scaleform.gfx.{cname} (interface={})", inst.is_interface);
            let traits = inst.traits.iter().map(|t| ("inst", t)).chain(abc.classes[ci].traits.iter().map(|t| ("static", t)));
            for (scope, t) in traits {
                let tname = local(&abc, t.name).unwrap_or_default();
                let method = match &t.kind {
                    swf::avm2::types::TraitKind::Method { method, .. }
                    | swf::avm2::types::TraitKind::Getter { method, .. }
                    | swf::avm2::types::TraitKind::Setter { method, .. } => Some(*method),
                    _ => None,
                };
                let Some(method) = method else {
                    println!("  {scope} {tname}: (slot/const)");
                    continue;
                };
                let body = abc.method_bodies.iter().find(|bd| bd.method == method);
                let ops: Vec<String> = match body {
                    Some(bd) => {
                        let mut rd = Avm2Reader::new(&bd.code);
                        let mut v = Vec::new();
                        while !rd.as_slice().is_empty() {
                            match rd.read_op() {
                                Ok(op) => v.push(format!("{op:?}")),
                                Err(_) => break,
                            }
                        }
                        v
                    }
                    None => vec!["<no body>".into()],
                };
                println!("  {scope} {tname}: {} ops {}", ops.len(), if ops.len() <= 12 { ops.join(" | ") } else { String::from("...") });
            }
        }
    }
    Ok(())
}

/// `disasm` mode: dump every method of every class whose name contains `needle`, with constant-pool
/// indices resolved to names, to read how a movie reaches the host.
fn dump_class(data: &[u8], needle: &str) -> Result<(), String> {
    let buf = swf::decompress_swf(data).map_err(|e| format!("decompress: {e}"))?;
    let mut b = Blocks { actions: vec![], abcs: vec![] };
    collect(tag_list(&buf.data), &mut b, 0);
    for payload in &b.abcs {
        let abc = Avm2Reader::new(payload).read().map_err(|e| format!("abc: {e}"))?;
        let resolve = |dbg: String| -> String {
            let mut out = String::new();
            let mut rest = dbg.as_str();
            while let Some(i) = rest.find("Index(") {
                out.push_str(&rest[..i]);
                let tail = &rest[i..];
                let end = tail.find(">)").map(|e| e + 2).unwrap_or(tail.len());
                let tok = &tail[..end];
                let n: u32 = tok["Index(".len()..].split(',').next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
                let name = if tok.contains("Multiname") {
                    local(&abc, Index::new(n)).map(|x| format!("{x}")).unwrap_or_else(|| format!("mn#{n}"))
                } else if tok.contains("String") {
                    pool_str(&abc, Index::new(n)).map(|x| format!("{x:?}")).unwrap_or_else(|| format!("str#{n}"))
                } else {
                    format!("#{n}")
                };
                out.push_str(&name);
                rest = &tail[end..];
            }
            out.push_str(rest);
            out
        };
        for (ci, inst) in abc.instances.iter().enumerate() {
            let Some((nss, cname)) = mn(&abc, inst.name) else { continue };
            if !cname.contains(needle) {
                continue;
            }
            println!("CLASS {}::{cname}", nss.join("|"));
            let traits = inst.traits.iter().map(|t| ("inst", t)).chain(abc.classes[ci].traits.iter().map(|t| ("static", t)));
            for (scope, t) in traits {
                let tname = local(&abc, t.name).unwrap_or_default();
                let method = match &t.kind {
                    swf::avm2::types::TraitKind::Method { method, .. }
                    | swf::avm2::types::TraitKind::Getter { method, .. }
                    | swf::avm2::types::TraitKind::Setter { method, .. } => Some(*method),
                    _ => None,
                };
                let Some(method) = method else { println!("  {scope} {tname}: (slot/const)"); continue };
                println!("  {scope} {tname}:");
                if let Some(bd) = abc.method_bodies.iter().find(|bd| bd.method == method) {
                    let mut rd = Avm2Reader::new(&bd.code);
                    while !rd.as_slice().is_empty() {
                        match rd.read_op() {
                            Ok(op) => println!("      {}", resolve(format!("{op:?}"))),
                            Err(_) => break,
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// `bodies` mode: print every method body (class trait or not — frame scripts included) that
/// contains an op naming `needle`, with a few ops of context. Used to spot-check the census.
fn dump_bodies(data: &[u8], needle: &str) -> Result<(), String> {
    let buf = swf::decompress_swf(data).map_err(|e| format!("decompress: {e}"))?;
    let mut b = Blocks { actions: vec![], abcs: vec![] };
    collect(tag_list(&buf.data), &mut b, 0);
    for payload in &b.abcs {
        let abc = Avm2Reader::new(payload).read().map_err(|e| format!("abc: {e}"))?;
        for (bi, body) in abc.method_bodies.iter().enumerate() {
            let mut rd = Avm2Reader::new(&body.code);
            let mut ops = Vec::new();
            while !rd.as_slice().is_empty() {
                match rd.read_op() {
                    Ok(op) => ops.push(op),
                    Err(_) => break,
                }
            }
            for (i, op) in ops.iter().enumerate() {
                let idx = match op {
                    Op::GetLex { index } | Op::FindPropStrict { index } => Some(*index),
                    _ => None,
                };
                if idx.and_then(|ix| local(&abc, ix)).as_deref() == Some(needle) {
                    println!("body#{bi} op{i}:");
                    for o in ops.iter().skip(i).take(8) {
                        let name = match o {
                            Op::GetProperty { index } | Op::SetProperty { index } | Op::CallProperty { index, .. }
                            | Op::CallPropVoid { index, .. } | Op::GetLex { index } | Op::FindPropStrict { index } => {
                                format!("{o:?} => {}", local(&abc, *index).unwrap_or_default())
                            }
                            _ => format!("{o:?}"),
                        };
                        println!("    {name}");
                    }
                }
            }
        }
    }
    Ok(())
}

fn swf_ceiling(data: &[u8]) -> Result<(), String> {
    // Same absolute ceiling idea as prepare.rs: refuse a header declaring > 256 MiB.
    if data.len() >= 8 {
        let declared = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        if declared > 256 * 1024 * 1024 {
            return Err(format!("declared length {declared} over ceiling"));
        }
    }
    Ok(())
}

// ---- driver --------------------------------------------------------------------------------

enum Arc {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Arc {
    fn open(path: &str) -> std::io::Result<Self> {
        if path.to_ascii_lowercase().ends_with(".bsa") {
            Ok(Arc::Bsa(BsaArchive::open(path)?))
        } else {
            Ok(Arc::Ba2(Ba2Archive::open(path)?))
        }
    }
    fn files(&self) -> Vec<String> {
        let mut v: Vec<String> = match self {
            Arc::Bsa(a) => a.list_files().into_iter().map(str::to_string).collect(),
            Arc::Ba2(a) => a.list_files().into_iter().map(str::to_string).collect(),
        };
        v.sort();
        v
    }
    fn extract(&self, p: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Arc::Bsa(a) => a.extract(p),
            Arc::Ba2(a) => a.extract(p),
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() >= 2, "usage: gfx_extension_census <as2|as3|strings> <archive> [movie]");
    let archive = Arc::open(&args[1])?;
    let swfs: Vec<String> = archive
        .files()
        .into_iter()
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l.ends_with(".swf") || l.ends_with(".gfx")
        })
        .collect();
    match args[0].as_str() {
        "disasm" => {
            let data = archive.extract(&args[2])?;
            dump_class(&data, &args[3]).map_err(anyhow::Error::msg)?;
        }
        "bodies" => {
            let data = archive.extract(&args[2])?;
            dump_bodies(&data, &args[3]).map_err(anyhow::Error::msg)?;
        }
        "stubs" => {
            let data = archive.extract(&args[2])?;
            dump_stubs(&data).map_err(anyhow::Error::msg)?;
        }
        "strings" => {
            let data = archive.extract(&args[2])?;
            run_as3(&args[2], &data, true).map_err(anyhow::Error::msg)?;
        }
        mode @ ("as2" | "as3") => {
            for movie in &swfs {
                let data = match archive.extract(movie) {
                    Ok(d) => d,
                    Err(e) => {
                        println!("M\t{movie}\t0\textract-failed: {e}");
                        continue;
                    }
                };
                let status = if mode == "as2" {
                    let host = byroredux_ui::avm1_host::referenced_host_methods(&data);
                    if let Ok(inv) = &host {
                        for m in &inv.methods {
                            let (lo, hi) = inv.arg_counts.get(m).copied().unwrap_or((0, 0));
                            println!("H\t{movie}\t{m}\t{lo}\t{hi}");
                        }
                        if inv.unresolved > 0 {
                            println!("U\t{movie}\t{}", inv.unresolved);
                        }
                    }
                    let ext = run_as2(movie, &data);
                    match (host, ext) {
                        (Err(e), _) => Err(format!("host-scan: {e}")),
                        (_, r) => r,
                    }
                } else {
                    run_as3(movie, &data, false)
                };
                match status {
                    Ok(()) => println!("M\t{movie}\t{}\tok", data.len()),
                    Err(e) => println!("M\t{movie}\t{}\terr: {e}", data.len()),
                }
            }
        }
        other => anyhow::bail!("unknown mode {other}"),
    }
    Ok(())
}
