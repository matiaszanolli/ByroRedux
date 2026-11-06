//! What a system acquires, read from a syntax tree (#6685).
//!
//! `system_access_declaration_tests` proves every system's declared `Access`
//! covers the storages and resources its body takes. It used to find those by
//! searching function text for `::<` turbofish spellings, which meant a
//! needle assembled at run time (so the test's own file would not match
//! it), a by-name special case for `PlayerEntity::get` (#6362), and a table
//! kept inside a function body so the scan could see it (#6361).
//!
//! This parses each source with `syn` and visits method calls and paths, so
//! comments and string literals cannot match, a helper's acquisitions are
//! followed through the call, and the function-pointer spelling
//! (`remove_component::<T>,` passed as an argument) is a path like any other.
//!
//! Test-only, like everything that reads source text here: `syn` is a
//! dev-dependency of the binary crate, so no release build compiles it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::rc::Rc;

use syn::buffer::{Cursor, TokenBuffer};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{
    AngleBracketedGenericArguments, Attribute, Block, Expr, File, GenericArgument, Generics,
    ImplItemFn, ItemFn, ItemImpl, ItemMod, ItemTrait, Macro, Meta, Token, TraitItemFn, Type,
};

/// How a type is acquired. A write subsumes a read of the same type, so the
/// strongest mode seen is the one recorded (`Read < Write`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Mode {
    Read,
    Write,
}

/// Type name (last path segment) → strongest mode it is acquired in.
pub(crate) type Acquired = BTreeMap<String, Mode>;

/// The calls that take a storage or resource lock, by the turbofish they
/// carry. `get` / `get_mut` / `has` are the `world.get::<T>` forms (#4994);
/// `remove_component` / `insert_component` take the storage through a generic
/// `query_mut::<T>` the scan cannot resolve, so their call sites are the
/// acquisition (#4821).
const ACQUIRE_FORMS: &[(&str, Mode)] = &[
    ("query", Mode::Read),
    ("try_query", Mode::Read),
    ("resource", Mode::Read),
    ("try_resource", Mode::Read),
    ("get", Mode::Read),
    ("has", Mode::Read),
    ("query_mut", Mode::Write),
    ("try_query_mut", Mode::Write),
    ("resource_mut", Mode::Write),
    ("try_resource_mut", Mode::Write),
    ("get_mut", Mode::Write),
    ("remove_component", Mode::Write),
    ("insert_component", Mode::Write),
    // #6707 — `fragment/effects/deferred.rs`'s `drain_into_resource::<R, _>(..)` takes
    // `try_resource_mut::<R>()` inside a generic helper, where the type is a parameter
    // the scan skips, so the call site is the acquisition.
    ("drain_into_resource", Mode::Write),
];

/// The resource whose one read is a plain call, `PlayerEntity::get(world)`,
/// with no turbofish (#6362). Matched on whole path segments, so a longer name
/// that merely ends in it (`PapyrusPlayerEntity::get`) is a different resource.
const PLAYER_ENTITY: &str = "PlayerEntity";

/// Hops followed from the entry function through same-file callees.
const MAX_DEPTH: usize = 3;

fn mode_of(name: &str) -> Option<Mode> {
    ACQUIRE_FORMS
        .iter()
        .find(|(form, _)| *form == name)
        .map(|(_, mode)| *mode)
}

// ---- parsing --------------------------------------------------------------

thread_local! {
    /// Parsed sources, keyed by the `&'static str`'s address and length. Several
    /// systems list the same file, and a parse of a large one is not free.
    static PARSED: RefCell<HashMap<(usize, usize), Rc<File>>> = RefCell::new(HashMap::new());
}

/// Parse `src` (a whole file, or several concatenated — the `SOURCES` lists
/// join files with `concat!`).
///
/// A concatenation carries each file's inner doc comments and attributes in the
/// middle of the text, where Rust does not allow them, so those lines are
/// dropped first. They are comments or lint levels; no acquisition is spelled
/// there.
pub(crate) fn parse(src: &'static str) -> Rc<File> {
    let key = (src.as_ptr() as usize, src.len());
    if let Some(hit) = PARSED.with(|cache| cache.borrow().get(&key).cloned()) {
        return hit;
    }
    let text: String = src
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !(line.starts_with("//!") || line.starts_with("#!["))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let file = syn::parse_file(&text)
        .unwrap_or_else(|error| panic!("the acquisition scan could not parse a source: {error}"));
    let file = Rc::new(file);
    PARSED.with(|cache| cache.borrow_mut().insert(key, file.clone()));
    file
}

