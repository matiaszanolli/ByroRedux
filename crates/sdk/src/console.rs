//! Bounded engine-console declarations and callback results.

use serde::{Deserialize, Serialize};

use crate::identity::{ComponentId, ConsoleCommandId, ExtensionId};

/// Maximum commands one extension package may contribute.
pub const MAX_CONSOLE_COMMANDS: usize = 64;
/// Maximum UTF-8 bytes in one command description.
pub const MAX_CONSOLE_DESCRIPTION_BYTES: usize = 256;
/// Maximum UTF-8 bytes forwarded from one console invocation.
pub const MAX_CONSOLE_ARGUMENT_BYTES: usize = 4 * 1024;
/// Maximum lines one invocation may return.
pub const MAX_CONSOLE_OUTPUT_LINES: usize = 64;
/// Maximum UTF-8 bytes in one returned line.
pub const MAX_CONSOLE_OUTPUT_LINE_BYTES: usize = 4 * 1024;
/// Maximum aggregate UTF-8 bytes returned by one invocation.
pub const MAX_CONSOLE_OUTPUT_BYTES: usize = 64 * 1024;

/// One manifest-declared command routed to a specific sandbox component.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleCommandDeclaration {
    /// Manifest-local command name. The engine publishes it as
    /// `ext.<extension-id>.<id>` so packages cannot shadow engine commands.
    pub id: ConsoleCommandId,
    /// Component receiving `on-console-command` for this declaration.
    pub component: ComponentId,
    /// Bounded one-line help text shown by the engine console.
    pub description: String,
}

impl ConsoleCommandDeclaration {
    /// Fully qualified engine-console spelling reserved to this principal.
    pub fn qualified_name(&self, extension: &ExtensionId) -> String {
        format!("ext.{extension}.{}", self.id)
    }
}

/// Bounded result produced by one sandbox console callback.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConsoleCommandResult {
    pub success: bool,
    pub lines: Vec<String>,
}
