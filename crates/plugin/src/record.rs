//! Records: component bundles loaded from plugins.
//!
//! A [`Record`] is an entity template — a collection of components that
//! can be spawned into the [`World`]. Each record carries a stable
//! [`FormIdPair`] identity and a [`RecordType`] tag.
//!
//! Component data is type-erased via [`ErasedComponentData`] so that
//! records can hold arbitrary component types without the record type
//! itself being generic.

use gamebyro_core::ecs::components::FormIdComponent;
use gamebyro_core::ecs::storage::{Component, EntityId};
use gamebyro_core::ecs::world::World;
use gamebyro_core::form_id::{FormIdPair, FormIdPool};
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
        self.components.insert(
            TypeId::of::<T>(),
            Box::new(ErasedComponent { data }),
        );
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
/// # use gamebyro_plugin::RecordType;
/// assert_eq!(RecordType::WEAP.as_str(), "WEAP");
/// assert_eq!(RecordType::from_str("NPC_"), RecordType::NPC_);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordType(pub [u8; 4]);

impl RecordType {
    // ── WorldObjects ────────────────────────────────────────────────────
    pub const STAT: Self = Self(*b"STAT");
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
    pub const ADDN: Self = Self(*b"ADDN");
    pub const ARTO: Self = Self(*b"ARTO");
    pub const MATO: Self = Self(*b"MATO");
    pub const HAZD: Self = Self(*b"HAZD");

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
    use gamebyro_core::ecs::components::Transform;
    use gamebyro_core::ecs::sparse_set::SparseSetStorage;
    use gamebyro_core::form_id::{LocalFormId, PluginId};
    use gamebyro_core::math::Vec3;

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
}