/// Items that only exist under test. The old scan cut the text at the first
/// `#[cfg(test)] mod`; here each such item is skipped wherever it sits.
fn test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        if !attr.path().is_ident("cfg") {
            return false;
        }
        let Meta::List(list) = &attr.meta else {
            return false;
        };
        let text = list.tokens.to_string();
        let words: Vec<&str> = text
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| !word.is_empty())
            .collect();
        words.contains(&"test") && !words.contains(&"not")
    })
}

fn type_params(generics: &Generics) -> impl Iterator<Item = String> + '_ {
    generics.type_params().map(|param| param.ident.to_string())
}

// ---- function index -------------------------------------------------------

struct FnDef<'a> {
    block: &'a Block,
    /// The function's own and enclosing impl/trait type parameters. A turbofish
    /// naming one is the helper's parameter, not a type to declare.
    generics: HashSet<String>,
}

#[derive(Default)]
struct Index<'a> {
    fns: HashMap<String, Vec<FnDef<'a>>>,
    enclosing: Vec<HashSet<String>>,
}

impl<'a> Index<'a> {
    fn build(file: &'a File) -> Self {
        Self::build_all(&[file])
    }

    /// An index over several files, so a declaration can be followed into a
    /// subsystem's own file (`Access::with`, #6701).
    fn build_all(files: &[&'a File]) -> Self {
        let mut index = Self::default();
        for file in files {
            index.visit_file(file);
        }
        index
    }

    fn register(&mut self, name: String, block: &'a Block, generics: &Generics) {
        let mut all: HashSet<String> = self.enclosing.iter().flatten().cloned().collect();
        all.extend(type_params(generics));
        self.fns.entry(name).or_default().push(FnDef {
            block,
            generics: all,
        });
    }
}

impl<'ast> Visit<'ast> for Index<'ast> {
    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        if !test_only(&module.attrs) {
            visit::visit_item_mod(self, module);
        }
    }

    fn visit_item_fn(&mut self, function: &'ast ItemFn) {
        if test_only(&function.attrs) {
            return;
        }
        self.register(
            function.sig.ident.to_string(),
            &function.block,
            &function.sig.generics,
        );
        visit::visit_item_fn(self, function);
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        if test_only(&item.attrs) {
            return;
        }
        self.enclosing.push(type_params(&item.generics).collect());
        visit::visit_item_impl(self, item);
        self.enclosing.pop();
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        if test_only(&item.attrs) {
            return;
        }
        self.enclosing.push(type_params(&item.generics).collect());
        visit::visit_item_trait(self, item);
        self.enclosing.pop();
    }

    fn visit_impl_item_fn(&mut self, function: &'ast ImplItemFn) {
        if test_only(&function.attrs) {
            return;
        }
        self.register(
            function.sig.ident.to_string(),
            &function.block,
            &function.sig.generics,
        );
        visit::visit_impl_item_fn(self, function);
    }

    fn visit_trait_item_fn(&mut self, function: &'ast TraitItemFn) {
        if test_only(&function.attrs) {
            return;
        }
        if let Some(block) = &function.default {
            self.register(
                function.sig.ident.to_string(),
                block,
                &function.sig.generics,
            );
        }
        visit::visit_trait_item_fn(self, function);
    }
}

// ---- one body -------------------------------------------------------------

struct Collector<'g> {
    generics: &'g HashSet<String>,
    acquired: Acquired,
    callees: BTreeSet<String>,
}

impl<'g> Collector<'g> {
    fn new(generics: &'g HashSet<String>) -> Self {
        Self {
            generics,
            acquired: Acquired::new(),
            callees: BTreeSet::new(),
        }
    }

