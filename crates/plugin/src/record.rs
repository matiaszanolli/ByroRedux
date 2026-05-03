//! Records: component bundles loaded from plugins.
//!
//! A [`Record`] is an entity template — a collection of components that
//! can be spawned into the [`World`]. Each record carries a stable
//! [`FormIdPair`] identity and a [`RecordType`] tag.
//!
//! Component data is type-erased via [`ErasedComponentData`] so that
//! records can hold arbitrary component types without the record type
//! itself being generic.

use byroredux_core::ecs::components::FormIdComponent;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::form_id::{FormIdPair, FormIdPool};
use std::any::TypeId;
use std::collections::HashMap;

// ── Type-erased component data ──────────────────────────────────────────

/// Object-safe trait for inserting a component into a [`World`] without
/// knowing its concrete type at the call site.
pub trait ErasedComponentData: Send + Sync {
    /// Insert this component onto `entity` in `world`.
    fn insert_into(&self, world: &mut World, entity: EntityId);

    /// The concrete component type's [`TypeId`].
    fn type_id(&self) -> TypeId;
}

/// Concrete wrapper that implements [`ErasedComponentData`] for any
/// component type. One wrapper per `T` — this is the bridge between
/// the generic component world and the type-erased record storage.
pub struct ErasedComponent<T: Component + Clone> {
    pub data: T,
}

impl<T: Component + Clone + Send + Sync + 'static> ErasedComponentData for ErasedComponent<T> {
    fn insert_into(&self, world: &mut World, entity: EntityId) {
        world.insert(entity, self.data.clone());
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

// ── Record ──────────────────────────────────────────────────────────────

/// An entity template loaded from a plugin.
///
/// Holds a stable identity ([`FormIdPair`]), a type tag, and a bag of
/// type-erased components. Call [`spawn`](Self::spawn) to materialise
/// the record as a live entity in the [`World`].
pub struct Record {
    pub form_id: FormIdPair,
    pub record_type: RecordType,
    pub components: HashMap<TypeId, Box<dyn ErasedComponentData>>,
}

impl Record {
    pub fn new(form_id: FormIdPair, record_type: RecordType) -> Self {
        Self {
            form_id,
            record_type,
            components: HashMap::new(),
        }
    }

    /// Add a component to this record template.
    ///
    /// If a component of the same type was already present, it is replaced.
    pub fn add_component<T: Component + Clone + Send + Sync + 'static>(&mut self, data: T) {
        self.components
            .insert(TypeId::of::<T>(), Box::new(ErasedComponent { data }));
    }

    /// Spawn this record as a live entity in the [`World`].
    ///
    /// 1. Allocates a new entity via `world.spawn()`
    /// 2. Interns the record's [`FormIdPair`] into the [`FormIdPool`]
    ///    resource and attaches a [`FormIdComponent`] so the entity is
    ///    findable via [`World::find_by_form_id`]
    /// 3. Inserts all component data from the template
    ///
    /// # Panics
    /// Panics if the [`FormIdPool`] resource has not been inserted into
    /// the world.
    pub fn spawn(&self, world: &mut World) -> EntityId {
        let entity = world.spawn();

        // Intern the stable identity and attach it as a component.
        let form_id = {
            let mut pool = world.resource_mut::<FormIdPool>();
            pool.intern(self.form_id)
        };
        world.insert(entity, FormIdComponent(form_id));

        // Insert all template components.
        for component in self.components.values() {
            component.insert_into(world, entity);
        }

        entity
    }
}

// ── RecordType ──────────────────────────────────────────────────────────

/// 4-byte record type code matching the ESM/ESP binary format.
///
/// Used for filtering and display — not for dispatch. The actual data
/// lives in the component bundle.
///
/// Common types have named constants, but any `[u8; 4]` is valid —
/// unknown types from future games or mods work without changes.
///
/// ```
/// # use byroredux_plugin::RecordType;
/// assert_eq!(RecordType::WEAP.as_str(), "WEAP");
/// assert_eq!(RecordType::from_str("NPC_"), RecordType::NPC_);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordType(pub [u8; 4]);

