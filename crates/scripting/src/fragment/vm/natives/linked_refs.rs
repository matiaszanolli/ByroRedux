//! Linked-reference natives: a link, a chain, its children and the chain's enable state.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getlinkedref",
    "getnthlinkedref",
    "enablelinkchain",
    "disablelinkchain",
    "getlinkedrefchain",
    "getlinkedrefchildren",
    "countlinkedrefchain",
];

impl Vm<'_> {
    pub(in crate::fragment::vm) fn linked_ref(&self, form: u32, keyword: u32) -> Option<u32> {
        crate::linked_ref(self.world, form, keyword)
    }

    /// The linked-ref reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_linked_refs(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            // CK *EnableLinkChain* / *DisableLinkChain*: the function is script
            // code over `GetLinkedRef` — follow the keyword's links from this
            // reference, enabling (or disabling) each. A chain that loops back
            // ends there.
            (Value::Form(start), "enablelinkchain" | "disablelinkchain") if args.len() <= 2 => {
                let values = self.eval_args(frame, args)?;
                let keyword = match values.first() {
                    Some(Value::Form(keyword)) => *keyword,
                    _ => 0,
                };
                let enabled = name == "enablelinkchain";
                let mut seen = std::collections::HashSet::new();
                let mut current = self.linked_ref(*start, keyword);
                while let Some(link) = current {
                    if !seen.insert(link) || seen.len() > MAX_LINK_CHAIN_HOPS {
                        break;
                    }
                    self.deferred.reference_enable_changes.push((link, enabled));
                    current = self.linked_ref(link, keyword);
                }
                Some(Value::None)
            }
            (Value::Form(form), "getlinkedref") => {
                let keyword = match args.first() {
                    Some(arg) => match self.eval(frame, &arg.value.node)? {
                        Value::Form(keyword) => keyword,
                        _ => 0,
                    },
                    None => 0,
                };
                Some(self.linked_ref(*form, keyword).map_or(Value::None, Value::Form))
            }
            // CK *GetLinkedRefChain*: the references the object is linked to
            // under the keyword, and whatever those are linked to, and so on;
            // "Will stop if it finds a loop", or at `iMaxExpectedLinkedRefs`
            // (default 100).
            (Value::Form(form), "getlinkedrefchain") if args.len() <= 2 => {
                let values = self.eval_args(frame, args)?;
                let keyword = match values.first() {
                    Some(Value::Form(keyword)) => *keyword,
                    _ => 0,
                };
                let limit = values
                    .get(1)
                    .and_then(Value::number)
                    .map_or(MAX_LINK_CHAIN_HOPS as f32, |max| max.max(0.0))
                    .min(MAX_LINK_CHAIN_HOPS as f32) as usize;
                let mut seen = vec![*form];
                let mut current = *form;
                while seen.len() - 1 < limit {
                    let Some(next) = self.linked_ref(current, keyword) else {
                        break;
                    };
                    if seen.contains(&next) {
                        break;
                    }
                    seen.push(next);
                    current = next;
                }
                let chain = seen.into_iter().skip(1).map(Value::Form).collect();
                Some(Value::Array(Rc::new(RefCell::new(chain))))
            }
            // CK *CountLinkedRefChain*, which the page gives as script: starting
            // at the reference, each step takes the keyword's link and counts
            // it (a missing link too: A to B to C counts 3) unless the link is
            // the reference itself, and stops past the cap (default 100).
            (Value::Form(form), "countlinkedrefchain") if args.len() <= 2 => {
                let values = self.eval_args(frame, args)?;
                let keyword = match values.first() {
                    Some(Value::Form(keyword)) => *keyword,
                    _ => 0,
                };
                let cap = values.get(1).and_then(Value::number).map_or(MAX_LINK_CHAIN_HOPS as f32, |max| max).min(MAX_LINK_CHAIN_HOPS as f32) as i64;
                let mut count = 0i64;
                let mut current = Some(*form);
                while let Some(here) = current {
                    if count > cap {
                        break;
                    }
                    let next = self.linked_ref(here, keyword);
                    if next == Some(*form) {
                        current = None;
                    } else {
                        current = next;
                        count += 1;
                    }
                }
                Some(Value::Int(count as i32))
            }
            // CK *GetLinkedRefChildren*: the loaded references directly linked
            // to this one under the keyword.
            (Value::Form(form), "getlinkedrefchildren") if args.len() == 1 => {
                let keyword = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(keyword) => keyword,
                    _ => 0,
                };
                let children = crate::linked_ref_children(self.world, *form, keyword)
                    .into_iter()
                    .map(Value::Form)
                    .collect();
                Some(Value::Array(Rc::new(RefCell::new(children))))
            }
            // CK *GetNthLinkedRef*: 0 is the object itself, n follows the link
            // under the keyword (default none) n times.
            (Value::Form(form), "getnthlinkedref") => {
                let values = self.eval_args(frame, args)?;
                let hops = values.first().and_then(Value::number).unwrap_or(0.0);
                let keyword = match values.get(1) {
                    Some(Value::Form(keyword)) => *keyword,
                    _ => 0,
                };
                let mut current = Some(*form);
                // #5844 — past the ceiling there is no Nth link: return
                // None rather than silently the 100th.
                if hops.max(0.0) as usize > MAX_LINK_CHAIN_HOPS {
                    return Ok(Some(Value::None));
                }
                for _ in 0..(hops.max(0.0) as usize).min(MAX_LINK_CHAIN_HOPS) {
                    current = current.and_then(|form| self.linked_ref(form, keyword));
                }
                Some(current.map_or(Value::None, Value::Form))
            }
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The linked-ref calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_linked_refs(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "getlinkedref") => {
                return Some(Kind::Object);
            }
            (Kind::Object | Kind::Player, "getlinkedrefchain")
                if arg_kinds.len() <= 2
                    && arg_kinds.first().is_none_or(|keyword| {
                        matches!(keyword, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object))
                    })
                    && arg_kinds
                        .get(1)
                        .is_none_or(|max| matches!(max, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::Array);
            }
            (Kind::Object | Kind::Player, "countlinkedrefchain")
                if arg_kinds.len() <= 2
                    && arg_kinds.first().is_none_or(|keyword| {
                        matches!(keyword, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object))
                    })
                    && arg_kinds
                        .get(1)
                        .is_none_or(|max| matches!(max, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::Num);
            }
            (Kind::Object | Kind::Player, "getlinkedrefchildren")
                if matches!(arg_kinds, [Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object)]) =>
            {
                return Some(Kind::Array);
            }
            (Kind::Object | Kind::Player, "getnthlinkedref")
                if match arg_kinds {
                    [hops] | [hops, _] => {
                        matches!(hops, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))
                            && arg_kinds.get(1).is_none_or(|keyword| {
                                matches!(keyword, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object))
                            })
                    }
                    _ => false,
                } =>
            {
                return Some(Kind::Object);
            }
            (Kind::Object | Kind::Player, "enablelinkchain" | "disablelinkchain")
                if statement && arg_kinds.len() <= 2 && arg_kinds.iter().all(|a| matches!(a, Arg::Kind(Kind::Object | Kind::Bool) | Arg::Literal(Expr::NoneLit | Expr::BoolLit(_)))) =>
            {
                return Some(Kind::None);
            }
            _ => {}
        }
        None
    }
}