    fn add(&mut self, name: String, mode: Mode) {
        let slot = self.acquired.entry(name).or_insert(mode);
        *slot = (*slot).max(mode);
    }

    /// `name::<T>` or `.name::<T>()` where `name` is an acquisition form.
    fn turbofish(&mut self, name: &str, args: &AngleBracketedGenericArguments) {
        let Some(mode) = mode_of(name) else {
            return;
        };
        let Some(GenericArgument::Type(Type::Path(ty))) = args.args.first() else {
            return;
        };
        if ty.qself.is_some() {
            return;
        }
        let Some(segment) = ty.path.segments.last() else {
            return;
        };
        let ident = segment.ident.to_string();
        if !self.generics.contains(&ident) {
            self.add(ident, mode);
        }
    }

    /// Walk the tokens of a macro whose arguments are not a comma list of
    /// expressions (`matches!` patterns, `info!(target: ..)`, a `macro_rules!`
    /// body). Looks for the same two spellings as the visitor does:
    /// `form :: < Type` and `PlayerEntity :: get`.
    fn tokens(&mut self, mut cursor: Cursor<'_>) {
        while !cursor.eof() {
            if let Some((inside, _, _, after)) = cursor.any_group() {
                self.tokens(inside);
                cursor = after;
                continue;
            }
            let Some((ident, next)) = cursor.ident() else {
                cursor = cursor.token_tree().map_or(cursor, |(_, after)| after);
                if cursor.eof() {
                    break;
                }
                continue;
            };
            let name = ident.to_string();
            if let Some(after_colons) = path_sep(next) {
                if name == PLAYER_ENTITY {
                    if let Some((method, _)) = after_colons.ident() {
                        if method == "get" {
                            self.add(PLAYER_ENTITY.to_owned(), Mode::Read);
                        }
                    }
                }
                if let (Some(mode), Some(after_lt)) = (mode_of(&name), punct(after_colons, '<')) {
                    // `$Comp` is a macro metavariable, not a type.
                    if let Some(ty) = last_path_ident(after_lt) {
                        if !self.generics.contains(&ty) {
                            self.add(ty, mode);
                        }
                    }
                }
            }
            cursor = next;
        }
    }
}

/// `:` `:` at the cursor → the cursor after both.
fn path_sep(cursor: Cursor<'_>) -> Option<Cursor<'_>> {
    punct(punct(cursor, ':')?, ':')
}

fn punct(cursor: Cursor<'_>, wanted: char) -> Option<Cursor<'_>> {
    let (found, next) = cursor.punct()?;
    (found.as_char() == wanted).then_some(next)
}

/// The final identifier of the `a::b::C` path at the cursor, provided the path
/// runs unbroken up to a `>` or `,` — anything else (`&T`, `(A, B)`, `$Comp`)
/// is not a plain type name.
fn last_path_ident(mut cursor: Cursor<'_>) -> Option<String> {
    loop {
        let (ident, next) = cursor.ident()?;
        if let Some(after_colons) = path_sep(next) {
            cursor = after_colons;
            continue;
        }
        let closes = punct(next, '>').is_some() || punct(next, ',').is_some();
        return closes.then(|| ident.to_string());
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        if let Some(args) = &call.turbofish {
            self.turbofish(&name, args);
        }
        self.callees.insert(name);
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*call.func {
            if let Some(last) = path.path.segments.last() {
                self.callees.insert(last.ident.to_string());
            }
        }
        visit::visit_expr_call(self, call);
    }

    /// Every path expression — a call's target, a function pointer passed as an
    /// argument, a bare `PlayerEntity::get` — is looked at the same way.
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        let segments = &path.path.segments;
        if let Some(last) = segments.last() {
            if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                self.turbofish(&last.ident.to_string(), args);
            }
            if last.ident == "get"
                && segments.len() >= 2
                && segments[segments.len() - 2].ident == PLAYER_ENTITY
            {
                self.add(PLAYER_ENTITY.to_owned(), Mode::Read);
            }
        }
        visit::visit_expr_path(self, path);
    }

    /// A macro's arguments are not part of the tree. The common shape — a comma
    /// list of expressions, as in `assert!` / `vec!` / `format!` — parses as
    /// one and is visited normally; the rest are read token by token.
    fn visit_macro(&mut self, mac: &'ast Macro) {
        match mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
            Ok(exprs) => {
                for expr in &exprs {
                    self.visit_expr(expr);
                }
            }
            Err(_) => {
                let buffer = TokenBuffer::new2(mac.tokens.clone());
                self.tokens(buffer.begin());
            }
        }
        visit::visit_macro(self, mac);
    }
}