impl RecordType {
    // ── WorldObjects ────────────────────────────────────────────────────
    pub const STAT: Self = Self(*b"STAT");
    pub const SCOL: Self = Self(*b"SCOL");
    pub const MSTT: Self = Self(*b"MSTT");
    pub const DOOR: Self = Self(*b"DOOR");
    pub const FURN: Self = Self(*b"FURN");
    pub const ACTI: Self = Self(*b"ACTI");
    pub const TACT: Self = Self(*b"TACT");
    pub const CONT: Self = Self(*b"CONT");
    pub const FLOR: Self = Self(*b"FLOR");
    pub const TREE: Self = Self(*b"TREE");
    pub const GRAS: Self = Self(*b"GRAS");
    pub const LIGH: Self = Self(*b"LIGH");
    pub const IDLM: Self = Self(*b"IDLM");
    pub const BNDS: Self = Self(*b"BNDS");
    pub const PKIN: Self = Self(*b"PKIN");
    pub const MOVS: Self = Self(*b"MOVS");
    pub const ADDN: Self = Self(*b"ADDN");
    pub const ARTO: Self = Self(*b"ARTO");
    pub const MATO: Self = Self(*b"MATO");
    pub const HAZD: Self = Self(*b"HAZD");
    pub const TERM: Self = Self(*b"TERM");

    // ── Items ───────────────────────────────────────────────────────────
    pub const WEAP: Self = Self(*b"WEAP");
    pub const ARMO: Self = Self(*b"ARMO");
    pub const ARMA: Self = Self(*b"ARMA");
    pub const AMMO: Self = Self(*b"AMMO");
    pub const BOOK: Self = Self(*b"BOOK");
    pub const NOTE: Self = Self(*b"NOTE");
    pub const KEYM: Self = Self(*b"KEYM");
    pub const MISC: Self = Self(*b"MISC");
    pub const ALCH: Self = Self(*b"ALCH");
    pub const INGR: Self = Self(*b"INGR");
    pub const CMPO: Self = Self(*b"CMPO");
    pub const COBJ: Self = Self(*b"COBJ");
    pub const FLST: Self = Self(*b"FLST");
    pub const LVLI: Self = Self(*b"LVLI");
    pub const OMOD: Self = Self(*b"OMOD");
    pub const OTFT: Self = Self(*b"OTFT");
    pub const MSWP: Self = Self(*b"MSWP");

    // ── Actors ──────────────────────────────────────────────────────────
    pub const NPC_: Self = Self(*b"NPC_");
    /// Pre-FO4 creature record. Folded into NPC_ on FO4+ but Oblivion /
    /// FO3 / FNV / Skyrim still ship it as a distinct record family.
    /// Game-invariant for [`render_layer`]: maps to [`RenderLayer::Actor`]
    /// regardless of which game emitted it.
    pub const CREA: Self = Self(*b"CREA");
    pub const RACE: Self = Self(*b"RACE");
    pub const LVLN: Self = Self(*b"LVLN");
    pub const CLAS: Self = Self(*b"CLAS");
    pub const FACT: Self = Self(*b"FACT");
    pub const PACK: Self = Self(*b"PACK");
    pub const CSTY: Self = Self(*b"CSTY");
    pub const MOVT: Self = Self(*b"MOVT");
    pub const BPTD: Self = Self(*b"BPTD");
    pub const HDPT: Self = Self(*b"HDPT");
    pub const EQUP: Self = Self(*b"EQUP");
    pub const RELA: Self = Self(*b"RELA");

    // ── WorldData ───────────────────────────────────────────────────────
    pub const CELL: Self = Self(*b"CELL");
    pub const WRLD: Self = Self(*b"WRLD");
    pub const CLMT: Self = Self(*b"CLMT");
    pub const WTHR: Self = Self(*b"WTHR");
    pub const LCTN: Self = Self(*b"LCTN");
    pub const LCRT: Self = Self(*b"LCRT");
    pub const ECZN: Self = Self(*b"ECZN");
    pub const WATR: Self = Self(*b"WATR");
    pub const LTEX: Self = Self(*b"LTEX");
    pub const TXST: Self = Self(*b"TXST");
    pub const LGTM: Self = Self(*b"LGTM");
    pub const LAYR: Self = Self(*b"LAYR");
    pub const LSCR: Self = Self(*b"LSCR");

    // ── Magic ───────────────────────────────────────────────────────────
    pub const SPEL: Self = Self(*b"SPEL");
    pub const ENCH: Self = Self(*b"ENCH");
    pub const MGEF: Self = Self(*b"MGEF");
    pub const LVSP: Self = Self(*b"LVSP");
    pub const PERK: Self = Self(*b"PERK");
    pub const DUAL: Self = Self(*b"DUAL");
    pub const EXPL: Self = Self(*b"EXPL");
    pub const PROJ: Self = Self(*b"PROJ");

