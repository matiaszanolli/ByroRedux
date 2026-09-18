//! Parser for the Oblivion / FO3 / FNV "MenuXml" dialect.
//!
//! Bethesda's menu XML is *not* well-formed XML: vanilla files contain
//! overlapping comment spans (`<!--` … `-->` … `<!--` … `-->` where the
//! second `-->` lands inside `</text-->`), an `<onlynotif>` typo for
//! `onlyifnot`, and `<include src="prefabs\button_long.xml"/>` splices of
//! fragment files that have no root element. A strict parser rejects all
//! three. The source engine's tokenizer was lenient in exactly these
//! places, so this module is a small hand-rolled scanner with the same
//! leniency rather than a general XML library.
//!
//! Grammar (per the CS Wiki "Oblivion XML Reference"):
//!
//! ```text
//! menu   := '<menu name="...">' body '</menu>'
//! tile   := ('<rect'|'<image'|'<text'|'<nif'|'<3d'|'<template') body '</...>'
//! body   := (text | include | tile | trait)*
//! trait  := '<' name '>' body '</' name '>'      -- body is text OR ops
//! op     := '<' opname [src] [trait] '>' body '</' name '>'  -- same shape
//! include:= '<include src="file.xml"/>'
//! ```
//!
//! Entities (`&true;`, `&xbox;`, `&HUDMainMenu;`, …) are expanded by the
//! scanner before value parsing; see [`expand_entity`].

use std::collections::BTreeMap;
use std::collections::HashMap;

/// Trait values before per-frame evaluation: a literal number, a literal
/// string, or an operator chain resolved against live trait state.
#[derive(Debug, Clone, PartialEq)]
pub enum RawTrait {
    Num(f32),
    Str(String),
    Ops(Vec<Op>),
}

/// A single operator in a trait's expression chain. Evaluation is a fold:
/// each operator combines the accumulated "working value" with its
/// *argument* — inline literal, `src`/`trait` selection, or the value of
/// its child operator chain (CS Wiki "Operators": "operating with
/// awareness of its previous sibling's returned value").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Copy,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Min,
    Max,
    And,
    Or,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    OnlyIf,
    OnlyIfNot,
    Ceil,
    Floor,
    Abs,
    Not,
}

impl OpKind {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "copy" => Self::Copy,
            "add" => Self::Add,
            "sub" => Self::Sub,
            // `mult` is the wiki spelling; vanilla files use both.
            "mul" | "mult" => Self::Mul,
            "div" => Self::Div,
            "mod" => Self::Mod,
            "min" => Self::Min,
            "max" => Self::Max,
            "and" => Self::And,
            "or" => Self::Or,
            "eq" => Self::Eq,
            "neq" | "ne" => Self::Neq,
            "lt" => Self::Lt,
            "lte" => Self::Lte,
            "gt" => Self::Gt,
            "gte" | "ge" => Self::Gte,
            "onlyif" => Self::OnlyIf,
            // Vanilla `button_long.xml` carries an `onlynotif` typo for
            // `onlyifnot`; accepted as an alias rather than silently
            // dropped so third-party prefabs that copied the typo keep
            // evaluating.
            "onlyifnot" | "onlynotif" => Self::OnlyIfNot,
            "ceil" => Self::Ceil,
            "floor" | "trunc" => Self::Floor,
            "abs" => Self::Abs,
            "not" => Self::Not,
            _ => return None,
        })
    }
}

/// Where an operator's argument comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum OpArg {
    /// No argument at all (`<add/>`) — treated as 0 downstream.
    None,
    /// Inline text content, entity-expanded (`<add> 24 </add>`).
    Lit(Scalar),
    /// `src="screen()" trait="width"` style selection.
    Src {
        src: String,
        trait_name: Option<String>,
    },
    /// Nested operator chain evaluated as a parenthesised sub-expression.
    Children(Vec<Op>),
}

/// One operator: kind + where its argument comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Op {
    pub kind: OpKind,
    pub arg: OpArg,
}

/// A value as it flows through expressions or sits in a trait: Gamebryo's
/// menu system treats booleans as numbers (`&true;` == 2), and a handful
/// of traits (`string`, `filename`, `animation`, class names) are text.
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Num(f32),
    Str(String),
}