// ---- entry points ---------------------------------------------------------

/// Everything `entry` acquires, plus everything of the functions it calls that
/// are defined in the same file, to [`MAX_DEPTH`] hops. Breadth-first, so a
/// function reachable by two routes is read at the shorter one.
///
/// A hop into a different file is not followed: the caller lists it as its own
/// `(source, entry)` pair, because following bare names across files walks
/// into unrelated code.
pub(crate) fn acquired(file: &File, entry: &str) -> Acquired {
    let index = Index::build(file);
    let mut out = Acquired::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut pending: VecDeque<(String, usize)> = VecDeque::from([(entry.to_owned(), 0)]);
    while let Some((name, depth)) = pending.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(definitions) = index.fns.get(&name) else {
            continue;
        };
        for definition in definitions {
            let mut collector = Collector::new(&definition.generics);
            collector.visit_block(definition.block);
            for (ty, mode) in collector.acquired {
                let slot = out.entry(ty).or_insert(mode);
                *slot = (*slot).max(mode);
            }
            if depth < MAX_DEPTH {
                for callee in collector.callees {
                    if callee != name && index.fns.contains_key(&callee) {
                        pending.push_back((callee, depth + 1));
                    }
                }
            }
        }
    }
    out
}

/// What one `add_*_with_access` registration declares.
#[derive(Debug, Default)]
pub(crate) struct Declared {
    /// Every type named in a `reads` / `writes` / `*_resource` call.
    pub named: BTreeSet<String>,
    /// The subset named by a `writes` / `writes_resource` call.
    pub written: BTreeSet<String>,
}

impl Declared {
    /// Whether this declaration covers an acquisition in `mode`.
    pub(crate) fn covers(&self, ty: &str, mode: Mode) -> bool {
        self.named.contains(ty) && (mode == Mode::Read || self.written.contains(ty))
    }
}

const ACCESS_CALLS: &[(&str, bool)] = &[
    ("reads", false),
    ("reads_resource", false),
    ("writes", true),
    ("writes_resource", true),
];

/// The `Access` combinator that delegates a declaration to a function.
const ACCESS_WITH: &str = "with";

/// Collects the `reads` / `writes` calls of an `Access` expression, and of the
/// helper functions it passes the access through (`bridged_script_getter_reads(
/// Access::new())` declares in its own body, not in the registration).
struct AccessChain<'a, 'ast> {
    declared: Declared,
    index: &'a Index<'ast>,
    followed: HashSet<String>,
}

