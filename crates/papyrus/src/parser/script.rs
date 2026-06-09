//! M30.2 Phase 2+3 — top-level item parser + `parse_script` driver.
//!
//! Top-level item grammar (informal):
//!
//! ```text
//! script ::= NEWLINE* script_header NEWLINE script_item*
//! script_header ::= "ScriptName" IDENT ("Extends" IDENT)? script_flag*
//! script_item ::= import | property | function | event | state |
//!                 struct | custom_event | group | variable
//! property ::= type "Property" IDENT (
//!                 ("=" expr)? property_flag* NEWLINE |
//!                 NEWLINE function_or_event* "EndProperty" NEWLINE
//!              )
//! function ::= type? "Function" IDENT "(" param_list? ")" function_flag*
//!              NEWLINE block "EndFunction" NEWLINE
//! event ::= "Event" IDENT "(" param_list? ")" function_flag*
//!           NEWLINE block "EndEvent" NEWLINE
//! state ::= "Auto"? "State" IDENT NEWLINE state_item* "EndState" NEWLINE
//! struct ::= "Struct" IDENT NEWLINE variable* "EndStruct" NEWLINE
//! custom_event ::= "CustomEvent" IDENT NEWLINE
//! group ::= "Group" IDENT group_flag* NEWLINE property* "EndGroup" NEWLINE
//! variable ::= type IDENT ("=" expr)? var_flag* NEWLINE
//! import ::= "Import" qualified_ident NEWLINE
//! ```
//!
//! Flag tokens are parsed loosely — any unknown identifier after a
//! recognised item form is preserved as a parse error rather than
//! silently dropped, since flag drift across game versions (FO4
//! adds `Const`, `Hidden`, `Mandatory`, `BetaOnly`, …) is the
//! number-one breakage source for a Papyrus parser.

use crate::ast::*;
use crate::error::ParseError;
use crate::span::{Span, Spanned};
use crate::token::Token;

use super::Parser;

/// Parsed `ScriptName <ident> [Extends <ident>] [flags…]` header:
/// `(script name, optional parent, flags)`. Aliased to satisfy
/// `clippy::type_complexity` on `parse_script_header`'s return.
type ScriptHeader = (
    Spanned<Identifier>,
    Option<Spanned<Identifier>>,
    ScriptFlags,
);

impl Parser {
    /// Parse a complete `.psc` source into a [`Script`]. The
    /// canonical M30.2 entry point.
    ///
    /// Errors recovered along the way are accessible via
    /// [`Parser::errors`] after the call returns; a hard failure
    /// (the script header couldn't be parsed) returns `Err`.
    pub fn parse_script(&mut self) -> Result<Script, ParseError> {
        // Skip leading newlines / preamble doc-comments. ScriptName
        // is the first significant token in every Papyrus script.
        self.skip_newlines();
        let (name, parent, flags) = self.parse_script_header()?;
        self.expect_eol()?;

        let mut body = Vec::new();
        loop {
            // `skip_newlines_collect_doc` over `skip_newlines`: doc
            // comments at the top of a script (after the ScriptName
            // line) or between items would otherwise sit at the next
            // peek and crash the type-prefix dispatcher. Doc-aware
            // handlers (`parse_function`, `parse_event`,
            // `parse_property`) re-collect inside themselves; for
            // items that don't (Variable, State, Struct, …) the
            // metadata is dropped, but the parse continues. Full
            // doc-comment threading is M47.2 transpiler territory.
            self.skip_newlines_collect_doc();
            if self.at_eof() {
                break;
            }
            match self.parse_script_item() {
                Ok(item) => body.push(item),
                Err(e) => {
                    self.push_error(e);
                    // Recover: skip to next line and continue, so
                    // one malformed item doesn't sink the whole file.
                    self.skip_to_next_line();
                }
            }
        }

        Ok(Script {
            name,
            parent,
            flags,
            body,
        })
    }

