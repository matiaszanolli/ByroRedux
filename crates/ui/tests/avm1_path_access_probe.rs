//! M48 Phase 0 probe (#6643, PRD `game-ui-integration` Q2 / GUI-PATH-001):
//! can an AVM1 adapter reach `_root` members through Ruffle's supported
//! ExternalInterface boundary?
//!
//! Q2 is decided as "adapter injection on both VMs"; the PRD makes this the
//! design section's first question and falls back to Alternative A if the
//! answer is no. This test answers it with the shipped Ruffle pin and no
//! engine code: a hand-assembled AVM1 movie carries (a) stand-in game state
//! under `_root`, and (b) the three adapter callbacks a path service needs
//! (get, set, invoke), registered with `ExternalInterface.addCallback`. The
//! host then drives them with `Player::call_internal_interface`, the same
//! call `SwfPlayer::invoke_callback` makes.
//!
//! **Answer: yes.** Get (flat and nested), set (existing and new members) and
//! invoke (return value and side effects) all cross the boundary with the
//! pinned Ruffle. The AS2 class is reached as `flash.external.ExternalInterface`
//! (a bare `ExternalInterface` is compile-time sugar for an `import`, so a
//! bytecode adapter spells the full path, as Ruffle's own fixture does).
//! `GetVariable` resolves a dotted path string, which is all the get / set
//! adapter needs; `CallMethod` with an undefined method name calls the
//! function object itself. A bare `eval(p)` cannot tell an absent path from a
//! present `undefined`, so the real adapter owns a presence check
//! (GUI-PATH-002) — pinned by the last test below.
//!
//! Hermetic: no game data, no renderer, no GPU. The AVM1 bytecode is built
//! here (SWF action ids per the SWF 19 specification, chapter "ActionScript
//! 1.0/2.0 actions"), so the probe documents exactly what a Phase 2 adapter
//! has to inject into a vanilla movie.

use ruffle_core::external::{ExternalInterfaceProvider, Value};
use ruffle_core::tag_utils::SwfMovieData;
use ruffle_core::{FloatDuration, LoadBehavior, PlayerBuilder};
use swf::{write_swf, Fixed8, Header, Tag};

/// Names registered through the transport; the real adapter would use the
/// `__byro` prefix GUI-PATH-004 reserves (`__byro_path_get` etc.).
const GET: &str = "__byro_path_get";
const SET: &str = "__byro_path_set";
const CALL: &str = "__byro_path_call";

struct Provider;

impl ExternalInterfaceProvider for Provider {
    fn call_method(
        &self,
        _context: &mut ruffle_core::context::UpdateContext<'_>,
        _name: &str,
        _args: &[Value],
    ) -> Value {
        Value::Undefined
    }

    fn on_callback_available(&self, _name: &str) {}

    fn get_id(&self) -> Option<String> {
        Some("avm1-path-access-probe".to_string())
    }
}

/// A just-big-enough AVM1 assembler: one action per call, no jumps.
#[derive(Default)]
struct Asm(Vec<u8>);

impl Asm {
    const POP: u8 = 0x17;
    const GET_VARIABLE: u8 = 0x1C;
    const SET_VARIABLE: u8 = 0x1D;
    const RETURN: u8 = 0x3E;
    const GET_MEMBER: u8 = 0x4E;
    const INIT_OBJECT: u8 = 0x43;
    const ADD2: u8 = 0x47;
    const CALL_METHOD: u8 = 0x52;
    const PUSH: u8 = 0x96;
    const DEFINE_FUNCTION: u8 = 0x9B;

    fn op(&mut self, code: u8) -> &mut Self {
        self.0.push(code);
        self
    }

    fn push(&mut self, ty: u8, data: &[u8]) -> &mut Self {
        self.0.push(Self::PUSH);
        self.0.extend_from_slice(&((data.len() + 1) as u16).to_le_bytes());
        self.0.push(ty);
        self.0.extend_from_slice(data);
        self
    }