impl<'a, 'ast> AccessChain<'a, 'ast> {
    /// Read the body of every function named `name`, once.
    fn follow(&mut self, name: String) {
        let index = self.index;
        if let Some(definitions) = index.fns.get(&name) {
            if self.followed.insert(name) {
                for definition in definitions {
                    self.visit_block(definition.block);
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for AccessChain<'_, 'ast> {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        if let (Some((_, writes)), Some(args)) = (
            ACCESS_CALLS.iter().find(|(form, _)| *form == name),
            &call.turbofish,
        ) {
            if let Some(GenericArgument::Type(Type::Path(ty))) = args.args.first() {
                if let Some(last) = ty.path.segments.last() {
                    let ident = last.ident.to_string();
                    if *writes {
                        self.declared.written.insert(ident.clone());
                    }
                    self.declared.named.insert(ident);
                }
            }
        }
        // #6701 — `Access::with(subsystem::access)` hands the access to a
        // function the subsystem owns. The function is passed, not called, so
        // `visit_expr_call` never sees it; follow the path argument.
        if name == ACCESS_WITH {
            for arg in &call.args {
                if let Expr::Path(path) = arg {
                    if let Some(last) = path.path.segments.last() {
                        self.follow(last.ident.to_string());
                    }
                }
            }
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*call.func {
            if let Some(last) = path.path.segments.last() {
                self.follow(last.ident.to_string());
            }
        }
        visit::visit_expr_call(self, call);
    }
}

/// The name a registration's second argument registers: `path::system` or
/// `make_system()` → `system` / `make_system`.
fn registered_name(target: &Expr) -> Option<String> {
    let path = match target {
        Expr::Path(path) => &path.path,
        Expr::Call(call) => match &*call.func {
            Expr::Path(path) => &path.path,
            _ => return None,
        },
        _ => return None,
    };
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

/// Every `scheduler.add_*_with_access(stage, system, access)` call in `file`.
struct Registrations<'ast> {
    calls: Vec<(&'static str, &'ast syn::ExprMethodCall)>,
}

impl<'ast> Visit<'ast> for Registrations<'ast> {
    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        if !test_only(&module.attrs) {
            visit::visit_item_mod(self, module);
        }
    }

    fn visit_item_fn(&mut self, function: &'ast ItemFn) {
        if !test_only(&function.attrs) {
            visit::visit_item_fn(self, function);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        // `ExprMethodCall::method` is an `Ident`, which cannot be borrowed as
        // `&str` for `'ast`; the registration kind is one of two fixed names.
        let kind = match call.method.to_string().as_str() {
            "add_to_with_access" => Some("add_to_with_access"),
            "add_exclusive_with_access" => Some("add_exclusive_with_access"),
            _ => None,
        };
        if let Some(kind) = kind {
            self.calls.push((kind, call));
        }
        visit::visit_expr_method_call(self, call);
    }
}

fn registrations(file: &File) -> Vec<(&'static str, &syn::ExprMethodCall)> {
    let mut found = Registrations { calls: Vec::new() };
    found.visit_file(file);
    found.calls
}

/// How many `add_to_with_access` (parallel) registrations `file` makes.
pub(crate) fn parallel_registration_count(file: &File) -> usize {
    registrations(file)
        .iter()
        .filter(|(kind, _)| *kind == "add_to_with_access")
        .count()
}

/// The declared access of the registration that registers `system`.
///
/// Anchored on the call's registration *target* (its second argument), not on
/// the first mention of the name: a system's block can name another system in
/// prose, and anchoring on that compared one system's acquisitions against
/// another's declaration (#4064).
pub(crate) fn declaration(file: &File, system: &str) -> Declared {
    declaration_with_delegates(file, &[], system)
}

/// [`declaration`], where the registration may delegate part of its `Access`
/// to functions defined in `delegates` (`Access::with(crime_gold::access)`,
/// #6701). The registration is still found in `file` alone.
pub(crate) fn declaration_with_delegates(
    file: &File,
    delegates: &[&File],
    system: &str,
) -> Declared {
    let mut files = vec![file];
    files.extend_from_slice(delegates);
    let index = Index::build_all(&files);
    for (_, call) in registrations(file) {
        let (Some(target), Some(access)) = (call.args.iter().nth(1), call.args.iter().last())
        else {
            continue;
        };
        if registered_name(target).as_deref() == Some(system) {
            let mut chain = AccessChain {
                declared: Declared::default(),
                index: &index,
                followed: HashSet::new(),
            };
            chain.visit_expr(access);
            return chain.declared;
        }
    }
    panic!("{system}: no `add_*_with_access` registration found in boot/schedule/");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(src: &str, entry: &str) -> Acquired {
        let file = syn::parse_file(src).expect("fixture parses");
        acquired(&file, entry)
    }

    fn modes(found: &Acquired) -> Vec<(&str, Mode)> {
        found
            .iter()
            .map(|(ty, mode)| (ty.as_str(), *mode))
            .collect()
    }

    #[test]
    fn direct_acquisitions_carry_their_mode() {
        let found = scan(
            "fn sys(world: &World) {
                let a = world.query::<crate::Foo>();
                let b = world.query_mut::<Bar>();
                let c = world.try_resource::<Baz>();
                let d = world.resource_mut::<Qux>();
                let e = world.get::<Quux>(entity);
                let f = world.get_mut::<Corge>(entity);
            }",
            "sys",
        );
        assert_eq!(
            modes(&found),
            [
                ("Bar", Mode::Write),
                ("Baz", Mode::Read),
                ("Corge", Mode::Write),
                ("Foo", Mode::Read),
                ("Quux", Mode::Read),
                ("Qux", Mode::Write),
            ]
        );
    }

    #[test]
    fn a_write_subsumes_a_read_of_the_same_type() {
        let found = scan(
            "fn sys(world: &World) { world.query::<Foo>(); world.query_mut::<Foo>(); world.query::<Foo>(); }",
            "sys",
        );
        assert_eq!(modes(&found), [("Foo", Mode::Write)]);
    }

    /// The acquisition is two calls away, in a helper and then a method on a
    /// type — the text scan reached it only because both names were lowercase
    /// and in the same file; here it is the call itself that is followed.
    #[test]
    fn an_acquisition_is_found_through_helper_calls() {
        let found = scan(
            "fn sys(world: &World) { helper(world); }
             fn helper(world: &World) { Thing::step(world); }
             impl Thing { fn step(world: &World) { world.resource_mut::<Deep>(); } }
             fn unrelated(world: &World) { world.query::<NotCalled>(); }",
            "sys",
        );
        assert_eq!(modes(&found), [("Deep", Mode::Write)]);
    }

    #[test]
    fn a_method_call_is_followed_like_a_function_call() {
        let found = scan(
            "fn sys(this: &Thing, world: &World) { this.step(world); }
             impl Thing { fn step(&self, world: &World) { world.query::<ViaMethod>(); } }",
            "sys",
        );
        assert_eq!(modes(&found), [("ViaMethod", Mode::Read)]);
    }

    #[test]
    fn helper_hops_stop_at_the_depth_limit() {
        let found = scan(
            "fn a(w: &World) { b(w); }
             fn b(w: &World) { c(w); }
             fn c(w: &World) { d(w); }
             fn d(w: &World) { e(w); }
             fn e(w: &World) { w.query::<TooDeep>(); }
             fn d2(w: &World) { w.query::<Reached>(); }",
            "a",
        );
        // a -> b -> c -> d is three hops; `e` is the fourth.
        assert!(found.is_empty(), "{found:?}");
        let found = scan(
            "fn a(w: &World) { b(w); } fn b(w: &World) { w.query::<Reached>(); }",
            "a",
        );
        assert_eq!(modes(&found), [("Reached", Mode::Read)]);
    }

    /// `remove_component::<T>,` as a bare path — handed to something that calls
    /// it — is the acquisition (#4821), with no call at all in the body.
    #[test]
    fn a_function_pointer_spelling_is_an_acquisition() {
        let found = scan(
            "fn sys(world: &World) {
                clear_all(world, &[remove_component::<AmbientBehavior>, remove_component::<AmbientState>]);
                apply(world, insert_component::<Marker>,);
            }",
            "sys",
        );
        assert_eq!(
            modes(&found),
            [
                ("AmbientBehavior", Mode::Write),
                ("AmbientState", Mode::Write),
                ("Marker", Mode::Write),
            ]
        );
    }

    #[test]
    fn a_player_entity_read_is_a_call_with_no_turbofish() {
        let found = scan(
            "fn sys(world: &World) { let me = crate::systems::PlayerEntity::get(world); }",
            "sys",
        );
        assert_eq!(modes(&found), [(PLAYER_ENTITY, Mode::Read)]);
        let found = scan(
            "fn sys(world: &World) { PapyrusPlayerEntity::get(world); }",
            "sys",
        );
        assert!(
            found.is_empty(),
            "a longer name is a different resource: {found:?}"
        );
    }

    #[test]
    fn comments_and_string_literals_do_not_match() {
        let found = scan(
            r#"fn sys(world: &World) {
                // world.query::<InComment>()
                /* world.query_mut::<InBlockComment>() */
                let text = "world.resource_mut::<InString>()";
                let raw = r"PlayerEntity::get(";
            }"#,
            "sys",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_helpers_own_type_parameter_is_not_a_type_to_declare() {
        let found = scan(
            "fn sys(world: &World) { teardown::<Real>(world); }
             fn teardown<T: Component>(world: &World) { world.query_mut::<T>(); world.query::<Fixed>(); }",
            "sys",
        );
        assert_eq!(modes(&found), [("Fixed", Mode::Read)]);
    }

    #[test]
    fn items_under_cfg_test_are_not_production() {
        let found = scan(
            "fn sys(world: &World) { helper(world); }
             fn helper(world: &World) { world.query::<Production>(); }
             #[cfg(test)]
             fn helper(world: &World) { world.query::<TestOnly>(); }
             #[cfg(test)]
             mod tests { fn sys(world: &World) { world.query::<AlsoTestOnly>(); } }",
            "sys",
        );
        assert_eq!(modes(&found), [("Production", Mode::Read)]);
    }

    #[test]
    fn acquisitions_inside_macro_arguments_are_seen() {
        // A comma list of expressions: parsed.
        let found = scan(
            "fn sys(world: &World) { assert!(world.get::<InAssert>(e).is_some(), \"msg\"); }",
            "sys",
        );
        assert_eq!(modes(&found), [("InAssert", Mode::Read)]);
        // Not a comma list of expressions: read token by token.
        let found = scan(
            "fn sys(world: &World) { log::info!(target: \"x\", \"{}\", world.resource::<InLog>().n); }",
            "sys",
        );
        assert_eq!(modes(&found), [("InLog", Mode::Read)]);
        let found = scan(
            "fn sys(world: &World) { if matches!(world.query::<InMatches>(), Some(_) if true) {} }",
            "sys",
        );
        assert_eq!(modes(&found), [("InMatches", Mode::Read)]);
    }

    #[test]
    fn a_macro_metavariable_is_not_a_type() {
        let found = scan(
            "fn sys(world: &World) {
                macro_rules! each { ($Comp:ty) => { world.query_mut::<$Comp>(); world.query::<Literal>(); } }
                each!(Foo);
             }",
            "sys",
        );
        assert_eq!(modes(&found), [("Literal", Mode::Read)]);
    }

    #[test]
    fn a_concatenated_source_parses_with_its_inner_docs_dropped() {
        let src: &'static str = "//! first file\n#![allow(dead_code)]\nfn a(w: &World) { w.query::<A>(); }\n\
                                 //! second file\n#![allow(unused)]\nfn b(w: &World) { w.query::<B>(); }\n";
        let file = parse(src);
        assert_eq!(modes(&acquired(&file, "a")), [("A", Mode::Read)]);
        assert_eq!(modes(&acquired(&file, "b")), [("B", Mode::Read)]);
    }

    #[test]
    fn a_declaration_is_read_from_the_registration_that_registers_the_system() {
        let file = syn::parse_file(
            "fn build(scheduler: &mut Scheduler) {
                // `first_system` is named in prose here, and is not what registers.
                scheduler.add_to_with_access(
                    Stage::Early,
                    crate::systems::other_system,
                    Access::new().reads::<Wrong>().writes_resource::<WrongRes>(),
                );
                scheduler.add_to_with_access(
                    Stage::Early,
                    make_system(),
                    Access::new()
                        .reads::<a::b::Read>()
                        .writes::<Written>()
                        .reads_resource::<Res>(),
                );
                scheduler.add_exclusive_with_access(Stage::Late, exclusive_one, Access::new().writes_resource::<Ex>());
            }
            #[cfg(test)]
            mod tests { fn t(s: &mut Scheduler) { s.add_to_with_access(Stage::Early, make_system(), Access::new().reads::<TestOnly>()); } }",
        )
        .expect("fixture parses");
        assert_eq!(parallel_registration_count(&file), 2);

        let declared = declaration(&file, "make_system");
        assert!(declared.covers("Read", Mode::Read));
        assert!(
            !declared.covers("Read", Mode::Write),
            "a read declaration does not cover a write"
        );
        assert!(declared.covers("Written", Mode::Write));
        assert!(declared.covers("Res", Mode::Read));
        assert!(
            !declared.covers("Wrong", Mode::Read),
            "another system's declaration"
        );
        assert!(
            !declared.covers("TestOnly", Mode::Read),
            "a test module's registration"
        );

        assert!(declaration(&file, "exclusive_one").covers("Ex", Mode::Write));
    }

    /// #6707 — a flush routed through the generic helper still acquires its resource at the call
    /// site, and the helper's own generic `R` is not reported as a type.
    #[test]
    fn a_flush_through_the_drain_helper_acquires_the_resource_it_names() {
        let found = scan(
            "impl Deferred {
                fn flush_events(&mut self, world: &World) {
                    drain_into_resource::<super::EventRegistrations, _>(world, &mut self.events, |r, e| r.add(e));
                }
            }
            fn drain_into_resource<R: Resource, T>(world: &World, queue: &mut Vec<T>, mut apply: impl FnMut(&mut R, T)) {
                if let Some(mut resource) = world.try_resource_mut::<R>() { let _ = &mut resource; }
            }",
            "flush_events",
        );
        assert_eq!(modes(&found), [("EventRegistrations", Mode::Write)]);
    }