    /// Parse the `ScriptName <ident> [Extends <ident>] [flags…]`
    /// header line. Caller has already skipped leading newlines.
    fn parse_script_header(&mut self) -> Result<ScriptHeader, ParseError> {
        self.expect(&Token::KwScriptName, "ScriptName")?;
        let name = self.expect_ident("script name")?;
        let parent = if matches!(self.peek(), Some(Token::KwExtends)) {
            self.advance().unwrap();
            Some(self.expect_ident("parent script name")?)
        } else {
            None
        };
        // Flag loop — read any number of script-level flags. Unknown
        // flag tokens stop the loop (the caller will hit them at the
        // body-item dispatcher and emit a sensible error there).
        let mut flags = ScriptFlags::empty();
        loop {
            match self.peek() {
                Some(Token::KwNative) => {
                    self.advance().unwrap();
                    flags |= ScriptFlags::NATIVE;
                }
                Some(Token::KwConst) => {
                    self.advance().unwrap();
                    flags |= ScriptFlags::CONST;
                }
                Some(Token::KwDebugOnly) => {
                    self.advance().unwrap();
                    flags |= ScriptFlags::DEBUG_ONLY;
                }
                Some(Token::KwHidden) => {
                    self.advance().unwrap();
                    flags |= ScriptFlags::HIDDEN;
                }
                _ => break,
            }
        }
        Ok((name, parent, flags))
    }

    /// Parse one top-level script item. Dispatches on the leading
    /// keyword (Import / Function / Event / Property / State / etc.)
    /// or falls through to the type-prefix path for variable and
    /// type-prefixed-property declarations.
    fn parse_script_item(&mut self) -> Result<Spanned<ScriptItem>, ParseError> {
        let (tok, start_span) = self
            .peek_with_span()
            .ok_or_else(|| ParseError::unexpected_eof("script item", self.current_span()))?;
        let tok = tok.clone();
        match tok {
            Token::KwImport => self.parse_import(start_span),
            Token::KwEvent => {
                let event = self.parse_event()?;
                let span = event.name.span;
                Ok(Spanned::new(ScriptItem::Event(event), span))
            }
            Token::KwState | Token::KwAuto => {
                let state = self.parse_state()?;
                let span = state.name.span;
                Ok(Spanned::new(ScriptItem::State(state), span))
            }
            Token::KwStruct => {
                let s = self.parse_struct()?;
                let span = s.name.span;
                Ok(Spanned::new(ScriptItem::Struct(s), span))
            }
            Token::KwCustomEvent => {
                self.advance().unwrap();
                let name = self.expect_ident("custom event name")?;
                let span = name.span;
                self.expect_eol()?;
                Ok(Spanned::new(ScriptItem::CustomEvent(name.node), span))
            }
            Token::KwGroup => {
                let g = self.parse_group()?;
                let span = g.name.span;
                Ok(Spanned::new(ScriptItem::Group(g), span))
            }
            // Function form WITHOUT return type — `Function Foo()`.
            // Function form WITH return type comes through the
            // type-prefix path below.
            Token::KwFunction => {
                let func = self.parse_function(None)?;
                let span = func.name.span;
                Ok(Spanned::new(ScriptItem::Function(func), span))
            }
            // Type-prefix path — could be Variable, Property, or
            // typed Function. Disambiguate by what follows the type.
            _ => self.parse_type_prefixed_item(),
        }
    }

    /// `Import <qualified_ident> NEWLINE`.
    fn parse_import(&mut self, start_span: Span) -> Result<Spanned<ScriptItem>, ParseError> {
        self.advance().unwrap(); // `Import`
        let name = self.expect_ident("import target")?;
        let span = start_span.merge(name.span);
        self.expect_eol()?;
        Ok(Spanned::new(ScriptItem::Import(name.node), span))
    }