    fn s(&mut self, text: &str) -> &mut Self {
        let mut data = text.as_bytes().to_vec();
        data.push(0);
        self.push(0, &data)
    }

    fn int(&mut self, value: i32) -> &mut Self {
        self.push(7, &value.to_le_bytes())
    }

    fn null(&mut self) -> &mut Self {
        self.push(2, &[])
    }

    fn undefined(&mut self) -> &mut Self {
        self.push(3, &[])
    }

    fn boolean(&mut self, value: bool) -> &mut Self {
        self.push(5, &[value as u8])
    }

    /// `DefineFunction` (anonymous: the function object is pushed on the stack).
    fn function(&mut self, params: &[&str], body: &Asm) -> &mut Self {
        let mut head = vec![0u8]; // empty name
        head.extend_from_slice(&(params.len() as u16).to_le_bytes());
        for param in params {
            head.extend_from_slice(param.as_bytes());
            head.push(0);
        }
        head.extend_from_slice(&(body.0.len() as u16).to_le_bytes());
        self.0.push(Self::DEFINE_FUNCTION);
        self.0.extend_from_slice(&(head.len() as u16).to_le_bytes());
        self.0.extend_from_slice(&head);
        self.0.extend_from_slice(&body.0);
        self
    }

    /// `ExternalInterface.addCallback(name, null, <function on the stack>)`.
    /// The function must already be pushed; arguments go on in reverse order.
    fn register(&mut self, name: &str) -> &mut Self {
        self.null()
            .s(name)
            .int(3)
            // AS2 reaches the class as `flash.external.ExternalInterface`; a bare
            // `ExternalInterface` is only a global after an `import`, which is
            // compile-time sugar, so the bytecode spells the full path.
            .s("flash")
            .op(Self::GET_VARIABLE)
            .s("external")
            .op(Self::GET_MEMBER)
            .s("ExternalInterface")
            .op(Self::GET_MEMBER)
            .s("addCallback")
            .op(Self::CALL_METHOD)
            .op(Self::POP)
    }
}

/// The movie: stand-in game state, then the adapter. The state mimics what a
/// vanilla HUD keeps under `_root` (a meter object with a value and a setter).
fn movie_bytes() -> Vec<u8> {
    let mut a = Asm::default();

    // _root.meter = {}; _root.meter.value = 7; _root.hp = 42;
    a.s("_root.meter").int(0).op(Asm::INIT_OBJECT).op(Asm::SET_VARIABLE);
    a.s("_root.meter.value").int(7).op(Asm::SET_VARIABLE);
    a.s("_root.hp").int(42).op(Asm::SET_VARIABLE);

    // _root.meter.bump = function (n) { _root.meter.value = n; return n + 1; }
    let mut bump = Asm::default();
    bump.s("_root.meter.value").s("n").op(Asm::GET_VARIABLE).op(Asm::SET_VARIABLE);
    bump.s("n").op(Asm::GET_VARIABLE).int(1).op(Asm::ADD2).op(Asm::RETURN);
    a.s("_root.meter.bump").function(&["n"], &bump).op(Asm::SET_VARIABLE);

    // get(p): return eval(p)  — GetVariable resolves a dotted path string.
    let mut get = Asm::default();
    get.s("p").op(Asm::GET_VARIABLE).op(Asm::GET_VARIABLE).op(Asm::RETURN);
    a.function(&["p"], &get).register(GET);

    // set(p, v): eval(p) = v; return true
    let mut set = Asm::default();
    set.s("p").op(Asm::GET_VARIABLE).s("v").op(Asm::GET_VARIABLE).op(Asm::SET_VARIABLE);
    set.boolean(true).op(Asm::RETURN);
    a.function(&["p", "v"], &set).register(SET);

    // call(p, arg): return eval(p)(arg) — CallMethod with an undefined method
    // name calls the object itself as a function.
    let mut call = Asm::default();
    call.s("arg").op(Asm::GET_VARIABLE).int(1);
    call.s("p").op(Asm::GET_VARIABLE).op(Asm::GET_VARIABLE).undefined();
    call.op(Asm::CALL_METHOD).op(Asm::RETURN);
    a.function(&["p", "arg"], &call).register(CALL);

    a.op(0x00); // End

    let mut bytes = Vec::new();
    let mut header = Header::default_with_swf_version(8);
    header.frame_rate = Fixed8::from_f32(30.0);
    header.num_frames = 1;
    write_swf(&header, &[Tag::DoAction(&a.0), Tag::ShowFrame], &mut bytes).expect("write SWF");
    bytes
}

