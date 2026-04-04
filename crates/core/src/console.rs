//! Console command system — extensible command registry for debug and scripting.
//!
//! The `CommandRegistry` is stored as a World resource. Any crate can define
//! commands by implementing `ConsoleCommand`; the binary registers them at startup.
//! Commands execute against `&World` and return text output.

use crate::ecs::resource::Resource;
use crate::ecs::world::World;

/// Output from a console command execution.
pub struct CommandOutput {
    pub lines: Vec<String>,
}

impl CommandOutput {
    pub fn line(msg: impl Into<String>) -> Self {
        Self {
            lines: vec![msg.into()],
        }
    }

    pub fn lines(lines: Vec<String>) -> Self {
        Self { lines }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            lines: vec![format!("Error: {}", msg.into())],
        }
    }
}

/// A command that can be executed against the ECS world.
pub trait ConsoleCommand: Send + Sync {
    /// The name used to invoke this command (e.g., "stats", "help").
    fn name(&self) -> &str;

    /// One-line description for help text.
    fn description(&self) -> &str;

    /// Execute the command. `args` is the remainder after the command name.
    fn execute(&self, world: &World, args: &str) -> CommandOutput;
}

/// Registry of available console commands, stored as a World resource.
pub struct CommandRegistry {
    commands: Vec<Box<dyn ConsoleCommand>>,
}

impl Resource for CommandRegistry {}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Register a new command.
    pub fn register(&mut self, cmd: impl ConsoleCommand + 'static) {
        self.commands.push(Box::new(cmd));
    }

    /// Execute a command by parsing the input string.
    ///
    /// Splits on the first whitespace: the first word is the command name,
    /// the rest is passed as args.
    pub fn execute(&self, world: &World, input: &str) -> CommandOutput {
        let input = input.trim();
        if input.is_empty() {
            return self.help_output();
        }

        let (name, args) = match input.split_once(char::is_whitespace) {
            Some((n, a)) => (n, a.trim()),
            None => (input, ""),
        };

        for cmd in &self.commands {
            if cmd.name() == name {
                return cmd.execute(world, args);
            }
        }

        CommandOutput::error(format!(
            "Unknown command '{}'. Type 'help' for a list.",
            name
        ))
    }

    /// List all registered commands as (name, description) pairs.
    pub fn list(&self) -> Vec<(&str, &str)> {
        self.commands
            .iter()
            .map(|cmd| (cmd.name(), cmd.description()))
            .collect()
    }

    fn help_output(&self) -> CommandOutput {
        let mut lines = vec!["Available commands:".to_string()];
        for cmd in &self.commands {
            lines.push(format!("  {:16} {}", cmd.name(), cmd.description()));
        }
        CommandOutput::lines(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::world::World;

    struct EchoCommand;

    impl ConsoleCommand for EchoCommand {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes the input"
        }
        fn execute(&self, _world: &World, args: &str) -> CommandOutput {
            CommandOutput::line(format!("Echo: {}", args))
        }
    }

    struct CountCommand;

    impl ConsoleCommand for CountCommand {
        fn name(&self) -> &str {
            "count"
        }
        fn description(&self) -> &str {
            "Returns a fixed count"
        }
        fn execute(&self, _world: &World, _args: &str) -> CommandOutput {
            CommandOutput::line("42")
        }
    }

    #[test]
    fn register_and_execute() {
        let mut registry = CommandRegistry::new();
        registry.register(EchoCommand);

        let world = World::new();
        let output = registry.execute(&world, "echo hello world");
        assert_eq!(output.lines, vec!["Echo: hello world"]);
    }

    #[test]
    fn unknown_command_returns_error() {
        let registry = CommandRegistry::new();
        let world = World::new();
        let output = registry.execute(&world, "nonexistent");
        assert!(output.lines[0].contains("Unknown command"));
    }

    #[test]
    fn empty_input_returns_help() {
        let mut registry = CommandRegistry::new();
        registry.register(EchoCommand);

        let world = World::new();
        let output = registry.execute(&world, "");
        assert!(output.lines[0].contains("Available commands"));
        assert!(output.lines.iter().any(|l| l.contains("echo")));
    }

    #[test]
    fn list_returns_registered_commands() {
        let mut registry = CommandRegistry::new();
        registry.register(EchoCommand);
        registry.register(CountCommand);

        let list = registry.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].0, "echo");
        assert_eq!(list[1].0, "count");
    }

    #[test]
    fn command_with_no_args() {
        let mut registry = CommandRegistry::new();
        registry.register(CountCommand);

        let world = World::new();
        let output = registry.execute(&world, "count");
        assert_eq!(output.lines, vec!["42"]);
    }

    #[test]
    fn whitespace_only_input_returns_help() {
        let registry = CommandRegistry::new();
        let world = World::new();
        let output = registry.execute(&world, "   ");
        assert!(output.lines[0].contains("Available commands"));
    }
}