    /// Disambiguate type-prefixed items. After parsing a type, the
    /// next token is:
    ///   - `Function` → typed Function (`Int Function Foo()`)
    ///   - `Property` → Property declaration
    ///   - `Ident` → Variable declaration (top-level field)
    fn parse_type_prefixed_item(&mut self) -> Result<Spanned<ScriptItem>, ParseError> {
        let ty = self.parse_type()?;
        match self.peek() {
            Some(Token::KwFunction) => {
                let func = self.parse_function(Some(ty))?;
                let span = func.name.span;
                Ok(Spanned::new(ScriptItem::Function(func), span))
            }
            Some(Token::KwProperty) => {
                let prop = self.parse_property(ty)?;
                let span = prop.name.span;
                Ok(Spanned::new(ScriptItem::Property(Box::new(prop)), span))
            }
            Some(Token::Ident(_)) => {
                // Top-level variable declaration.
                let name = self.expect_ident("variable name")?;
                let initial_value = if matches!(self.peek(), Some(Token::Eq)) {
                    self.advance().unwrap();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                let (is_conditional, is_const) = self.parse_variable_flags();
                self.expect_eol()?;
                let span = ty.span.merge(name.span);
                Ok(Spanned::new(
                    ScriptItem::Variable(Variable {
                        ty,
                        name,
                        initial_value,
                        is_conditional,
                        is_const,
                    }),
                    span,
                ))
            }
            other => {
                let span = self.current_span();
                Err(ParseError::unexpected_token(
                    "Function, Property, or variable name after type",
                    other.cloned(),
                    span,
                ))
            }
        }
    }

    fn parse_variable_flags(&mut self) -> (bool, bool) {
        let mut is_conditional = false;
        let mut is_const = false;
        loop {
            match self.peek() {
                Some(Token::KwConditional) => {
                    self.advance().unwrap();
                    is_conditional = true;
                }
                Some(Token::KwConst) => {
                    self.advance().unwrap();
                    is_const = true;
                }
                _ => break,
            }
        }
        (is_conditional, is_const)
    }

    /// `[type] Function IDENT ( [param_list] ) [flags] NEWLINE
    ///  block EndFunction NEWLINE`
    ///
    /// `Native` functions have no body — the EndFunction is omitted
    /// (and there's no NEWLINE before EndFunction either since the
    /// declaration ends after the flags).
    fn parse_function(
        &mut self,
        return_type: Option<Spanned<Type>>,
    ) -> Result<Function, ParseError> {
        let doc_comment = self.skip_newlines_collect_doc();
        self.expect(&Token::KwFunction, "Function")?;
        let name = self.expect_ident("function name")?;
        let params = self.parse_param_list()?;
        let flags = self.parse_function_flags();
        self.expect_eol()?;
        let body = if flags.contains(FunctionFlags::NATIVE) {
            // Native functions have no body. The header is the
            // entire item.
            Vec::new()
        } else {
            let body = self.parse_block(&[Token::KwEndFunction])?;
            self.expect(&Token::KwEndFunction, "EndFunction")?;
            self.expect_eol()?;
            body
        };
        Ok(Function {
            return_type,
            name,
            params,
            flags,
            body,
            doc_comment,
        })
    }

    /// `Event IDENT ( [param_list] ) [flags] NEWLINE block EndEvent NEWLINE`.
    /// Events follow the same shape as functions but always return
    /// void and don't allow `Global` / `Native` (per Bethesda spec —
    /// but we accept the flags anyway and let semantic analysis flag
    /// the violation).
    fn parse_event(&mut self) -> Result<Event, ParseError> {
        let doc_comment = self.skip_newlines_collect_doc();
        self.expect(&Token::KwEvent, "Event")?;
        let name = self.expect_ident("event name")?;
        let params = self.parse_param_list()?;
        let flags = self.parse_function_flags();
        self.expect_eol()?;
        let body = self.parse_block(&[Token::KwEndEvent])?;
        self.expect(&Token::KwEndEvent, "EndEvent")?;
        self.expect_eol()?;
        Ok(Event {
            name,
            params,
            flags,
            body,
            doc_comment,
        })
    }

    /// `( [Type IDENT ("=" expr)? ("," ...)?] )`
    fn parse_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(&Token::LParen, "(")?;
        let mut params = Vec::new();
        if matches!(self.peek(), Some(Token::RParen)) {
            self.advance().unwrap();
            return Ok(params);
        }
        loop {
            let ty = self.parse_type()?;
            let name = self.expect_ident("parameter name")?;
            let default = if matches!(self.peek(), Some(Token::Eq)) {
                self.advance().unwrap();
                Some(self.parse_expr()?)
            } else {
                None
            };
            params.push(Param { ty, name, default });
            match self.peek() {
                Some(Token::Comma) => {
                    self.advance().unwrap();
                }
                _ => break,
            }
        }
        self.expect(&Token::RParen, ")")?;
        Ok(params)
    }

    fn parse_function_flags(&mut self) -> FunctionFlags {
        let mut flags = FunctionFlags::empty();
        loop {
            match self.peek() {
                Some(Token::KwGlobal) => {
                    self.advance().unwrap();
                    flags |= FunctionFlags::GLOBAL;
                }
                Some(Token::KwNative) => {
                    self.advance().unwrap();
                    flags |= FunctionFlags::NATIVE;
                }
                Some(Token::KwDebugOnly) => {
                    self.advance().unwrap();
                    flags |= FunctionFlags::DEBUG_ONLY;
                }
                Some(Token::KwBetaOnly) => {
                    self.advance().unwrap();
                    flags |= FunctionFlags::BETA_ONLY;
                }
                _ => break,
            }
        }
        flags
    }