impl Scalar {
    pub fn as_num(&self) -> f32 {
        match self {
            Scalar::Num(n) => *n,
            // String→number coercion keeps `copy` of a numeric string
            // usable in arithmetic (vanilla stores counts as strings).
            Scalar::Str(s) => s.trim().parse().unwrap_or(0.0),
        }
    }
    /// Truthiness follows Gamebryo: anything non-zero is true.
    pub fn truthy(&self) -> bool {
        self.as_num() != 0.0
    }
}

/// Tile kinds the dialect distinguishes. `Template` tiles are prototypes
/// the engine clones via the menu API; they parse but never draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    Menu,
    Rect,
    Image,
    Text,
    Nif,
    Template,
}

impl TileKind {
    fn from_element(name: &str) -> Option<Self> {
        Some(match name {
            "menu" => Self::Menu,
            "rect" | "window" => Self::Rect,
            "image" => Self::Image,
            "text" => Self::Text,
            "nif" | "3d" => Self::Nif,
            "template" => Self::Template,
            _ => return None,
        })
    }
}

/// One tile in the document arena.
#[derive(Debug, Clone)]
pub struct Tile {
    pub name: Option<String>,
    /// `<id>` / `<ID>` — the numeric handle the engine's menu code uses.
    pub id: Option<i32>,
    pub kind: TileKind,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Trait store, keys lowercased (the dialect is case-insensitive:
    /// vanilla mixes `<id>` and `<ID>`, `<mult>` and `<mul>`).
    pub traits: BTreeMap<String, RawTrait>,
}

/// A parsed menu document: the tile arena plus lookup indexes.
///
/// `tiles[0]` is always the root tile (`<menu>` for menus, the eponymous
/// `<rect>` for `strings.xml`).
#[derive(Debug, Clone)]
pub struct Document {
    pub menu_name: String,
    pub tiles: Vec<Tile>,
    /// First tile registered under each (lowercased) name. Names repeat
    /// in vanilla files (prefab splices); the first wins, matching the
    /// source engine's name→tile registration order.
    pub name_index: HashMap<String, usize>,
    /// tile index → `<id>`, for engine-side handles.
    pub id_index: HashMap<i32, usize>,
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

/// Entity table. Numeric entities expand to their value; class entities
/// (`&HUDMainMenu;`) expand to the bare string (they appear as the text of
/// `<class>` traits). Values per the CS Wiki trait/operator pages:
/// `&true;` is numerically 2, `&false;` 0; platform entities gate
/// Xbox-only art paths (dead on PC).
fn expand_entity(name: &str) -> Option<String> {
    let out = match name {
        "true" => "2".to_string(),
        "false" | "xenon" | "xbox" => "0".to_string(),
        "pc" => "2".to_string(),
        "left" => "0".to_string(),
        "center" => "1".to_string(),
        "right" => "2".to_string(),
        "scale" => "-1".to_string(),
        "xbuttona" | "xbutton_a" => "0".to_string(),
        "xbuttonb" | "xbutton_b" => "1".to_string(),
        "xbuttonx" | "xbutton_x" => "2".to_string(),
        "xbuttony" | "xbutton_y" => "3".to_string(),
        // Menu-class entities: `<class>` compares against the string, so
        // keep the bare identifier.
        other if other.contains(|c: char| c.is_ascii_uppercase()) => other.to_string(),
        _ => return None,
    };
    Some(out)
}

/// Replace `&name;` entities in `text`. Unknown entities expand to 0 with
/// a debug log — vanilla only uses the table above plus class names.
fn expand_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        match rest.find(';') {
            // Entity names in this dialect are short identifiers; a `;`
            // further than 64 bytes away is not an entity reference.
            Some(semi) if semi <= 64 => {
                let name = &rest[1..semi];
                match expand_entity(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        log::debug!("menuxml: unknown entity &{name}; treated as 0");
                        out.push('0');
                    }
                }
                rest = &rest[semi + 1..];
            }
            _ => {
                // Bare `&` with no terminator — keep it verbatim.
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parse expanded trait text into a literal scalar. Empty or unparseable
/// text becomes `Num(0.0)`; non-numeric text stays a string (class names,
/// region text).
fn literal_from_text(text: &str) -> Scalar {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Scalar::Num(0.0);
    }
    match trimmed.parse::<f32>() {
        Ok(n) => Scalar::Num(n),
        Err(_) => Scalar::Str(trimmed.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Scanner
// ---------------------------------------------------------------------------

/// Source for menu XML bytes: the top-level file plus `<include>`
/// prefabs, both resolved against the game's `menus\` tree.
pub trait MenuFileSource {
    /// `path` is archive-form (`menus\prefabs\button_long.xml`).
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>>;
}

/// Byte scanner over one XML unit.
struct Scanner<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    /// Skip whitespace and comments. Comments end at the *first* `-->`
    /// (no nesting) — this is what makes vanilla's overlapping comment
    /// spans parse the way the source engine read them.
    fn skip_trivia(&mut self) {
        loop {
            self.skip_ws();
            if self.src[self.pos..].starts_with("<!--") {
                match self.src[self.pos + 4..].find("-->") {
                    Some(end) => self.pos += 4 + end + 3,
                    None => self.pos = self.src.len(),
                }
            } else {
                return;
            }
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.src[self.pos..].chars().next() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                return;
            }
        }
    }

    /// Whether the next token is a close tag.
    fn at_close_tag(&mut self) -> bool {
        self.skip_trivia();
        self.src[self.pos..].starts_with("</")
    }

    /// Consume the next element, returning `(name, attrs, self_closing)`.
    fn take_element(&mut self) -> Option<(String, String, bool)> {
        self.skip_trivia();
        let rest = &self.src[self.pos..];
        if !rest.starts_with('<') || rest.starts_with("</") {
            return None;
        }
        let inner_end = rest.find('>')?;
        let inner = rest[1..inner_end].trim();
        self.pos += inner_end + 1;
        if let Some(trimmed) = inner.strip_suffix('/') {
            let (name, attrs) = split_name_attrs(trimmed);
            Some((name, attrs, true))
        } else {
            let (name, attrs) = split_name_attrs(inner);
            Some((name, attrs, false))
        }
    }

    /// Skip past the next close tag (any name — we do not verify names on
    /// close; the dialect never mistags and tolerance is cheaper).
    fn skip_close_tag(&mut self) {
        self.skip_trivia();
        if let Some(end) = self.src[self.pos..].find('>') {
            self.pos += end + 1;
        } else {
            self.pos = self.src.len();
        }
    }

    /// Raw text up to the next `<` (entities not yet expanded).
    fn take_text(&mut self) -> &'a str {
        let start = self.pos;
        match self.src[self.pos..].find('<') {
            Some(lt) => {
                self.pos += lt;
                &self.src[start..start + lt]
            }
            None => {
                self.pos = self.src.len();
                &self.src[start..]
            }
        }
    }
}

fn split_name_attrs(inner: &str) -> (String, String) {
    let trimmed = inner.trim();
    let name_len = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    (
        trimmed[..name_len].to_string(),
        trimmed[name_len..].to_string(),
    )
}

/// Pull `"value"` out of `name="value"` in an attribute string
/// (case-insensitive on the key). Advances past each *value*, not each
/// `=`, so an attribute following a quoted one still parses.
fn attr_value(attrs: &str, key: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim();
        let after = rest[eq + 1..].trim_start();
        let mut chars = after.chars();
        let quote = chars.next()?;
        let (value, end) = if quote == '"' || quote == '\'' {
            let body = &after[quote.len_utf8()..];
            let end = body.find(quote)?;
            (&body[..end], quote.len_utf8() + end + quote.len_utf8())
        } else {
            let word = after.split_whitespace().next()?;
            (word, word.len())
        };
        if name.eq_ignore_ascii_case(key) {
            return Some(value.to_string());
        }
        rest = &after[end..];
    }
    None
}

// ---------------------------------------------------------------------------
// Element bodies
// ---------------------------------------------------------------------------

/// The content of a trait or operator element: leading text and/or a
/// chain of operator child elements. Both trait elements and operator
/// elements have this shape, which is what makes op nesting recursive.
struct Body {
    text: String,
    ops: Vec<Op>,
}

impl Body {
    /// Collapse into a trait value: ops win when present, else the
    /// entity-expanded literal text.
    fn into_trait(self) -> RawTrait {
        if self.ops.is_empty() {
            match literal_from_text(&expand_entities(&self.text)) {
                Scalar::Num(n) => RawTrait::Num(n),
                Scalar::Str(s) => RawTrait::Str(s),
            }
        } else {
            RawTrait::Ops(self.ops)
        }
    }

