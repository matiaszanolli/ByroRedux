//! Tests for the AVM1 `GameDelegate.call` scanner (#3103).
//!
//! The synthetic cases pin the decoding rules; the corpus sweep is what
//! actually answers the issue, and is `#[ignore]`d behind installed game data
//! the same way #2966's Fallout 4 sweep is.
//!
//! Blocks are assembled through `swf`'s own AVM1 writer rather than by hand,
//! so a test describes the instruction sequence it means and the encoding
//! stays the crate's problem.

use super::*;
use swf::avm1::types::{ConstantPool, DefineFunction, Push};
use swf::avm1::write::Writer;
use swf::SwfStr;

fn assemble(actions: &[Action<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = Writer::new(&mut out, 15);
    for action in actions {
        writer.write_action(action).expect("write action");
    }
    writer.write_action(&Action::End).expect("write end");
    out
}

fn text(value: &str) -> &SwfStr {
    SwfStr::from_utf8_str(value)
}

fn push<'a>(values: Vec<Value<'a>>) -> Action<'a> {
    Action::Push(Push { values })
}

/// The exact shape a vanilla Skyrim movie compiles `GameDelegate.call(name,
/// args)` to, transcribed from a real action dump — see the module doc.
fn vanilla_call_site<'a>(method: &'a str) -> Vec<Action<'a>> {
    vec![
        push(vec![Value::Int(0)]),
        Action::InitArray,
        push(vec![
            Value::Str(text(method)),
            Value::Int(2),
            Value::Str(text("gfx")),
        ]),
        Action::GetVariable,
        push(vec![Value::Str(text("io"))]),
        Action::GetMember,
        push(vec![Value::Str(text("GameDelegate"))]),
        Action::GetMember,
        push(vec![Value::Str(text("call"))]),
        Action::CallMethod,
        Action::Pop,
    ]
}

#[test]
fn reads_the_method_name_out_of_a_vanilla_call_site() {
    let found = scan_block(&assemble(&vanilla_call_site("CloseMenu")), 15, &[]);
    assert_eq!(
        found.methods.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["CloseMenu"]
    );
    assert_eq!(found.unresolved, 0);
}

/// The receiver is matched on the last component of the member chain, so a
/// movie that `import`s the class and calls it by its short name resolves the
/// same way a spelled-out `gfx.io.GameDelegate` does. A scanner that only
/// matched the long form would measure the vanilla corpus correctly and any
/// other one silently short.
#[test]
fn an_imported_short_name_receiver_resolves_the_same() {
    let actions = vec![
        push(vec![Value::Int(0)]),
        Action::InitArray,
        push(vec![
            Value::Str(text("ShowItemsList")),
            Value::Int(2),
            Value::Str(text("GameDelegate")),
        ]),
        Action::GetVariable,
        push(vec![Value::Str(text("call"))]),
        Action::CallMethod,
    ];
    let found = scan_block(&assemble(&actions), 15, &[]);
    assert_eq!(
        found.methods.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["ShowItemsList"]
    );
    assert_eq!(found.unresolved, 0);
}

/// A call on anything else is not a host call. `ExternalInterface.call` sits
/// in the same movies and takes a string first argument too, so matching on
/// the method name alone would fold a different transport's names into the
/// catalog.
#[test]
fn a_call_on_another_receiver_is_not_a_host_call() {
    let actions = vec![
        push(vec![
            Value::Str(text("NotAHostMethod")),
            Value::Int(1),
            Value::Str(text("ExternalInterface")),
        ]),
        Action::GetVariable,
        push(vec![Value::Str(text("call"))]),
        Action::CallMethod,
    ];
    let found = scan_block(&assemble(&actions), 15, &[]);
    assert!(found.methods.is_empty(), "{:?}", found.methods);
    assert_eq!(found.unresolved, 0);
}