    /// `type Property IDENT (= expr)? property_flag* NEWLINE` (short form)
    /// or
    /// `type Property IDENT NEWLINE function_or_event* EndProperty NEWLINE` (full form).
    fn parse_property(&mut self, ty: Spanned<Type>) -> Result<Property, ParseError> {
        let doc_comment = self.skip_newlines_collect_doc();
        self.expect(&Token::KwProperty, "Property")?;
        let name = self.expect_ident("property name")?;
        let initial_value = if matches!(self.peek(), Some(Token::Eq)) {
            self.advance().unwrap();
            Some(self.parse_expr()?)
        } else {
            None
        };
        let flags = self.parse_property_flags();
        self.expect_eol()?;

        // Auto / AutoReadOnly properties have no body — the
        // declaration ends with the newline. Full-form properties
        // have getter / setter functions between Property and
        // EndProperty.
        let (getter, setter) = if flags.contains(PropertyFlags::AUTO)
            || flags.contains(PropertyFlags::AUTO_READ_ONLY)
        {
            (None, None)
        } else {
            self.parse_property_accessors()?
        };

        Ok(Property {
            ty,
            name,
            flags,
            initial_value,
            getter,
            setter,
            doc_comment,
        })
    }

    fn parse_property_flags(&mut self) -> PropertyFlags {
        let mut flags = PropertyFlags::empty();
        loop {
            match self.peek() {
                Some(Token::KwAuto) => {
                    self.advance().unwrap();
                    flags |= PropertyFlags::AUTO;
                }
                Some(Token::KwAutoReadOnly) => {
                    self.advance().unwrap();
                    flags |= PropertyFlags::AUTO_READ_ONLY;
                }
                Some(Token::KwConst) => {
                    self.advance().unwrap();
                    flags |= PropertyFlags::CONST;
                }
                Some(Token::KwMandatory) => {
                    self.advance().unwrap();
                    flags |= PropertyFlags::MANDATORY;
                }
                Some(Token::KwHidden) => {
                    self.advance().unwrap();
                    flags |= PropertyFlags::HIDDEN;
                }
                Some(Token::KwConditional) => {
                    self.advance().unwrap();
                    flags |= PropertyFlags::CONDITIONAL;
                }
                _ => break,
            }
        }
        flags
    }

    /// Full-form property: walk `Function` / `Event` accessors until
    /// `EndProperty`. A property has at most one getter + one setter;
    /// the parser identifies which is which by name convention
    /// (Get / Set) AND by signature (getter takes 0 args + returns
    /// the property type; setter takes 1 arg + returns void).
    /// M30.2's first iteration accepts any two functions and stores
    /// the first as `getter`, second as `setter` — semantic
    /// validation is M47.2 territory.
    fn parse_property_accessors(
        &mut self,
    ) -> Result<(Option<Function>, Option<Function>), ParseError> {
        let mut getter = None;
        let mut setter = None;
        loop {
            self.skip_newlines();
            match self.peek() {
                Some(Token::KwEndProperty) => {
                    self.advance().unwrap();
                    self.expect_eol()?;
                    return Ok((getter, setter));
                }
                Some(Token::KwFunction) => {
                    let func = self.parse_function(None)?;
                    if getter.is_none() {
                        getter = Some(func);
                    } else if setter.is_none() {
                        setter = Some(func);
                    } else {
                        // Third accessor — error but keep parsing
                        // so the rest of the script still loads.
                        let span = func.name.span;
                        self.push_error(ParseError::unexpected_token(
                            "EndProperty after second accessor",
                            None,
                            span,
                        ));
                    }
                }
                // Type-prefixed function inside a property — `Int Function Get()`.
                _ => {
                    let ty = self.parse_type()?;
                    let func = self.parse_function(Some(ty))?;
                    if getter.is_none() {
                        getter = Some(func);
                    } else if setter.is_none() {
                        setter = Some(func);
                    }
                }
            }
        }
    }

