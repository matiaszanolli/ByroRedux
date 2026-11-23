//! `cndf` - evaluate a Starfield condition form (`ConditionForm.IsTrue`) from the console.

use super::shared::*;

/// `cndf <form_id> <subject entity|.> [target entity]`: what `ConditionForm.IsTrue(subject,
/// target)` answers for the `CNDF` record, with the form's conditions; `cndf list` names the
/// forms the interpreter can answer.
pub(crate) struct CndfCommand;

impl ConsoleCommand for CndfCommand {
    fn name(&self) -> &str {
        "cndf"
    }

    fn description(&self) -> &str {
        "ConditionForm.IsTrue from the console: cndf <form_id> <subject|.> [target], or cndf list"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let Some(first) = tokens.next() else {
            return CommandOutput::error("usage: cndf <form_id> <subject|.> [target] | cndf list");
        };
        let Some(forms) = world.try_resource::<byroredux_scripting::ConditionForms>() else {
            return CommandOutput::error("cndf: no condition forms installed (Starfield only)");
        };
        if first == "list" {
            let runnable = forms.runnable(world);
            let shown: Vec<String> = runnable.iter().take(40).map(|form| format!("{form:08X}")).collect();
            return CommandOutput::line(format!("{} answerable forms: {}", runnable.len(), shown.join(" ")));
        }
        let Some(form) = parse_console_form_id(first) else {
            return CommandOutput::error(format!("cndf: bad form id `{first}`"));
        };
        let Some(subject_token) = tokens.next() else {
            return CommandOutput::error("usage: cndf <form_id> <subject|.> [target]");
        };
        let subject = match resolve_console_entity(world, subject_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("cndf: {message}")),
        };
        let target = match tokens.next().map(|token| resolve_console_entity(world, token)) {
            Some(Ok(entity)) => Some(entity),
            Some(Err(message)) => return CommandOutput::error(format!("cndf: {message}")),
            None => None,
        };
        let conditions = forms.conditions(form).map(|list| format!("{list:?}"));
        drop(forms);
        let answer = byroredux_scripting::condition_form_is_true(world, form, subject, target);
        CommandOutput::lines(vec![format!("IsTrue({form:08X}) = {answer:?}"), format!("conditions: {conditions:?}")])
    }
}