    // ── Audio ───────────────────────────────────────────────────────────
    pub const SOUN: Self = Self(*b"SOUN");
    pub const SNDR: Self = Self(*b"SNDR");
    pub const SNCT: Self = Self(*b"SNCT");
    pub const SOPM: Self = Self(*b"SOPM");
    pub const MUSC: Self = Self(*b"MUSC");
    pub const MUST: Self = Self(*b"MUST");
    pub const ASPC: Self = Self(*b"ASPC");
    pub const REVB: Self = Self(*b"REVB");
    pub const AECH: Self = Self(*b"AECH");

    // ── Character / Misc ────────────────────────────────────────────────
    pub const AVIF: Self = Self(*b"AVIF");
    pub const AACT: Self = Self(*b"AACT");
    pub const GLOB: Self = Self(*b"GLOB");
    pub const KYWD: Self = Self(*b"KYWD");
    pub const DMGT: Self = Self(*b"DMGT");
    pub const CLFM: Self = Self(*b"CLFM");
    pub const DFOB: Self = Self(*b"DFOB");
    pub const MESG: Self = Self(*b"MESG");
    pub const QUST: Self = Self(*b"QUST");
    pub const VTYP: Self = Self(*b"VTYP");

    // ── Special Effects ─────────────────────────────────────────────────
    pub const EFSH: Self = Self(*b"EFSH");
    pub const RFCT: Self = Self(*b"RFCT");
    pub const SPGD: Self = Self(*b"SPGD");
    pub const IMAD: Self = Self(*b"IMAD");

    // ── Placed references (not base objects, but found in cells) ────────
    pub const REFR: Self = Self(*b"REFR");
    pub const ACHR: Self = Self(*b"ACHR");
    pub const PGRE: Self = Self(*b"PGRE");
    pub const PMIS: Self = Self(*b"PMIS");
    pub const PHZD: Self = Self(*b"PHZD");

    /// Classify this record type into a [`RenderLayer`] for the
    /// renderer's per-layer depth-bias ladder. Game-invariant — the
    /// mapping is the same across Oblivion / FO3 / FNV / Skyrim / FO4
    /// / FO76 / Starfield, even when the record itself is only emitted
    /// by a subset of games (CREA pre-FO4, SCOL/PKIN/MOVS FO4+).
    ///
    /// Layer assignment per
    /// `crates/core/src/ecs/components/render_layer.rs`:
    ///
    /// * **Architecture** — large fixtures the level designer placed
    ///   at fixed Y. Owns the depth buffer (zero bias). Includes
    ///   wall-mounted lamps (LIGH), built-in containers (CONT —
    ///   footlockers, safes), wall/desk-mounted terminals (TERM),
    ///   activators (ACTI — vending machines, levers), and the FO4+
    ///   collection records (SCOL/PKIN/MOVS).
    /// * **Clutter** — player-pickup-able small items resting on
    ///   architecture. Need a tiny depth bias to win the coplanar
    ///   z-fight against the surface beneath (papers on desks, ammo
    ///   on shelves).
    /// * **Actor** — NPCs and pre-FO4 creatures. Their feet plant on
    ///   floors at exactly the floor's Y; without bias every standing
    ///   actor z-fights the floor at the foot-plant patch.
    /// * **Default fallback** — Architecture (zero bias). Reached only
    ///   for record types that shouldn't appear in `EsmCellIndex.statics`
    ///   but might be passed in by an unforeseen caller; safe inert
    ///   value.
    ///
    /// The "lay-flat overlay" decal escalation (alpha-tested rugs,
    /// NIF-flagged blood splats, etc.) is **not** handled here — it
    /// lives at the cell-loader spawn site as
    /// `mesh.is_decal || mesh.alpha_test_func != 0` →
    /// [`RenderLayer::Decal`], which overrides whatever this method
    /// returns for the base record. See `byroredux/src/cell_loader.rs`.
    pub const fn render_layer(&self) -> byroredux_core::ecs::components::RenderLayer {
        use byroredux_core::ecs::components::RenderLayer;
        match *self {
            // Architecture — large fixtures, ground rooted, wall/desk
            // mounted. Zero bias.
            Self::STAT
            | Self::MSTT
            | Self::FURN
            | Self::DOOR
            | Self::FLOR
            | Self::TREE
            | Self::IDLM
            | Self::BNDS
            | Self::ADDN
            | Self::TACT
            | Self::ACTI
            | Self::CONT
            | Self::LIGH
            | Self::TERM
            | Self::SCOL
            | Self::PKIN
            | Self::MOVS => RenderLayer::Architecture,
            // Clutter — player-pickup-able small items.
            Self::WEAP
            | Self::ARMO
            | Self::AMMO
            | Self::MISC
            | Self::KEYM
            | Self::ALCH
            | Self::INGR
            | Self::BOOK
            | Self::NOTE => RenderLayer::Clutter,
            // Actors — NPC_ all games, CREA pre-FO4.
            Self::NPC_ | Self::CREA => RenderLayer::Actor,
            // Anything else (unknown FourCC, non-renderable record
            // type that nonetheless reached this caller) → safe
            // inert default.
            _ => RenderLayer::Architecture,
        }
    }

