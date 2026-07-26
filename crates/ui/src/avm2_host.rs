//! Fallout 4's AVM2 native-object adapter.
//!
//! Vanilla menus create `root.BGSCodeObj` themselves. The game populates that
//! dynamic object with native functions, then calls `onCodeObjCreate` on the
//! root. Ruffle intentionally keeps its AVM2 object model private, so the
//! adapter is injected as ordinary ABC bytecode before Ruffle parses the SWF.
//! The injected ActionScript schedules installation for the next AVM2 turn,
//! fills the menu-created object, and forwards each call through
//! ExternalInterface.

use swf::avm2::types::{
    AbcFile, ConstantPool, Index, Method, MethodBody, MethodFlags, Multiname, Namespace, Op,
    Script, Trait, TraitKind,
};
use swf::avm2::write::Writer;
use swf::{decompress_swf, parse_swf, write_swf, DoAbc2, DoAbc2Flag, SwfStr, Tag};

use crate::ScaleformHostCatalog;

const HELPER_PREFIX: &str = "__byro_fallout4_host_";
const INSTALL_HELPER: &str = "__byro_fallout4_install";
const READY_HELPER: &str = "__byro_fallout4_ready";
const DESTROY_HELPER: &str = "__byro_fallout4_destroy";
pub(crate) const READY_CALLBACK: &str = "__byroBGSCodeObjReady";
pub(crate) const LOADED_CALLBACK: &str = "__byroBGSAdapterLoaded";
pub(crate) const DESTROY_CALLBACK: &str = "__byroBGSCodeObjDestroy";
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

    let adapter = build_adapter_abc(catalog)
        .map_err(|error| format!("building Fallout 4 AVM2 adapter: {error}"))?;
    let tag = Tag::DoAbc2(DoAbc2 {
        flags: DoAbc2Flag::empty(),
        name: SwfStr::from_utf8_str(ADAPTER_NAME),
        data: &adapter,
    });
    let insertion_point = movie
        .tags
        .iter()
        .position(|tag| matches!(tag, Tag::ShowFrame | Tag::End))
        .unwrap_or(movie.tags.len());
    movie.tags.insert(insertion_point, tag);

    let mut patched = Vec::new();
    write_swf(movie.header.swf_header(), &movie.tags, &mut patched)
        .map_err(|error| format!("serializing patched SWF: {error}"))?;
    Ok((patched, ScaleformHostObjectState::AdapterInjected))
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
    let loader_info = Index::new(2);
    let call = Index::new(3);
    let apply = Index::new(4);
    let unshift = Index::new(5);
    let get_loader_info = Index::new(6);
    let content = Index::new(9);
    let code_object = Index::new(10);
    let on_create = Index::new(11);
    let install_helper = Index::new(12);
    let add_callback = Index::new(13);
    let ready_helper = Index::new(14);
    let set_timeout = Index::new(15);
    let on_destroy = Index::new(16);
    let destroy_helper = Index::new(17);
    let ready_callback_string = Index::new(19);
    let loaded_callback_string = Index::new(22);
    let destroy_callback_string = Index::new(25);

    let mut methods = Vec::with_capacity(catalog.len() + 4);
    let mut method_bodies = Vec::with_capacity(catalog.len() + 4);
    let mut traits = Vec::with_capacity(catalog.len() + 3);
    let mut helper_multinames = Vec::with_capacity(catalog.len());
    let mut method_property_multinames = Vec::with_capacity(catalog.len());

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
            Op::GetLex { index: loader_info },
            Op::GetLex {
                index: helper_multinames[0],
            },
            Op::CallProperty {
                index: get_loader_info,
                num_args: 1,
            },
            Op::GetProperty { index: content },
            Op::CallPropVoid {
                index: on_destroy,
                num_args: 0,
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
        params: Vec::new(),
        return_type: Index::new(0),
        flags: MethodFlags::empty(),
        body: Some(installer_body),
    });
    let mut install_ops = vec![
        Op::GetLocal { index: 0 },
        Op::PushScope,
        Op::GetLex { index: loader_info },
        Op::GetLex {
            index: helper_multinames[0],
        },
        Op::CallProperty {
            index: get_loader_info,
            num_args: 1,
        },
        Op::GetProperty { index: content },
        Op::SetLocal { index: 2 },
        Op::GetLocal { index: 2 },
        Op::GetProperty { index: code_object },
        Op::SetLocal { index: 3 },
    ];
    for (helper, property) in helper_multinames.iter().zip(&method_property_multinames) {
        install_ops.extend([
            Op::GetLocal { index: 3 },
            Op::GetLex { index: *helper },
            Op::SetProperty { index: *property },
        ]);
    }
    install_ops.extend([
        Op::GetLocal { index: 2 },
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
        max_stack: 2,
        num_locals: 4,
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
            Op::FindPropStrict { index: set_timeout },
            Op::GetLex {
                index: install_helper,
            },
            Op::PushByte { value: 0 },
            Op::CallPropVoid {
                index: set_timeout,
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
    use ruffle_core::swf::avm2::read::Reader;
    use ruffle_core::tag_utils::SwfMovie;
    use ruffle_core::{LoadBehavior, PlayerBuilder};
    use swf::avm2::types::{AbcFile, ConstantPool, Index, Method, MethodBody, MethodFlags, Script};
    use swf::avm2::write::Writer;
    use swf::extensions::ReadSwfExt;
    use swf::{DoAbc2, DoAbc2Flag, FileAttributes, SwfStr, Tag};

    use super::{build_adapter_abc, inject_host_object_adapter, write_ops, LOADED_CALLBACK};
    use crate::{ScaleformHostBridge, ScaleformHostCatalog, ScaleformProfile};

    #[test]
    fn generated_adapter_is_valid_abc_with_one_helper_per_method() {
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
        let bytes = build_adapter_abc(catalog).unwrap();
        let abc = Reader::new(&bytes).read().unwrap();

        assert_eq!(abc.scripts.len(), 1);
        assert_eq!(abc.scripts[0].traits.len(), catalog.len() + 3);
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
        assert_eq!(installed_properties, catalog.len());
        assert_eq!(void_calls, 3);
    }

    #[test]
    fn injected_adapter_executes_as_eager_abc() {
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

        let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
        let (patched, state) = inject_host_object_adapter(&source, bridge.catalog()).unwrap();
        assert_eq!(state, super::ScaleformHostObjectState::AdapterInjected);
        let movie = SwfMovie::from_data(
            &patched,
            "file:///fallout4-host-adapter.swf".to_string(),
            None,
        )
        .unwrap();
        let player = PlayerBuilder::new()
            .with_external_interface(bridge.provider())
            .with_load_behavior(LoadBehavior::Blocking)
            .with_movie(movie)
            .build();
        player.lock().unwrap().run_frame();

        assert!(bridge.has_callback(LOADED_CALLBACK));
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
}