/// A variable read is not a literal, even though the two are the same bytes
/// by the time they reach the operand stack.
///
/// `GameDelegate.call(someVar, args)` puts a variable where a literal name
/// would sit; reading it as a name invents a host method called `someVar`,
/// which would then show up as an "uncataloged" entry and, under #2966's
/// regeneration rule, get written into the catalog. The site is counted as
/// unresolved instead — the measurement says it cannot see this one, which is
/// a fact a later sweep can act on.
#[test]
fn a_dynamic_method_name_counts_as_unresolved_rather_than_naming_the_variable() {
    let actions = vec![
        push(vec![Value::Str(text("someVar"))]),
        Action::GetVariable,
        push(vec![Value::Int(1), Value::Str(text("GameDelegate"))]),
        Action::GetVariable,
        push(vec![Value::Str(text("call"))]),
        Action::CallMethod,
    ];
    let found = scan_block(&assemble(&actions), 15, &[]);
    assert!(found.methods.is_empty(), "{:?}", found.methods);
    assert_eq!(found.unresolved, 1);
}

/// Constant-pool indices, and the inheritance of the pool into a nested
/// function body. Both are load-bearing: string literals in real movies are
/// pool references, and a body that does not inherit its enclosing pool
/// resolves every name to nothing — which is most of why a first pass over
/// this corpus reads almost empty.
#[test]
fn a_nested_function_body_inherits_the_constant_pool() {
    let body = assemble(&[
        push(vec![
            Value::ConstantPool(0),
            Value::Int(1),
            Value::ConstantPool(1),
        ]),
        Action::GetVariable,
        push(vec![Value::ConstantPool(2)]),
        Action::CallMethod,
    ]);
    let code = assemble(&[
        Action::ConstantPool(ConstantPool {
            strings: vec![text("EquipItem"), text("GameDelegate"), text("call")],
        }),
        Action::DefineFunction(DefineFunction {
            name: text(""),
            params: Vec::new(),
            actions: &body,
        }),
    ]);

    let found = scan_block(&code, 15, &[]);
    assert_eq!(
        found.methods.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["EquipItem"],
        "a nested body must resolve pool indices its parent set",
    );
    assert_eq!(found.unresolved, 0);
}

/// An unmodelled action clears the stack rather than desyncing it. Pinning
/// this keeps "conservative" from quietly becoming "wrong" if someone extends
/// the match arm list later: the site is lost and *counted*, never renamed.
#[test]
fn an_unmodelled_action_loses_the_site_instead_of_misreading_it() {
    let actions = vec![
        push(vec![
            Value::Str(text("CloseMenu")),
            Value::Int(1),
            Value::Str(text("GameDelegate")),
        ]),
        Action::GetVariable,
        // Anything the walk does not model, sitting between the operands and
        // the call.
        Action::Trace,
        push(vec![Value::Str(text("call"))]),
        Action::CallMethod,
    ];
    let found = scan_block(&assemble(&actions), 15, &[]);
    assert!(found.methods.is_empty(), "{:?}", found.methods);
}