    /// Collapse into an operator argument.
    fn into_arg(self) -> OpArg {
        if !self.ops.is_empty() {
            OpArg::Children(self.ops)
        } else {
            let trimmed = self.text.trim();
            if trimmed.is_empty() {
                OpArg::None
            } else {
                match literal_from_text(&expand_entities(trimmed)) {
                    Scalar::Num(n) => OpArg::Lit(Scalar::Num(n)),
                    Scalar::Str(s) => OpArg::Lit(Scalar::Str(s)),
                }
            }
        }
    }
}

/// Parse the body of a trait/op element: leading text, then op children
/// (each recursively a body), until the element's close tag.
///
/// `depth` guards against pathological nesting; `seen_includes` breaks
/// prefab include cycles (vanilla prefabs are acyclic, third-party ones
/// are not guaranteed to be).
fn parse_body(
    scanner: &mut Scanner,
    src: &mut dyn MenuFileSource,
    seen_includes: &mut Vec<String>,
    depth: usize,
) -> Body {
    let mut text = String::new();
    let mut ops = Vec::new();
    if depth > 64 {
        log::warn!("menuxml: expression nesting deeper than 64; truncating");
        return Body { text, ops };
    }
    loop {
        scanner.skip_trivia();
        if scanner.pos >= scanner.src.len() {
            break;
        }
        // Text accumulates only before the first op child.
        if ops.is_empty() {
            text.push_str(scanner.take_text());
        } else {
            let _ = scanner.take_text();
        }
        scanner.skip_trivia();
        if scanner.pos >= scanner.src.len() {
            break;
        }
        if scanner.at_close_tag() {
            scanner.skip_close_tag();
            break;
        }
        let Some((name, attrs, self_closing)) = scanner.take_element() else {
            // Stray `<` that did not parse as an element; drop one byte to
            // guarantee progress.
            scanner.pos += 1;
            continue;
        };
        let Some(kind) = OpKind::from_name(&name) else {
            log::debug!("menuxml: unknown op <{name}> ignored");
            if !self_closing {
                skip_element_subtree(scanner);
            }
            continue;
        };
        // `src`/`trait` attributes select the argument directly; body
        // text/children are the alternative forms.
        let attr_src = attr_value(&attrs, "src");
        let attr_trait = attr_value(&attrs, "trait");
        let arg = if let (Some(s), t) = (attr_src.clone(), attr_trait.clone()) {
            OpArg::Src {
                src: s,
                trait_name: t,
            }
        } else if self_closing {
            OpArg::None
        } else {
            parse_body(scanner, src, seen_includes, depth + 1).into_arg()
        };
        ops.push(Op { kind, arg });
    }
    Body { text, ops }
}

/// Skip an unknown element's entire subtree (depth-tracked, tolerant of
/// EOF).
fn skip_element_subtree(scanner: &mut Scanner) {
    let mut depth = 1usize;
    while depth > 0 {
        scanner.skip_trivia();
        let _ = scanner.take_text();
        scanner.skip_trivia();
        if scanner.pos >= scanner.src.len() {
            return;
        }
        if scanner.at_close_tag() {
            scanner.skip_close_tag();
            depth -= 1;
        } else {
            match scanner.take_element() {
                Some((_, _, self_closing)) => {
                    if !self_closing {
                        depth += 1;
                    }
                }
                None => {
                    scanner.pos += 1;
                }
            }
        }
    }
}

/// Result of parsing one tile element's content: traits and nested tiles.
struct ElementContent {
    traits: BTreeMap<String, RawTrait>,
    children: Vec<TileSeed>,
}

/// Parse the content of a tile/menu element until its close tag.
fn parse_element_content(
    scanner: &mut Scanner,
    src: &mut dyn MenuFileSource,
    seen_includes: &mut Vec<String>,
    depth: usize,
) -> ElementContent {
    let mut traits = BTreeMap::new();
    let mut children: Vec<TileSeed> = Vec::new();
    if depth > 48 {
        log::warn!("menuxml: tile nesting deeper than 48; truncating subtree");
        return ElementContent { traits, children };
    }
    loop {
        scanner.skip_trivia();
        if scanner.pos >= scanner.src.len() {
            break;
        }
        // Stray text between tiles (whitespace plus the `</text`-style
        // debris vanilla leaves after overlapping comment spans) — drop.
        let _ = scanner.take_text();
        scanner.skip_trivia();
        if scanner.pos >= scanner.src.len() {
            break;
        }
        if scanner.at_close_tag() {
            scanner.skip_close_tag();
            break;
        }
        let Some((name, attrs, self_closing)) = scanner.take_element() else {
            scanner.pos += 1;
            continue;
        };
        // `<include src="..."/>` splices a prefab fragment into this tile.
        if name == "include" {
            if let Some(path) = attr_value(&attrs, "src") {
                splice_include(
                    &path,
                    src,
                    seen_includes,
                    &mut traits,
                    &mut children,
                    depth,
                );
            }
            continue;
        }
        if TileKind::from_element(&name).is_some() {
            if let Some(seed) =
                parse_tile_element(&name, &attrs, self_closing, scanner, src, seen_includes, depth)
            {
                children.push(seed);
            }
            continue;
        }
        // Trait element: `<traitname> body </traitname>`.
        let key = name.to_lowercase();
        if self_closing {
            traits.insert(key, RawTrait::Num(0.0));
            continue;
        }
        let body = parse_body(scanner, src, seen_includes, depth + 1);
        traits.insert(key, body.into_trait());
    }
    ElementContent { traits, children }
}

/// Load and splice a prefab fragment (`<include src="prefabs\x.xml"/>`).
///
/// The fragment contributes its traits to the host tile and its children
/// become host children — this is how every vanilla button/scroll-bar
/// prefab is composed.
fn splice_include(
    path: &str,
    src: &mut dyn MenuFileSource,
    seen_includes: &mut Vec<String>,
    traits: &mut BTreeMap<String, RawTrait>,
    children: &mut Vec<TileSeed>,
    depth: usize,
) {
    let norm = path.replace('/', "\\").to_lowercase();
    if seen_includes.iter().any(|p| p == &norm) {
        log::warn!("menuxml: include cycle on '{path}' — splice skipped");
        return;
    }
    // Vanilla authors prefab includes relative to `menus\prefabs\`
    // (`<include src="button_long.xml"/>` from menus\dialog\*.xml).
    // Also accept a menus\-relative form and a raw archive path.
    let candidates = [
        format!("menus\\prefabs\\{path}"),
        format!("menus\\{path}"),
        path.to_string(),
    ];
    let bytes = candidates.iter().find_map(|p| src.menu_xml(p));
    let Some(bytes) = bytes else {
        log::warn!("menuxml: include '{path}' not found");
        return;
    };
    let Ok(text) = String::from_utf8(bytes) else {
        log::warn!("menuxml: include '{path}' is not UTF-8");
        return;
    };
    seen_includes.push(norm);
    let mut scanner = Scanner::new(&text);
    // A prefab fragment opens with a comment naming its intended host
    // shape (`<!-- image name="button_long" -->`); skip_trivia eats it
    // and the remainder parses as the host tile's own content.
    let content = parse_element_content(&mut scanner, src, seen_includes, depth);
    seen_includes.pop();
    for (k, v) in content.traits {
        traits.insert(k, v);
    }
    children.extend(content.children);
}

/// Partial tile: kind + name + traits + children, linked into the arena
/// by [`parse_document`].
struct TileSeed {
    name: Option<String>,
    id: Option<i32>,
    kind: TileKind,
    traits: BTreeMap<String, RawTrait>,
    children: Vec<TileSeed>,
}

/// Parse one tile element. The open tag has already been consumed by the
/// caller (`name`/`attrs`/`self_closing` come from it).
fn parse_tile_element(
    name: &str,
    attrs: &str,
    self_closing: bool,
    scanner: &mut Scanner,
    src: &mut dyn MenuFileSource,
    seen_includes: &mut Vec<String>,
    depth: usize,
) -> Option<TileSeed> {
    let kind = TileKind::from_element(name)?;
    let tile_name = attr_value(attrs, "name");
    if self_closing {
        return Some(TileSeed {
            name: tile_name,
            id: None,
            kind,
            traits: BTreeMap::new(),
            children: Vec::new(),
        });
    }
    let content = parse_element_content(scanner, src, seen_includes, depth + 1);
    // `<id>` may arrive before or after other traits; both spellings
    // appear in vanilla (`<id>` in HUD, `<ID>` in prefabs).
    let id = ["id"]
        .iter()
        .find_map(|k| content.traits.get(*k))
        .and_then(|t| match t {
            RawTrait::Num(n) => Some(*n as i32),
            _ => None,
        });
    Some(TileSeed {
        name: tile_name,
        id,
        kind,
        traits: content.traits,
        children: content.children,
    })
}

/// Parse a full menu document (e.g. `menus\main\hud_main_menu.xml`) or a
/// rootless fragment (`menus\strings.xml` roots a `<rect>`; prefabs have
/// no root at all and are parsed via [`splice_include`]'s content path).
pub fn parse_document(text: &str, src: &mut dyn MenuFileSource) -> Document {
    let mut scanner = Scanner::new(text);
    let mut includes: Vec<String> = Vec::new();
    // Locate the root element, skipping the leading file comment.
    let root_seed = {
        scanner.skip_trivia();
        if let Some((name, attrs, self_closing)) = scanner.take_element() {
            if TileKind::from_element(&name).is_some() {
                parse_tile_element(&name, &attrs, self_closing, &mut scanner, src, &mut includes, 0)
                    .unwrap_or_else(TileSeed::default_root)
            } else {
                // First element is not a tile (should not happen); parse
                // the rest as a synthetic rect root.
                let content = parse_element_content(&mut scanner, src, &mut includes, 0);
                TileSeed {
                    name: attr_value(&attrs, "name"),
                    id: None,
                    kind: TileKind::Rect,
                    traits: content.traits,
                    children: content.children,
                }
            }
        } else {
            log::warn!("menuxml: document has no root element");
            TileSeed::default_root()
        }
    };

    // Flatten seeds into the arena in document order (parents precede
    // children by construction, which the evaluator relies on).
    let mut tiles: Vec<Tile> = Vec::new();
    let mut name_index: HashMap<String, usize> = HashMap::new();
    let mut id_index: HashMap<i32, usize> = HashMap::new();
    fn push_seed(
        seed: TileSeed,
        parent: Option<usize>,
        tiles: &mut Vec<Tile>,
        name_index: &mut HashMap<String, usize>,
        id_index: &mut HashMap<i32, usize>,
    ) -> usize {
        let idx = tiles.len();
        let TileSeed {
            name,
            id,
            kind,
            traits,
            children,
        } = seed;
        tiles.push(Tile {
            name: name.clone(),
            id,
            kind,
            parent,
            children: Vec::new(),
            traits,
        });
        if let Some(n) = &name {
            name_index.entry(n.to_lowercase()).or_insert(idx);
        }
        if let Some(id) = id {
            id_index.entry(id).or_insert(idx);
        }
        for child in children {
            let cidx = push_seed(child, Some(idx), tiles, name_index, id_index);
            tiles[idx].children.push(cidx);
        }
        idx
    }
    let menu_name = root_seed.name.clone().unwrap_or_default();
    let root = push_seed(root_seed, None, &mut tiles, &mut name_index, &mut id_index);
    debug_assert_eq!(root, 0);

    Document {
        menu_name,
        tiles,
        name_index,
        id_index,
    }
}

impl TileSeed {
    fn default_root() -> Self {
        Self {
            name: None,
            id: None,
            kind: TileKind::Menu,
            traits: BTreeMap::new(),
            children: Vec::new(),
        }
    }
}
