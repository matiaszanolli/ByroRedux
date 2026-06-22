//! [`SaveRegistry`] — the type-erased table the save/load drivers walk.
//!
//! Each registered component or resource type contributes a `save`
//! closure (captures `T`, reads it out of the World via `query` /
//! `try_resource`, emits one `serde_json::Value`) and a `load` closure
//! (takes that `Value` back, deserialises, and re-inserts). The drivers
//! never name a concrete `T`; they iterate the registry.
//!
//! The binary builds the registry once at startup with the curated
//! game-state type set. Ordering is preserved for a deterministic
//! [`schema_fingerprint`](SaveRegistry::schema_fingerprint).

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::world::World;
// `SaveRegistry` is itself stored as a World resource so the read-only
// `save` console command can reach it through `&World`.
impl Resource for SaveRegistry {}
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::hash::Hasher;

use crate::SaveError;

type SaveFn = Box<dyn Fn(&World) -> Result<serde_json::Value, SaveError> + Send + Sync>;
type LoadFn = Box<dyn Fn(&mut World, serde_json::Value) -> Result<usize, SaveError> + Send + Sync>;

/// One registered serialisable type (component or resource).
struct Entry {
    name: &'static str,
    save: SaveFn,
    load: LoadFn,
}

/// Registry of every component/resource type that participates in a save.
///
/// Populated by the binary at startup; consumed by [`save_world`] and
/// [`restore_world`].
///
/// [`save_world`]: crate::save_world
/// [`restore_world`]: crate::restore_world
#[derive(Default)]
pub struct SaveRegistry {
    components: Vec<Entry>,
    resources: Vec<Entry>,
}