    /// `bridged_script_getter_reads(Access::new())` declares in its own body.
    #[test]
    fn a_declaration_made_through_a_helper_is_read() {
        let file = syn::parse_file(
            "fn build(scheduler: &mut Scheduler) {
                scheduler.add_exclusive_with_access(
                    Stage::Late,
                    helped_system,
                    outer(inner(Access::new())).reads::<Direct>(),
                );
            }
            fn outer(access: Access) -> Access { access.writes::<ViaOuter>() }
            fn inner(access: Access) -> Access { access.reads_resource::<ViaInner>() }
            fn unrelated(access: Access) -> Access { access.writes::<NotCalled>() }",
        )
        .expect("fixture parses");
        let declared = declaration(&file, "helped_system");
        assert!(declared.covers("Direct", Mode::Read));
        assert!(declared.covers("ViaOuter", Mode::Write));
        assert!(declared.covers("ViaInner", Mode::Read));
        assert!(!declared.covers("NotCalled", Mode::Read));
    }

    /// #6701 — `.with(subsystem::access)` passes the function, so the scan must
    /// follow a path argument, and only into the delegate files it was given.
    #[test]
    fn a_declaration_delegated_through_with_is_read_and_a_removal_is_seen() {
        let boot = syn::parse_file(
            "fn build(scheduler: &mut Scheduler) {
                scheduler.add_exclusive_with_access(
                    Stage::Late,
                    delegating_system,
                    Access::new().reads::<Direct>().with(crime::crime_access).with(helper),
                );
            }
            fn helper(access: Access) -> Access { access.writes::<ViaHelper>() }",
        )
        .expect("fixture parses");
        let subsystem = syn::parse_file(
            "pub fn crime_access(access: Access) -> Access {
                access.reads_resource::<Gold>().writes_resource::<Gold>()
            }
            pub fn other_access(access: Access) -> Access { access.writes::<NotWith>() }",
        )
        .expect("fixture parses");
        let declared = declaration_with_delegates(&boot, &[&subsystem], "delegating_system");
        assert!(declared.covers("Direct", Mode::Read));
        assert!(declared.covers("Gold", Mode::Write), "delegated rows are read");
        assert!(declared.covers("ViaHelper", Mode::Write));
        assert!(
            !declared.covers("NotWith", Mode::Read),
            "a delegate function nobody passes to `.with` is not part of the row"
        );

        // The injected fault: the delegate loses its declaration.
        let removed = syn::parse_file(
            "pub fn crime_access(access: Access) -> Access { access }",
        )
        .expect("fixture parses");
        let declared = declaration_with_delegates(&boot, &[&removed], "delegating_system");
        assert!(
            !declared.covers("Gold", Mode::Write),
            "a declaration removed from the delegate must fail the guard"
        );
        // Without the delegate file at all, nothing is silently assumed.
        assert!(!declaration(&boot, "delegating_system").covers("Gold", Mode::Read));
    }

    #[test]
    #[should_panic(expected = "no `add_*_with_access` registration found")]
    fn a_system_with_no_registration_is_an_error() {
        let file = syn::parse_file("fn build() {}").expect("fixture parses");
        declaration(&file, "ghost_system");
    }
}
