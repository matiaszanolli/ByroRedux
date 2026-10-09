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
    // #4720 — the scanner keeps the call's own argument count: the
    // two-argument vanilla shape is a command under the fourth-argument
    // rule. #5274 — (min, max) so a mixed-arity name is detectable.
    assert_eq!(found.arg_counts.get("CloseMenu"), Some(&(2, 2)));
}

/// #4720 — a four-argument `GameDelegate.call` site passes scope + response
/// callback, which is SkyUI's fourth-argument rule typing it `Request`. The
/// scanner had this count in hand all along and discarded it; it now rides
/// the inventory so the corpus sweep can pin "four-arg ⇔ Request" against
/// the catalog.
#[test]
fn a_four_argument_site_records_the_callback_bearing_count() {
    // Transcribed the same way as `vanilla_call_site`, with two more
    // arguments between the args array and the count: the scope object and
    // the response-callback name. Emission order is the observed
    // right-to-left one (last call argument first).
    let actions = vec![
        push(vec![Value::Str(text("onLoadDLCResponse"))]),
        push(vec![Value::Str(text("this"))]),
        Action::GetVariable,
        push(vec![Value::Int(0)]),
        Action::InitArray,
        push(vec![
            Value::Str(text("LoadDLC")),
            Value::Int(4),
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
    ];
    let found = scan_block(&assemble(&actions), 15, &[]);
    assert_eq!(
        found.methods.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["LoadDLC"]
    );
    assert_eq!(
        found.arg_counts.get("LoadDLC"),
        Some(&(4, 4)),
        "the callback-bearing arity must survive into the inventory"
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

    // #3103 — this now asserts the same completeness #2966's Fallout 4 sweep
    // does, because the catalog has been regenerated from this sweep's own
    // result. The 68 entries it used to be short are merged in.
    //
    // #4720 retired the blocker this assertion used to record ("SkyUI's
    // fourth-argument rule does not transfer — every vanilla call site passes
    // exactly two arguments"): the scanner now carries each site's argument
    // count, and the corpus measurably disagrees with the old premise — the
    // request-typed methods are called with four arguments (scope + response
    // callback), the command-typed ones with two, and no name mixes arities.
    // Two entries whose sites measured four arguments (`LoadDLC`,
    // `RequestLoadingText`) were promoted out of #3773's name-prefix bucket
    // into `Measured` requests; the rest keep `HeuristicNamePrefix`, and
    // `skyrim_catalog_provenance_split_matches_the_3103_sweep` pins the 76/66
    // split so neither half drifts.
    assert!(
        uncataloged.is_empty(),
        "the shipped Skyrim menus call {} host methods absent from \
         SKYRIM_SKYUI_METHODS: {uncataloged:?}",
        uncataloged.len(),
    );

    // #4720 — the fourth-argument rule, pinned against the corpus for every
    // entry the sweep can see: a four-argument site expects a `respond` and
    // must be typed `Request`; a two-argument site must be `Command`. A
    // counterexample is either a misclassified catalog entry (the diagnostic
    // gap the issue was filed for) or a corpus change that re-opens the
    // question — both want to fail loudly here.
    //
    // #5274 — the scanner records (min, max) per name, so a MIXED-ARITY
    // name is detectable for the first time: min < max breaks the rule in
    // both directions at once and fails here instead of silently collapsing
    // onto the max.
    let mut four_arg = 0usize;
    let mut two_arg = 0usize;
    for (name, (min, max)) in &found.arg_counts {
        let method = catalog
            .find(name)
            .unwrap_or_else(|| panic!("{name}: completeness asserted above"));
        assert!(
            min == max,
            "{name}: MIXED call arity ({min}..{max}) — the fourth-argument \
             rule cannot type one name at two arities (#5274)"
        );
        assert!(
            *max == 2 || *max == 4,
            "{name}: unexpected {max}-argument call arity"
        );
        let expected = if *max == 4 {
            four_arg += 1;
            crate::ScaleformHostMethodKind::Request
        } else {
            two_arg += 1;
            crate::ScaleformHostMethodKind::Command
        };
        assert_eq!(
            method.kind, expected,
            "{name}: catalog types it {:?} but its call site passes {max} \
             arguments — the fourth-argument rule says {expected:?}",
            method.kind,
        );
    }
    eprintln!(
        "  arities: {four_arg} four-argument (request) methods, {two_arg} \
         two-argument (command) methods"
    );
    assert!(
        four_arg >= 16,
        "the corpus measures 16 request-typed methods called with four \
         arguments (every cataloged request, #5274 re-derived: command 62 / \
         request 14 / sweep-command 64 / sweep-request 2); only {four_arg} \
         resolved — the scanner is reading less than it did"
    );
    assert!(
        found.methods.len() >= 141,
        "the sweep resolved {} distinct host methods, below the 141 measured \
         on the vanilla corpus — the scanner is reading less than it did",
        found.methods.len(),
    );
}
