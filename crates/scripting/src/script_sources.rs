//! Where the session's compiled scripts come from: each source (a mod root's loose scripts or a script archive), the
//! scripts it owns and the principal it runs as. The extenders' version probes answer per source
//! ([`crate::extender_catalog::probe_answer_for`], LMC-SCR-010), and a mod event a script sends is published under its
//! source's principal ([`crate::fragment`]'s mod events, LMC-SCR-004). Implemented by the binary over its script
//! provider.

use byroredux_core::ecs::{Resource, World};
use byroredux_sdk::identity::PrincipalId;

/// The session's script sources.
pub trait ScriptSources: Send + Sync {
    /// The source a script resolves to, by index; `None`: no source holds it.
    fn source_of(&self, world: &World, script: &str) -> Option<usize>;
    /// Every compiled script the source owns.
    fn scripts(&self, world: &World, source: usize) -> Vec<Vec<u8>>;
    /// The principal a script's source runs as.
    fn principal_of(&self, world: &World, script: &str) -> Option<PrincipalId>;
}

/// The session's [`ScriptSources`].
pub struct ScriptSourceIndex(pub Box<dyn ScriptSources>);

impl Resource for ScriptSourceIndex {}