    /// `[Auto] State IDENT NEWLINE state_item* EndState NEWLINE`.
    /// State items are functions or events.
    fn parse_state(&mut self) -> Result<State, ParseError> {
        let is_auto = if matches!(self.peek(), Some(Token::KwAuto)) {
            self.advance().unwrap();
            true
        } else {
            false
        };
        self.expect(&Token::KwState, "State")?;
        let name = self.expect_ident("state name")?;
        self.expect_eol()?;
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Some(Token::KwEndState) => {
                    self.advance().unwrap();
                    self.expect_eol()?;
                    return Ok(State {
                        name,
                        is_auto,
                        body,
                    });
                }
                Some(Token::KwEvent) => {
                    let ev = self.parse_event()?;
                    let span = ev.name.span;
                    body.push(Spanned::new(StateItem::Event(ev), span));
                }
                Some(Token::KwFunction) => {
                    let func = self.parse_function(None)?;
                    let span = func.name.span;
                    body.push(Spanned::new(StateItem::Function(func), span));
                }
                _ => {
                    // Try type-prefixed function (returning function).
                    let ty = self.parse_type()?;
                    let func = self.parse_function(Some(ty))?;
                    let span = func.name.span;
                    body.push(Spanned::new(StateItem::Function(func), span));
                }
            }
        }
    }

    /// `Struct IDENT NEWLINE variable* EndStruct NEWLINE`.
    /// Struct members are typed fields (no flag annotations beyond
    /// what `Variable` carries).
    fn parse_struct(&mut self) -> Result<Struct, ParseError> {
        self.expect(&Token::KwStruct, "Struct")?;
        let name = self.expect_ident("struct name")?;
        self.expect_eol()?;
        let mut members = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Some(Token::KwEndStruct) => {
                    self.advance().unwrap();
                    self.expect_eol()?;
                    return Ok(Struct { name, members });
                }
                _ => {
                    let var = self.parse_variable_body()?;
                    self.expect_eol()?;
                    members.push(var);
                }
            }
        }
    }

    /// `Group IDENT [group_flag]* NEWLINE property* EndGroup NEWLINE`.
    fn parse_group(&mut self) -> Result<Group, ParseError> {
        self.expect(&Token::KwGroup, "Group")?;
        let name = self.expect_ident("group name")?;
        let mut flags = GroupFlags::empty();
        loop {
            match self.peek() {
                Some(Token::KwCollapsedOnRef) => {
                    self.advance().unwrap();
                    flags |= GroupFlags::COLLAPSED_ON_REF;
                }
                Some(Token::KwCollapsedOnBase) => {
                    self.advance().unwrap();
                    flags |= GroupFlags::COLLAPSED_ON_BASE;
                }
                _ => break,
            }
        }
        self.expect_eol()?;

        let mut properties = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Some(Token::KwEndGroup) => {
                    self.advance().unwrap();
                    self.expect_eol()?;
                    return Ok(Group {
                        name,
                        flags,
                        properties,
                    });
                }
                _ => {
                    let ty = self.parse_type()?;
                    let prop = self.parse_property(ty)?;
                    let span = prop.name.span;
                    properties.push(Spanned::new(prop, span));
                }
            }
        }
    }

    // ── Recovery helper ────────────────────────────────────────────

    /// Skip to the start of the next line after an error. Consumes
    /// tokens until a Newline (inclusive) or EOF.
    fn skip_to_next_line(&mut self) {
        while let Some(tok) = self.peek() {
            if matches!(tok, Token::Newline) {
                self.advance().unwrap();
                return;
            }
            self.advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{lex, preprocess};

    fn parse(src: &str) -> Script {
        let (preprocessed, _map) = preprocess(src);
        let (tokens, _errs) = lex(&preprocessed);
        let mut parser = Parser::new(tokens);
        let script = parser.parse_script().expect("parse_script must succeed");
        if !parser.errors().is_empty() {
            panic!(
                "parse_script left {} recovered errors: {:#?}",
                parser.errors().len(),
                parser.errors()
            );
        }
        script
    }

    #[test]
    fn minimal_script_header() {
        let src = "ScriptName Foo\n";
        let s = parse(src);
        assert_eq!(s.name.node.0, "Foo");
        assert!(s.parent.is_none());
        assert!(s.flags.is_empty());
        assert!(s.body.is_empty());
    }

    #[test]
    fn script_header_with_extends_and_flags() {
        let src = "ScriptName Foo Extends Quest Native Hidden\n";
        let s = parse(src);
        assert_eq!(s.name.node.0, "Foo");
        assert_eq!(s.parent.unwrap().node.0, "Quest");
        assert!(s.flags.contains(ScriptFlags::NATIVE));
        assert!(s.flags.contains(ScriptFlags::HIDDEN));
    }

    #[test]
    fn parse_event_with_body() {
        let src = "\
ScriptName Foo

Event OnActivate(ObjectReference akActionRef)
  Return
EndEvent
";
        let s = parse(src);
        assert_eq!(s.body.len(), 1);
        match &s.body[0].node {
            ScriptItem::Event(ev) => {
                assert_eq!(ev.name.node.0, "OnActivate");
                assert_eq!(ev.params.len(), 1);
                assert_eq!(ev.params[0].name.node.0, "akActionRef");
                assert_eq!(ev.body.len(), 1);
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn parse_typed_function_with_return() {
        let src = "\
ScriptName Foo

Int Function Add(Int a, Int b)
  Return a + b
EndFunction
";
        let s = parse(src);
        assert_eq!(s.body.len(), 1);
        match &s.body[0].node {
            ScriptItem::Function(func) => {
                assert_eq!(func.name.node.0, "Add");
                assert!(matches!(
                    func.return_type.as_ref().map(|t| &t.node),
                    Some(Type::Int)
                ));
                assert_eq!(func.params.len(), 2);
            }
            other => panic!("expected Function, got {other:?}"),
        }
    }

    #[test]
    fn parse_native_function_no_body() {
        let src = "\
ScriptName Foo

Float Function GetRange() Native Global
";
        let s = parse(src);
        match &s.body[0].node {
            ScriptItem::Function(func) => {
                assert_eq!(func.name.node.0, "GetRange");
                assert!(func.flags.contains(FunctionFlags::NATIVE));
                assert!(func.flags.contains(FunctionFlags::GLOBAL));
                assert!(func.body.is_empty());
            }
            other => panic!("expected Native Function, got {other:?}"),
        }
    }

    #[test]
    fn parse_auto_property_with_initializer() {
        let src = "\
ScriptName Foo

Float Property duration = 0.25 Auto
";
        let s = parse(src);
        match &s.body[0].node {
            ScriptItem::Property(p) => {
                assert_eq!(p.name.node.0, "duration");
                assert!(p.flags.contains(PropertyFlags::AUTO));
                let init = p.initial_value.as_ref().unwrap();
                assert!(matches!(init.node, Expr::FloatLit(_)));
            }
            other => panic!("expected Property, got {other:?}"),
        }
    }

    #[test]
    fn parse_state_with_event() {
        let src = "\
ScriptName Foo

Auto State waiting
  Event OnActivate(ObjectReference a)
    Return
  EndEvent
EndState
";
        let s = parse(src);
        match &s.body[0].node {
            ScriptItem::State(state) => {
                assert!(state.is_auto);
                assert_eq!(state.name.node.0, "waiting");
                assert_eq!(state.body.len(), 1);
                assert!(matches!(state.body[0].node, StateItem::Event(_)));
            }
            other => panic!("expected State, got {other:?}"),
        }
    }

    #[test]
    fn parse_import_statement() {
        let src = "\
ScriptName Foo

Import Debug
";
        let s = parse(src);
        match &s.body[0].node {
            ScriptItem::Import(name) => assert_eq!(name.0, "Debug"),
            other => panic!("expected Import, got {other:?}"),
        }
    }

    #[test]
    fn parse_full_rumble_on_activate_translation() {
        // The R5 source script that motivated M30.2.
        let src = "\
ScriptName defaultRumbleOnActivate Extends ObjectReference

Float Property cameraIntensity = 0.25 Auto
Float Property duration = 0.25 Auto
Bool Property repeatable = True Auto
Float Property shakeLeft = 0.25 Auto
Float Property shakeRight = 0.25 Auto

Auto State active
  Event OnActivate(ObjectReference actronaut)
    Return
  EndEvent
EndState

State busy
  Event OnActivate(ObjectReference actronaut)
  EndEvent
EndState

State inactive
  Event OnActivate(ObjectReference actronaut)
  EndEvent
EndState
";
        let s = parse(src);
        assert_eq!(s.name.node.0, "defaultRumbleOnActivate");
        assert_eq!(s.parent.unwrap().node.0, "ObjectReference");
        // 5 properties + 3 states = 8 items.
        assert_eq!(s.body.len(), 8);
        let prop_count = s
            .body
            .iter()
            .filter(|i| matches!(i.node, ScriptItem::Property(_)))
            .count();
        let state_count = s
            .body
            .iter()
            .filter(|i| matches!(i.node, ScriptItem::State(_)))
            .count();
        assert_eq!(prop_count, 5);
        assert_eq!(state_count, 3);
    }
}