/// #3103 — measure the Skyrim/AVM1 catalog against the installed corpus the
/// way #2966 did for Fallout 4, instead of trusting a source snapshot.
///
/// `SKYRIM_SKYUI_METHODS` is a read of SkyUI's decompiled ActionScript at one
/// tree. This sweep is the other direction: every `GameDelegate.call` site in
/// every movie the game actually ships, walked out of the bytecode. The two
/// disagreeing is information either way, and the sweep prints the difference
/// in both directions rather than asserting a total match — see the note on
/// the assertion below for why only one of those directions can gate.
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn installed_skyrim_host_calls_are_all_cataloged() {
    let data = std::env::var("BYROREDUX_SKYRIMSE_DATA").unwrap_or_else(|_| {
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data".to_string()
    });
    let archive_path = std::path::Path::new(&data).join("Skyrim - Interface.bsa");
    let Ok(archive) = byroredux_bsa::BsaArchive::open(&archive_path) else {
        eprintln!("skipping: {archive_path:?} not available (set BYROREDUX_SKYRIMSE_DATA)");
        return;
    };

    let movies: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".swf"))
        .map(str::to_string)
        .collect();
    assert!(
        !movies.is_empty(),
        "Skyrim - Interface.bsa must contain menu SWFs",
    );

    let catalog = crate::ScaleformHostCatalog::for_profile(crate::ScaleformProfile::SkyrimAvm1);
    let mut found = Avm1HostCallInventory::default();
    let mut failures = Vec::new();
    let mut movies_with_calls = 0usize;
    for movie in &movies {
        let Ok(swf) = archive.extract(movie) else {
            failures.push(format!("{movie}: extract failed"));
            continue;
        };
        match referenced_host_methods(&swf) {
            Ok(inventory) => {
                if !inventory.methods.is_empty() {
                    movies_with_calls += 1;
                }
                found.merge(inventory);
            }
            Err(error) => failures.push(format!("{movie}: scan failed: {error}")),
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} Skyrim SWFs could not be scanned:\n{}",
        failures.len(),
        movies.len(),
        failures.join("\n"),
    );

    let uncataloged: Vec<&str> = found
        .methods
        .iter()
        .map(String::as_str)
        .filter(|method| !catalog.contains(method))
        .collect();
    let uncalled: Vec<&str> = catalog
        .methods()
        .iter()
        .map(|method| method.name)
        .filter(|name| !found.methods.contains(*name))
        .collect();

    eprintln!(
        "Skyrim AVM1 sweep: {} movies, {movies_with_calls} with host calls, \
         {} distinct methods, {} unresolved call sites",
        movies.len(),
        found.methods.len(),
        found.unresolved,
    );
    eprintln!("  uncataloged ({}): {uncataloged:?}", uncataloged.len());
    // Not an assertion. The catalog is SkyUI-sourced and SkyUI is a *mod* —
    // it replaces these menus and adds host calls of its own, so a catalog
    // entry with no vanilla call site is the expected steady state, not a
    // defect. Naming them is still worth doing: it is the measured size of
    // the gap between the shipped corpus and the snapshot the catalog came
    // from, and the number a future SkyUI-installed sweep would close.
    eprintln!(
        "  cataloged but never called by vanilla ({}): {uncalled:?}",
        uncalled.len(),
    );

    // The unresolved sites are not a scanner gap, and raising the modelled
    // action set will not move them: every one passes a *runtime* name —
    // `this.callbackName`, `this.strFadeOutCallback`, a register holding a
    // value assigned by whoever opened the menu. Their shape in the bytecode
    // is `Push[Register(1), "callbackName"] | GetMember | Push[2, "gfx"] |
    // …`, i.e. a member read where a literal would sit. No static walk
    // resolves those; only running the menu does. So this is an upper bound
    // on what a static sweep can ever see, pinned so a regression that starts
    // *losing* resolvable sites shows up as the count climbing.
    assert!(
        found.unresolved <= 72,
        "{} GameDelegate.call sites resolved no literal method name, up from \
         the 72 dynamic ones the vanilla corpus contains — the extra ones are \
         sites this walk used to read and no longer can",
        found.unresolved,
    );

    // #2966's Fallout 4 sweep asserts `uncataloged.is_empty()` because that
    // catalog was *regenerated from its own sweep*. This one cannot yet:
    // regenerating needs each entry's kind (command vs request), and the
    // rule that decides it — SkyUI's "entries with a fourth callback
    // argument are requests" — does not transfer, because every vanilla call
    // site passes exactly two arguments. Classifying 68 entries by guess is
    // the kind of invented rule this measurement exists to replace, so the
    // gap is asserted *stable* rather than empty: the sweep gates on the
    // corpus not drifting, and names what regeneration still needs.
    assert_eq!(
        uncataloged.len(),
        68,
        "the shipped Skyrim menus call {} host methods absent from \
         SKYRIM_SKYUI_METHODS (was 68 when measured): {uncataloged:?}",
        uncataloged.len(),
    );
    assert!(
        found.methods.len() >= 141,
        "the sweep resolved {} distinct host methods, below the 141 measured \
         on the vanilla corpus — the scanner is reading less than it did",
        found.methods.len(),
    );
}