    /// Create from a 4-character ASCII string.
    ///
    /// # Panics
    /// Panics if `s` is not exactly 4 bytes.
    pub fn from_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        assert_eq!(bytes.len(), 4, "RecordType must be exactly 4 ASCII bytes");
        Self([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    /// View as a string slice (always 4 ASCII bytes).
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("????")
    }
}

impl std::fmt::Debug for RecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RecordType({})", self.as_str())
    }
}

impl std::fmt::Display for RecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::Transform;
    use byroredux_core::ecs::sparse_set::SparseSetStorage;
    use byroredux_core::form_id::{LocalFormId, PluginId};
    use byroredux_core::math::Vec3;

    fn test_pair() -> FormIdPair {
        FormIdPair {
            plugin: PluginId::from_filename("TestPlugin.esm"),
            local: LocalFormId(0x100),
        }
    }

    // A simple test component
    #[derive(Debug, Clone, PartialEq)]
    struct Damage(pub f32);
    impl Component for Damage {
        type Storage = SparseSetStorage<Self>;
    }

    #[test]
    fn record_add_component_and_spawn() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let mut record = Record::new(test_pair(), RecordType::WEAP);
        record.add_component(Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));
        record.add_component(Damage(50.0));

        let entity = record.spawn(&mut world);

        // Transform present
        let t = world.get::<Transform>(entity).unwrap();
        assert_eq!(t.translation, Vec3::new(1.0, 2.0, 3.0));

        // Damage present
        let d = world.get::<Damage>(entity).unwrap();
        assert_eq!(d.0, 50.0);
    }

    #[test]
    fn spawn_inserts_form_id_component() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let pair = test_pair();
        let record = Record::new(pair, RecordType::STAT);
        let entity = record.spawn(&mut world);

        // FormIdComponent is present
        let fid_comp = world.get::<FormIdComponent>(entity).unwrap();

        // Resolve back through the pool — must match original pair
        let pool = world.resource::<FormIdPool>();
        let resolved = pool.resolve(fid_comp.0).unwrap();
        assert_eq!(*resolved, pair);
    }

    #[test]
    fn spawned_entity_findable_by_form_id() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let pair = test_pair();
        let record = Record::new(pair, RecordType::NPC_);
        let entity = record.spawn(&mut world);

        // Look up the runtime FormId from the pool
        let pool = world.resource::<FormIdPool>();
        let fid = pool.get(&pair).unwrap();
        drop(pool);

        assert_eq!(world.find_by_form_id(fid), Some(entity));
    }

    #[test]
    fn add_component_replaces_same_type() {
        let mut record = Record::new(test_pair(), RecordType::WEAP);
        record.add_component(Damage(10.0));
        record.add_component(Damage(99.0));

        assert_eq!(record.components.len(), 1);

        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let entity = record.spawn(&mut world);

        let d = world.get::<Damage>(entity).unwrap();
        assert_eq!(d.0, 99.0);
    }

    // ── RecordType FourCC tests ─────────────────────────────────────────

    #[test]
    fn record_type_as_str() {
        assert_eq!(RecordType::WEAP.as_str(), "WEAP");
        assert_eq!(RecordType::NPC_.as_str(), "NPC_");
        assert_eq!(RecordType::CELL.as_str(), "CELL");
    }

    #[test]
    fn record_type_from_str() {
        assert_eq!(RecordType::from_str("WEAP"), RecordType::WEAP);
        assert_eq!(RecordType::from_str("STAT"), RecordType::STAT);
        assert_eq!(RecordType::from_str("CELL"), RecordType::CELL);
    }

    #[test]
    fn record_type_unknown_type_works() {
        let custom = RecordType::from_str("XYZW");
        assert_eq!(custom.as_str(), "XYZW");
        assert_ne!(custom, RecordType::WEAP);
    }

    #[test]
    fn record_type_equality() {
        assert_eq!(RecordType(*b"WEAP"), RecordType::WEAP);
        assert_ne!(RecordType::WEAP, RecordType::ARMO);
    }

    #[test]
    fn record_type_debug_display() {
        let rt = RecordType::DOOR;
        assert_eq!(format!("{rt}"), "DOOR");
        assert_eq!(format!("{rt:?}"), "RecordType(DOOR)");
    }

    #[test]
    fn record_type_hash_works() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(RecordType::WEAP);
        set.insert(RecordType::ARMO);
        set.insert(RecordType::WEAP); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    #[should_panic(expected = "exactly 4")]
    fn record_type_from_str_wrong_length_panics() {
        RecordType::from_str("AB");
    }

    // ── #renderlayer — RecordType::render_layer() classification table ──
    //
    // Game-invariant classification. Per the user-confirmed taxonomy:
    //
    //   Architecture: STAT, MSTT, FURN, DOOR, FLOR, TREE, IDLM, BNDS,
    //                 ADDN, TACT, ACTI, CONT, LIGH, TERM, SCOL, PKIN, MOVS
    //   Clutter:      WEAP, ARMO, AMMO, MISC, KEYM, ALCH, INGR, BOOK, NOTE
    //   Actor:        NPC_, CREA
    //   Default:      Architecture (zero-bias inert fallback)
    //
    // The Decal layer is set at the cell-loader spawn site via the
    // `mesh.is_decal || mesh.alpha_test_func != 0` escalation rule —
    // not via this classifier — so no RecordType maps to Decal.

    use byroredux_core::ecs::components::RenderLayer;

    fn assert_layer(rt: RecordType, expected: RenderLayer) {
        assert_eq!(
            rt.render_layer(),
            expected,
            "RecordType {rt} should classify as {expected:?}"
        );
    }

    #[test]
    fn render_layer_architecture_record_types() {
        for rt in [
            RecordType::STAT,
            RecordType::MSTT,
            RecordType::FURN,
            RecordType::DOOR,
            RecordType::FLOR,
            RecordType::TREE,
            RecordType::IDLM,
            RecordType::BNDS,
            RecordType::ADDN,
            RecordType::TACT,
            RecordType::ACTI,
            RecordType::CONT,
            RecordType::LIGH,
            RecordType::TERM,
            RecordType::SCOL,
            RecordType::PKIN,
            RecordType::MOVS,
        ] {
            assert_layer(rt, RenderLayer::Architecture);
        }
    }

    #[test]
    fn render_layer_clutter_record_types() {
        for rt in [
            RecordType::WEAP,
            RecordType::ARMO,
            RecordType::AMMO,
            RecordType::MISC,
            RecordType::KEYM,
            RecordType::ALCH,
            RecordType::INGR,
            RecordType::BOOK,
            RecordType::NOTE,
        ] {
            assert_layer(rt, RenderLayer::Clutter);
        }
    }

    #[test]
    fn render_layer_actor_record_types() {
        // CREA is pre-FO4 only but the classifier is game-invariant;
        // both NPC_ and CREA always map to Actor.
        assert_layer(RecordType::NPC_, RenderLayer::Actor);
        assert_layer(RecordType::CREA, RenderLayer::Actor);
    }

    #[test]
    fn render_layer_unknown_falls_back_to_architecture() {
        // Unknown FourCC — defensive default. Zero-bias means "no
        // visual change vs. pre-#renderlayer" for any caller that
        // accidentally passes a non-renderable record type.
        assert_layer(RecordType::from_str("XYZW"), RenderLayer::Architecture);
        // Records that exist in the constants table but aren't in the
        // classifier match arms (e.g. ENCH, GLOB) also default to
        // Architecture — safe inert.
        assert_layer(RecordType::ENCH, RenderLayer::Architecture);
        assert_layer(RecordType::GLOB, RenderLayer::Architecture);
        assert_layer(RecordType::CELL, RenderLayer::Architecture);
    }
}