impl SaveRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a component type for save/load.
    ///
    /// `name` is the **stable on-disk key** — it must not change once a
    /// save format ships (renaming it strands every existing save's
    /// column for that type). It is independent of the Rust type name.
    pub fn register_component<T>(&mut self, name: &'static str) -> &mut Self
    where
        T: Component + Serialize + DeserializeOwned,
    {
        self.components.push(Entry {
            name,
            save: Box::new(move |world: &World| {
                // Bind the query so the borrowed `&T`s outlive the
                // serialise call, then collect one column.
                let Some(q) = world.query::<T>() else {
                    return Ok(serde_json::Value::Array(Vec::new()));
                };
                let rows: Vec<(u32, &T)> = q.iter().collect();
                serde_json::to_value(&rows).map_err(|source| SaveError::Serde {
                    column: name.to_string(),
                    source,
                })
            }),
            load: Box::new(move |world: &mut World, value: serde_json::Value| {
                let rows: Vec<(u32, T)> =
                    serde_json::from_value(value).map_err(|source| SaveError::Serde {
                        column: name.to_string(),
                        source,
                    })?;
                let n = rows.len();
                // `set_next_entity` was already called by the driver, so
                // every original (possibly sparse) entity id passes the
                // `entity < next_entity` guard.
                world.insert_batch::<T, _>(rows);
                Ok(n)
            }),
        });
        self
    }

    /// Register a resource type for save/load.
    ///
    /// Only resources that carry *game state* belong here (e.g.
    /// `ItemInstancePool`). Engine config and per-frame telemetry are
    /// reconstructed, not saved. A missing resource at save time emits no
    /// column; a missing column at load time leaves the live resource
    /// untouched.
    pub fn register_resource<R>(&mut self, name: &'static str) -> &mut Self
    where
        R: Resource + Serialize + DeserializeOwned,
    {
        self.resources.push(Entry {
            name,
            save: Box::new(move |world: &World| {
                let Some(res) = world.try_resource::<R>() else {
                    return Ok(serde_json::Value::Null);
                };
                serde_json::to_value(&*res).map_err(|source| SaveError::Serde {
                    column: name.to_string(),
                    source,
                })
            }),
            load: Box::new(move |world: &mut World, value: serde_json::Value| {
                let res: R = serde_json::from_value(value).map_err(|source| SaveError::Serde {
                    column: name.to_string(),
                    source,
                })?;
                world.insert_resource(res);
                Ok(1)
            }),
        });
        self
    }

    /// Register the [`FormIdComponent`] specially, storing each entity's
    /// **stable** [`FormIdPair`] rather than its session-local
    /// [`FormId`] handle (the handle is a `FormIdPool` index that means
    /// nothing across loads — see the type's own docs).
    ///
    /// Save resolves `FormId → FormIdPair` through the live `FormIdPool`;
    /// load re-interns `FormIdPair → FormId` through the (reloaded) pool,
    /// so the handle is whatever this session assigns — internally
    /// consistent with every other re-interned reference.
    ///
    /// An unresolvable handle at save time is skipped with a warning
    /// rather than aborting the column; it indicates an entity whose pool
    /// entry was already dropped, which a fresh load can't honour anyway.
    pub fn register_form_id_component(&mut self, name: &'static str) -> &mut Self {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool};

        self.components.push(Entry {
            name,
            save: Box::new(move |world: &World| {
                let Some(q) = world.query::<FormIdComponent>() else {
                    return Ok(serde_json::Value::Array(Vec::new()));
                };
                let pool = world.try_resource::<FormIdPool>();
                let mut rows: Vec<(u32, FormIdPair)> = Vec::new();
                for (entity, comp) in q.iter() {
                    match pool.as_ref().and_then(|p| p.resolve(comp.0)).copied() {
                        Some(pair) => rows.push((entity, pair)),
                        None => log::warn!(
                            "save: FormId handle on entity {entity} doesn't resolve in \
                             FormIdPool — skipping (entity will load without a FormIdComponent)"
                        ),
                    }
                }
                serde_json::to_value(&rows).map_err(|source| SaveError::Serde {
                    column: name.to_string(),
                    source,
                })
            }),
            load: Box::new(move |world: &mut World, value: serde_json::Value| {
                let rows: Vec<(u32, FormIdPair)> =
                    serde_json::from_value(value).map_err(|source| SaveError::Serde {
                        column: name.to_string(),
                        source,
                    })?;
                let n = rows.len();
                let resolved: Vec<(u32, FormIdComponent)> = {
                    let mut pool = world.resource_mut::<FormIdPool>();
                    rows.into_iter()
                        .map(|(entity, pair)| (entity, FormIdComponent(pool.intern(pair))))
                        .collect()
                };
                world.insert_batch::<FormIdComponent, _>(resolved);
                Ok(n)
            }),
        });
        self
    }

    /// Fingerprint the registered schema: a stable hash over the ordered
    /// set of component + resource keys.
    ///
    /// Catches the coarse "a type was added / removed / renamed" drift.
    /// It does **not** hash field layout — an intra-type field change is
    /// caught at load time when `serde_json::from_value` fails on the
    /// changed shape. A versioned migrator chain is the follow-up for
    /// graceful intra-type evolution.
    pub fn schema_fingerprint(&self) -> u64 {
        // FNV-1a over the column keys, tagged by kind so a component and
        // a resource sharing a name still produce distinct fingerprints.
        let mut h = FnvHasher::new();
        for e in &self.components {
            h.write(b"C");
            h.write(e.name.as_bytes());
            h.write(b"\0");
        }
        for e in &self.resources {
            h.write(b"R");
            h.write(e.name.as_bytes());
            h.write(b"\0");
        }
        h.finish()
    }

    pub(crate) fn component_entries(&self) -> impl Iterator<Item = (&'static str, &SaveFn, &LoadFn)> {
        self.components
            .iter()
            .map(|e| (e.name, &e.save, &e.load))
    }

    pub(crate) fn resource_entries(&self) -> impl Iterator<Item = (&'static str, &SaveFn, &LoadFn)> {
        self.resources.iter().map(|e| (e.name, &e.save, &e.load))
    }
}

/// Minimal FNV-1a 64-bit hasher — deterministic across runs and builds
/// (unlike `DefaultHasher`, whose `std` implementation is unspecified),
/// which the on-disk schema fingerprint requires.
struct FnvHasher(u64);

impl FnvHasher {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}