fn start() -> std::sync::Arc<std::sync::Mutex<ruffle_core::Player>> {
    let movie = SwfMovieData::from_data(&movie_bytes(), "file:///probe.swf".to_string(), None, None)
        .expect("probe SWF must parse");
    assert!(!movie.is_action_script_3(), "the probe is an AVM1 movie");
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

fn call(
    player: &std::sync::Arc<std::sync::Mutex<ruffle_core::Player>>,
    name: &str,
    args: Vec<Value>,
) -> Value {
    player.lock().unwrap().call_internal_interface(name, args)
}

/// Q2's question: an AVM1 adapter reaches `_root` members, nested ones
/// included, through ExternalInterface alone.
#[test]
fn an_avm1_adapter_reads_root_members_by_path() {
    let player = start();
    assert_eq!(call(&player, GET, vec!["_root.hp".into()]), Value::Number(42.0));
    assert_eq!(call(&player, GET, vec!["_root.meter.value".into()]), Value::Number(7.0));
    // `_root.meter` is an object: the transport hands back its enumerable
    // members, so a structured read is possible too.
    match call(&player, GET, vec!["_root.meter".into()]) {
        Value::Object(members) => {
            assert_eq!(members.get("value"), Some(&Value::Number(7.0)));
        }
        other => panic!("expected an object, got {other:?}"),
    }
}

#[test]
fn an_avm1_adapter_writes_root_members_by_path() {
    let player = start();
    let ok = call(&player, SET, vec!["_root.meter.value".into(), Value::Number(31.0)]);
    assert_eq!(ok, Value::Bool(true));
    assert_eq!(call(&player, GET, vec!["_root.meter.value".into()]), Value::Number(31.0));
    // Creating a member that did not exist is also a plain write.
    call(&player, SET, vec!["_root.fresh".into(), Value::String("x".into())]);
    assert_eq!(call(&player, GET, vec!["_root.fresh".into()]), Value::String("x".into()));
}

#[test]
fn an_avm1_adapter_invokes_a_root_function_by_path() {
    let player = start();
    let out = call(&player, CALL, vec!["_root.meter.bump".into(), Value::Number(9.0)]);
    assert_eq!(out, Value::Number(10.0), "the function's return value crosses the boundary");
    assert_eq!(
        call(&player, GET, vec!["_root.meter.value".into()]),
        Value::Number(9.0),
        "and its side effect on movie state is visible to the next read"
    );
}

/// GUI-PATH-002: an absent path must come back typed, not as a silent no-op.
/// This records what the bare `eval(p)` adapter gives for an absent path, so
/// the real adapter knows it must add its own presence check.
#[test]
fn an_absent_path_reads_as_a_bare_undefined_so_the_adapter_must_add_a_presence_check() {
    let player = start();
    let absent = call(&player, GET, vec!["_root.no.such.member".into()]);
    // The bare adapter cannot tell "absent" from "present and undefined":
    // both arrive as Undefined/Null. A real adapter returns a tagged result.
    assert!(
        matches!(absent, Value::Undefined | Value::Null),
        "absent path read as {absent:?}"
    );
}
