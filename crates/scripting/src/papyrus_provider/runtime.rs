//! Runtime plumbing: the `PapyrusProviderRuntime` resource, its
//! host-supplied resolvers, and world registration.

use super::*;

/// Live catalog and host callback published atomically by the executable.
#[derive(Clone)]
pub struct PapyrusProviderRuntime {
    pub(crate) catalog: Arc<PapyrusProviderCatalog>,
    pub(crate) callback: Option<Arc<PapyrusProviderCallback>>,
    pub(crate) entity_resolver: Option<Arc<PapyrusProviderEntityResolver>>,
    pub(crate) form_resolver: Option<Arc<PapyrusProviderFormResolver>>,
    pub(crate) mod_event_publisher: Option<Arc<PapyrusProviderModEventPublisher>>,
}

impl Resource for PapyrusProviderRuntime {}

impl Default for PapyrusProviderRuntime {
    fn default() -> Self {
        Self {
            catalog: Arc::new(PapyrusProviderCatalog::engine_compatibility()),
            callback: None,
            entity_resolver: None,
            form_resolver: None,
            mod_event_publisher: None,
        }
    }
}

impl PapyrusProviderRuntime {
    /// Immutable manifest-backed alias catalog, whether or not a host is
    /// live. Use [`Self::servable_catalog`] for lowering.
    pub fn catalog(&self) -> Arc<PapyrusProviderCatalog> {
        Arc::clone(&self.catalog)
    }

    /// The catalog **only when the dispatcher could actually serve it** — an
    /// empty catalog otherwise (#3939).
    ///
    /// Lowering and dispatch read two different fields of this resource, and
    /// they can disagree. A fragment lowered against a non-empty catalog
    /// turns `Game.GetModCount()` / `StorageUtil.*` / `Input.*` / `UI.*`
    /// into a provider *barrier* instead of declining; at dispatch,
    /// `DeferredFragmentEffects::apply` finds no callback and drops every
    /// barrier **and its tail** — after the prefix has already mutated quest
    /// state. `Game.GetModByName("X.esp"); Self.SetStage(20)` then advances
    /// nothing past the barrier and never declines: a quest stuck
    /// mid-fragment with a `warn!`, where pre-seam the same fragment
    /// declined wholesale, inert and consistent.
    ///
    /// Enforced here, at the read, rather than by fixing each producer of
    /// the inconsistent state. There are at least fifteen: `Default`
    /// (a non-empty `engine_compatibility()` catalog with `callback: None`),
    /// and the thirteen `?` exits in `load_requested_extensions` that return
    /// before `sync_extension_script_function_invoker` ever publishes a
    /// callback — a startup error `App::new` logs and continues past. One
    /// invariant at the seam covers every one of them, including the next
    /// one added.
    ///
    /// An empty catalog is equivalent to lowering with no providers at all:
    /// `lower_provider_call` matches nothing, so the statement is unclaimed
    /// and the fragment declines at the boundary, which is the consistent
    /// behaviour.
    pub fn servable_catalog(&self) -> Arc<PapyrusProviderCatalog> {
        if self.callback.is_some() {
            Arc::clone(&self.catalog)
        } else {
            Arc::new(PapyrusProviderCatalog::default())
        }
    }

    /// Clone the live host callback for guard-free execution.
    pub fn callback(&self) -> Option<Arc<PapyrusProviderCallback>> {
        self.callback.clone()
    }

    pub fn entity_resolver(&self) -> Option<Arc<PapyrusProviderEntityResolver>> {
        self.entity_resolver.clone()
    }

    pub fn form_resolver(&self) -> Option<Arc<PapyrusProviderFormResolver>> {
        self.form_resolver.clone()
    }

    pub fn mod_event_publisher(&self) -> Option<Arc<PapyrusProviderModEventPublisher>> {
        self.mod_event_publisher.clone()
    }
}

/// Install the executable's opaque-handle converter independently of the
/// provider callback so test and headless runtimes can omit reference payloads.
pub fn set_papyrus_provider_entity_resolver(
    world: &World,
    resolver: Option<Arc<PapyrusProviderEntityResolver>>,
) {
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusProviderRuntime>() {
        runtime.entity_resolver = resolver;
    }
}

/// Install the executable's stable authored-form converter independently of
/// the provider callback so built-in compatibility routes can use it too.
pub fn set_papyrus_provider_form_resolver(
    world: &World,
    resolver: Option<Arc<PapyrusProviderFormResolver>>,
) {
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusProviderRuntime>() {
        runtime.form_resolver = resolver;
    }
}

/// Install the executable's shared ModEvent publisher independently of static
/// provider dispatch so headless runtimes can intentionally omit event I/O.
pub fn set_papyrus_provider_mod_event_publisher(
    world: &World,
    publisher: Option<Arc<PapyrusProviderModEventPublisher>>,
) {
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusProviderRuntime>() {
        runtime.mod_event_publisher = publisher;
    }
}

/// Replace the live Papyrus provider surface after extension activation.
pub fn set_papyrus_provider_runtime(
    world: &World,
    catalog: Arc<PapyrusProviderCatalog>,
    callback: Option<Arc<PapyrusProviderCallback>>,
) {
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusProviderRuntime>() {
        runtime.catalog = catalog;
        runtime.callback = callback;
    }
}

/// Register the provider runtime resource before any extension is loaded.
pub(crate) fn register(world: &mut World) {
    world.insert_resource(PapyrusProviderRuntime::default());
    world.insert_resource(PapyrusProviderContinuationQueue::default());
    world.insert_resource(PapyrusModEventRuntime::default());
    world.register::<PapyrusProviderProgram>();
}
