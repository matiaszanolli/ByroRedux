//! #5804 — `subject_requirement` and its types, split out of
//! `condition.rs` (see that hub's module docs for the overview).

use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::{ConditionList, RunOn};

use super::catalog::ConditionFunction;
use super::context::ConditionContext;
use super::eval::{comparand_value, evaluate_condition, or_blocks};

/// A test a subject passes when `function`'s
/// [`run_on_identity`](ConditionFunction::run_on_identity) equals `id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityTest {
    pub function: ConditionFunction,
    pub id: u32,
}

/// What a condition list demands of its subject, for a caller about to
/// evaluate the same list against many subjects in one context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubjectRequirement {
    /// A block is false whatever the subject, so [`evaluate`] is false for
    /// every subject.
    Unsatisfiable,
    /// A subject can pass only by passing one identity test from each listed
    /// block. Necessary, not sufficient — the list still has to be evaluated
    /// for each subject that does. Empty when no block constrains the
    /// subject that way.
    IdentityBlocks(Vec<Vec<IdentityTest>>),
}

/// Work out, once, what `conditions` demand of any subject evaluated in
/// `ctx`. `ctx.subject` is never read, so any placeholder will do.
///
/// A condition that cannot depend on the subject — its Run-On names another
/// slot, or its function ignores the entity
/// ([`ConditionFunction::reads_run_on_entity`]) — has the same value for
/// every subject, so it is evaluated here once: a block with one true member
/// is always true, and a block whose members are all false in this way is
/// always false. A subject-dependent member narrows its block only when it
/// is an identity test that fails on a 0.0 result; then the block can only
/// pass where one of those identities matches. A non-deterministic member
/// ([`ConditionFunction::is_deterministic`]) is never evaluated and never
/// narrows: it leaves its block open.
pub fn subject_requirement(
    conditions: &ConditionList,
    world: &World,
    ctx: &ConditionContext,
) -> SubjectRequirement {
    let mut identity_blocks = Vec::new();
    'blocks: for block in or_blocks(conditions) {
        let mut tests = Vec::new();
        let mut unconstrained = false;
        for condition in &conditions[block] {
            let function = ConditionFunction::from_index(condition.function_index);
            if !function.is_deterministic() {
                // #5645 — never evaluated here: one draw cannot stand for
                // every subject's own, and evaluating would advance the
                // RNG. It narrows nothing, so the block stays open.
                unconstrained = true;
            } else if !matches!(condition.run_on, RunOn::Subject)
                || !function.reads_run_on_entity()
            {
                if evaluate_condition(condition, world, ctx) {
                    continue 'blocks;
                }
            } else if function.is_identity_test()
                && !condition
                    .comparator
                    .apply(0.0, comparand_value(condition, world))
            {
                tests.push(IdentityTest {
                    function,
                    id: condition.param_1,
                });
            } else {
                unconstrained = true;
            }
        }
        if unconstrained {
            continue;
        }
        if tests.is_empty() {
            return SubjectRequirement::Unsatisfiable;
        }
        identity_blocks.push(tests);
    }
    SubjectRequirement::IdentityBlocks(identity_blocks)
}
