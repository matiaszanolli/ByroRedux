//! Crafting at a workbench — Skyrim `COBJ` recipes (M43).
//!
//! A recipe (`COBJ`) names the item it creates (`CNAM`), how many (`NAM1`), the
//! ingredients (`CNTO`), the bench keyword it is crafted at (`BNAM`) and the
//! conditions the crafter must meet (`CTDA`, the perk gates). A bench is a
//! placed `FURN` whose base carries that keyword among its `KWDA` (the forge's
//! `CraftingSmithingForge`, the cookpot's `CraftingCookpot`, …).
//!
//! Crafting from the bench: list the recipes whose keyword the bench carries
//! ([`recipes_at`]), show which of them the player's ingredients cover, and
//! [`craft`] one — the ingredients leave the inventory, the created item joins
//! it, and the Story Manager's `CRFT` (Craft Item) is raised with the bench in
//! `R1` and the session location in `L1`
//! (`OnStoryCraftItem(akBench, akLocation, akCreatedItem)`).
//!
//! Crafting at a forge raises Smithing: UESP *Skyrim:Smithing* gives the XP of
//! smithing an item as `3 × item value^0.65 + 25` ([`smithing_xp`]), in the
//! skill's usage units, so it banks through the same `AdvanceSkill` path a script
//! uses (`PendingSkillAdvances`). The same page names the benches that give none:
//! a smelter and a tanning rack ("does not give skill increases"); only the
//! forges (`CraftingSmithingForge`, `CraftingSmithingSkyforge`) are credited here.
//!
//! Not modelled: the perk-improved yield (Arcane Blacksmith's enchanted
//! tempering, Steel Smithing's armor-rating bonus — both act on *improving* an
//! item, which is a different action from crafting one), and the crafting animation. Fallout 4's component-based `COBJ` (`FVPA`) is a
//! different recipe format and reads as no ingredients, so it offers nothing.

use crate::inventory::InventoryCatalog;
use crate::systems::PlayerEntity;
use byroredux_core::ecs::components::Inventory;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::ecs::Resource;
use byroredux_plugin::esm::records::condition::ConditionList;
use byroredux_plugin::esm::records::EsmIndex;
use std::collections::HashMap;
use std::sync::Arc;

/// One craftable recipe.
#[derive(Debug, Clone)]
pub(crate) struct Recipe {
    pub(crate) form_id: u32,
    pub(crate) output: u32,
    pub(crate) output_count: u32,
    pub(crate) ingredients: Vec<(u32, u32)>,
    /// The `BNAM` bench keyword.
    pub(crate) bench_keyword: u32,
    pub(crate) conditions: ConditionList,
}

/// Every recipe of the load order and the keywords of every furniture base.
/// `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub(crate) struct CraftingBook {
    recipes: Arc<Vec<Recipe>>,
    bench_keywords: Arc<HashMap<u32, Vec<u32>>>,
    /// Furniture bases that carry a keyword some recipe is crafted at.
    bench_bases: Arc<std::collections::HashSet<u32>>,
    /// The bench keywords whose crafts raise Smithing ([`SMITHING_FORGE_KEYWORDS`]).
    forge_keywords: Arc<std::collections::HashSet<u32>>,
}

/// The editor ids of the bench keywords at which crafting gives Smithing XP.
const SMITHING_FORGE_KEYWORDS: [&str; 2] = ["CraftingSmithingForge", "CraftingSmithingSkyforge"];

/// The skill a forge craft raises.
const SMITHING_SKILL: &str = "Smithing";

/// UESP *Skyrim:Smithing*, "Crafting Items": the skill XP of smithing one item is
/// `3 × item value^0.65 + 25`.
pub(crate) fn smithing_xp(item_value: u32) -> f32 {
    3.0 * (item_value as f32).powf(0.65) + 25.0
}

impl Resource for CraftingBook {}

impl CraftingBook {
    pub(crate) fn from_index(index: &EsmIndex) -> Self {
        let mut recipes: Vec<Recipe> = index
            .recipes
            .values()
            // A recipe with no ingredients, output or bench is a stub of another game's format.
            .filter(|cobj| cobj.created_form != 0 && cobj.workbench_form != 0 && !cobj.components.is_empty())
            .map(|cobj| Recipe {
                form_id: cobj.form_id,
                output: cobj.created_form,
                output_count: cobj.created_count.max(1),
                ingredients: cobj.components.clone(),
                bench_keyword: cobj.workbench_form,
                conditions: cobj.conditions.clone(),
            })
            .collect();
        recipes.sort_by_key(|recipe| recipe.form_id);
        let crafted_at: std::collections::HashSet<u32> = recipes.iter().map(|recipe| recipe.bench_keyword).collect();
        let bench_bases = index
            .furniture_keywords
            .iter()
            .filter(|(_, keywords)| keywords.iter().any(|keyword| crafted_at.contains(keyword)))
            .map(|(base, _)| *base)
            .collect();
        let forge_keywords = index
            .keywords
            .values()
            .filter(|keyword| SMITHING_FORGE_KEYWORDS.contains(&keyword.editor_id.as_str()))
            .map(|keyword| keyword.form_id)
            .collect();
        Self {
            recipes: Arc::new(recipes),
            bench_keywords: Arc::new(index.furniture_keywords.clone()),
            bench_bases: Arc::new(bench_bases),
            forge_keywords: Arc::new(forge_keywords),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.recipes.len()
    }

    /// Whether a furniture base carries a keyword some recipe is crafted at.
    pub(crate) fn is_bench_base(&self, base: u32) -> bool {
        self.bench_bases.contains(&base)
    }
}

pub(crate) fn install_crafting(world: &mut World, index: &EsmIndex) -> usize {
    let book = CraftingBook::from_index(index);
    let count = book.len();
    world.insert_resource(book);
    count
}

/// The base form of a placed reference (the bench's furniture record).
fn base_of(world: &World, entity: EntityId) -> Option<u32> {
    world
        .get::<byroredux_scripting::SceneAliasCandidate>(entity)
        .map(|identity| identity.base_form_id)
}

/// One recipe as the crafting page lists it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RecipeView {
    pub(crate) form_id: u32,
    pub(crate) name: String,
    pub(crate) output_count: u32,
    /// `(ingredient name, needed, held)`.
    pub(crate) ingredients: Vec<(String, u32, u32)>,
    pub(crate) craftable: bool,
}

fn item_name(world: &World, form_id: u32) -> String {
    world
        .try_resource::<InventoryCatalog>()
        .and_then(|catalog| catalog.item_name(form_id).map(str::to_owned))
        .unwrap_or_else(|| format!("Item {form_id:08X}"))
}

/// Whether the player meets a recipe's conditions. A list with a function the evaluator does not
/// model passes (the ambient-package rule: an undecidable gate is not a refusal).
fn conditions_met(world: &World, player: EntityId, conditions: &ConditionList) -> bool {
    if conditions.is_empty() {
        return true;
    }
    let context = byroredux_scripting::condition::ConditionContext::for_subject(player);
    if !conditions.iter().all(|condition| byroredux_scripting::condition::is_modeled(world, condition)) {
        return true;
    }
    byroredux_scripting::condition::evaluate(conditions, world, &context)
}

/// The recipes `bench` offers the player, in form-id order, with what the player holds of each
/// ingredient. A recipe whose conditions the player fails is not listed at all (the CK hides it).
pub(crate) fn recipes_at(world: &World, bench: EntityId) -> Vec<RecipeView> {
    let Some(player) = PlayerEntity::get(world) else {
        return Vec::new();
    };
    let Some(book) = world.try_resource::<CraftingBook>() else {
        return Vec::new();
    };
    let Some(keywords) = base_of(world, bench).and_then(|base| book.bench_keywords.get(&base).cloned()) else {
        return Vec::new();
    };
    let held = |item: u32| world.get::<Inventory>(player).map_or(0, |inventory| inventory.count_of(item));
    book.recipes
        .iter()
        .filter(|recipe| keywords.contains(&recipe.bench_keyword))
        .filter(|recipe| conditions_met(world, player, &recipe.conditions))
        .map(|recipe| {
            let ingredients: Vec<(String, u32, u32)> = recipe
                .ingredients
                .iter()
                .map(|&(item, need)| (item_name(world, item), need, held(item)))
                .collect();
            RecipeView {
                form_id: recipe.form_id,
                name: item_name(world, recipe.output),
                output_count: recipe.output_count,
                craftable: ingredients.iter().all(|(_, need, have)| have >= need),
                ingredients,
            }
        })
        .collect()
}

/// Craft `recipe_form` at `bench`: the ingredients leave the player's inventory, the created item
/// joins it, and `CRFT` is raised. The answer is the line to show the player, or why it was refused.
pub(crate) fn craft(world: &World, recipe_form: u32, bench: EntityId) -> Result<String, String> {
    if !crate::systems::player_can_act(world) {
        return Err("you cannot craft now".to_owned());
    }
    let player = PlayerEntity::get(world).ok_or("no player")?;
    let (recipe, keywords, forge) = {
        let book = world.try_resource::<CraftingBook>().ok_or("no recipes are loaded")?;
        let recipe = book
            .recipes
            .iter()
            .find(|recipe| recipe.form_id == recipe_form)
            .cloned()
            .ok_or_else(|| format!("no recipe {recipe_form:08X}"))?;
        let keywords = base_of(world, bench)
            .and_then(|base| book.bench_keywords.get(&base).cloned())
            .unwrap_or_default();
        let forge = book.forge_keywords.contains(&recipe.bench_keyword);
        (recipe, keywords, forge)
    };
    if !keywords.contains(&recipe.bench_keyword) {
        return Err("this bench does not make that".to_owned());
    }
    if !conditions_met(world, player, &recipe.conditions) {
        return Err("you cannot craft that yet".to_owned());
    }
    let held = |item: u32| world.get::<Inventory>(player).map_or(0, |inventory| inventory.count_of(item));
    if let Some(&(item, need)) = recipe.ingredients.iter().find(|&&(item, need)| held(item) < need) {
        return Err(format!("not enough {} ({} of {need})", item_name(world, item), held(item)));
    }
    for &(item, need) in &recipe.ingredients {
        byroredux_scripting::fragment::remove_items(world, player, item, need, None);
    }
    crate::inventory::add_item(world, recipe.output, recipe.output_count)
        .ok_or("the crafted item could not be added")?;
    let name = item_name(world, recipe.output);
    if forge {
        let value = world.try_resource::<InventoryCatalog>().map_or(0, |catalog| catalog.item_value(recipe.output));
        if let Some(mut pending) = world.try_resource_mut::<byroredux_scripting::PendingSkillAdvances>() {
            pending.0.push((SMITHING_SKILL.to_owned(), smithing_xp(value)));
        }
    }
    crate::systems::story_events::raise_craft_story_event(world, player, bench);
    let line = if recipe.output_count > 1 {
        format!("Crafted {name} ({})", recipe.output_count)
    } else {
        format!("Crafted {name}")
    };
    crate::notifications::push(world, line.clone());
    Ok(line)
}

/// The bench whose page is open, if any. `NOT_SAVED_BY_DESIGN`: a menu session.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ActiveCrafting(pub(crate) Option<EntityId>);

impl Resource for ActiveCrafting {}

/// A bench the player activated, waiting for the app to open its page. `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CraftingRequest(pub(crate) Option<EntityId>);

impl Resource for CraftingRequest {}

/// Ask for the crafting page at `bench` (the activation of a bench).
pub(crate) fn request_crafting(world: &World, bench: EntityId) {
    if let Some(mut request) = world.try_resource_mut::<CraftingRequest>() {
        request.0 = Some(bench);
    }
}

pub(crate) fn take_crafting_request(world: &World) -> Option<EntityId> {
    world.try_resource_mut::<CraftingRequest>()?.0.take()
}

/// Open a session at `bench`; refused when it offers nothing.
pub(crate) fn begin_crafting(world: &World, bench: EntityId) -> Result<(), String> {
    if recipes_at(world, bench).is_empty() {
        return Err("this bench offers no recipes".to_owned());
    }
    let mut active = world.try_resource_mut::<ActiveCrafting>().ok_or("no crafting session slot")?;
    active.0 = Some(bench);
    Ok(())
}

pub(crate) fn end_crafting(world: &World) {
    if let Some(mut active) = world.try_resource_mut::<ActiveCrafting>() {
        active.0 = None;
    }
}

/// The page's snapshot while a session is open.
pub(crate) fn snapshot(world: &World) -> Option<byroredux_debug_ui::CraftingSnapshot> {
    let bench = world.try_resource::<ActiveCrafting>()?.0?;
    let bench_name = base_of(world, bench)
        .and_then(|base| {
            world
                .try_resource::<crate::cell_loader::LoadedCellIndex>()
                .and_then(|index| index.0.cells.statics.get(&base).map(|object| object.editor_id.clone()))
        })
        .map(|name| bench_display_name(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Workbench".to_owned());
    let recipes = recipes_at(world, bench)
        .into_iter()
        .map(|recipe| byroredux_debug_ui::CraftingRecipeView {
            form_id: recipe.form_id,
            name: recipe.name,
            count: recipe.output_count,
            ingredients: recipe.ingredients,
            craftable: recipe.craftable,
        })
        .collect();
    Some(byroredux_debug_ui::CraftingSnapshot { bench_name, recipes })
}

/// A bench's editor id as a heading: `CraftingTanningRackMarker` -> `Tanning Rack` (the `Crafting`
/// prefix and `Marker` suffix are authoring words; camel-case words are split).
fn bench_display_name(editor_id: &str) -> String {
    let trimmed = editor_id.strip_prefix("Crafting").unwrap_or(editor_id);
    let trimmed = trimmed.strip_suffix("Marker").unwrap_or(trimmed);
    let mut out = String::new();
    for (i, ch) in trimmed.chars().enumerate() {
        if i > 0 && ch.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Apply the page's action at the open bench.
pub(crate) fn apply_action(world: &World, action: byroredux_debug_ui::CraftingAction) -> Result<String, String> {
    let byroredux_debug_ui::CraftingAction::Craft { recipe } = action;
    let bench = world
        .try_resource::<ActiveCrafting>()
        .and_then(|active| active.0)
        .ok_or("no bench is open")?;
    craft(world, recipe, bench)
}

/// The placed bench nearest the player, for the console door.
pub(crate) fn nearest_bench(world: &World) -> Option<EntityId> {
    let player = PlayerEntity::get(world)?;
    let here = world.get::<byroredux_core::ecs::components::GlobalTransform>(player)?.translation;
    let book = world.try_resource::<CraftingBook>()?;
    let candidates: Vec<EntityId> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()?
        .iter()
        .filter(|(_, identity)| book.is_bench_base(identity.base_form_id))
        .map(|(entity, _)| entity)
        .collect();
    drop(book);
    candidates
        .into_iter()
        .filter_map(|entity| {
            let at = world.get::<byroredux_core::ecs::components::GlobalTransform>(entity)?.translation;
            Some((entity, (at - here).length()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(entity, _)| entity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::ItemStack;
    use byroredux_plugin::esm::records::CobjRecord;

    const FORGE_KW: u32 = 0x88105;
    const COOK_KW: u32 = 0xA5CB3;
    const FORGE: u32 = 0x1000;
    const COOKPOT: u32 = 0x1001;
    const IRON_INGOT: u32 = 0x5ACE4;
    const LEATHER: u32 = 0x800E4;
    const IRON_DAGGER: u32 = 0x12EB7;
    const RECIPE: u32 = 0x2000;

    fn world_with_bench() -> (World, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<Inventory>();
        world.register::<byroredux_scripting::SceneAliasCandidate>();
        let mut index = EsmIndex::default();
        index.recipes.insert(
            RECIPE,
            CobjRecord {
                form_id: RECIPE,
                created_form: IRON_DAGGER,
                workbench_form: FORGE_KW,
                components: vec![(IRON_INGOT, 1), (LEATHER, 1)],
                created_count: 1,
                ..Default::default()
            },
        );
        index.furniture_keywords.insert(FORGE, vec![FORGE_KW]);
        index.furniture_keywords.insert(COOKPOT, vec![COOK_KW]);
        for (form_id, editor_id) in [(FORGE_KW, "CraftingSmithingForge"), (COOK_KW, "CraftingCookpot")] {
            index.keywords.insert(form_id, byroredux_plugin::esm::records::KywdRecord { form_id, editor_id: editor_id.into(), attraction_rule: 0 });
        }
        crate::crafting::install_crafting(&mut world, &index);
        world.insert_resource(crate::notifications::PlayerNotifications::default());
        let player = world.spawn();
        world.insert(player, Inventory::new());
        world.insert_resource(PlayerEntity(Some(player)));
        let place = |world: &mut World, base: u32| {
            let bench = world.spawn();
            world.insert(
                bench,
                byroredux_scripting::SceneAliasCandidate {
                    reference_form_id: 0x9000 + base,
                    base_form_id: base,
                    linked_refs: Vec::new(),
                    location_ref_types: Vec::new(),
                },
            );
            bench
        };
        let forge = place(&mut world, FORGE);
        let cookpot = place(&mut world, COOKPOT);
        (world, player, forge, cookpot)
    }

    fn give(world: &mut World, player: EntityId, item: u32, count: u32) {
        world.get_mut::<Inventory>(player).unwrap().push(ItemStack::new(item, count));
    }

    #[test]
    fn bench_editor_ids_read_as_headings() {
        assert_eq!(bench_display_name("CraftingTanningRackMarker"), "Tanning Rack");
        assert_eq!(bench_display_name("CraftingSmithingForge"), "Smithing Forge");
        assert_eq!(bench_display_name("Skyforge"), "Skyforge");
    }

    /// The page's own action crafts at the open bench and refuses with none open.
    #[test]
    fn the_pages_craft_action_works_at_the_open_bench_only() {
        let (mut world, player, forge, _cookpot) = world_with_bench();
        world.insert_resource(ActiveCrafting::default());
        give(&mut world, player, IRON_INGOT, 1);
        give(&mut world, player, LEATHER, 1);
        let action = byroredux_debug_ui::CraftingAction::Craft { recipe: RECIPE };
        assert!(apply_action(&world, action).is_err(), "no bench open");
        begin_crafting(&world, forge).unwrap();
        assert!(apply_action(&world, action).is_ok());
        assert_eq!(world.get::<Inventory>(player).unwrap().count_of(IRON_DAGGER), 1);
        end_crafting(&world);
        assert!(snapshot(&world).is_none());
    }

    /// Only a bench that carries the recipe's keyword offers it, a recipe shows what is held, and a
    /// craft consumes exactly the ingredients, yields the item and raises `CRFT` with the bench in R1.
    #[test]
    fn crafting_consumes_ingredients_yields_the_item_and_raises_craft_item() {
        let (mut world, player, forge, cookpot) = world_with_bench();
        assert!(world.resource::<CraftingBook>().is_bench_base(FORGE));
        assert_eq!(recipes_at(&world, forge).len(), 1);
        assert!(recipes_at(&world, cookpot).is_empty(), "the cookpot does not make daggers");

        give(&mut world, player, IRON_INGOT, 3);
        let listed = &recipes_at(&world, forge)[0];
        assert!(!listed.craftable, "no leather yet");
        assert_eq!(listed.ingredients.iter().map(|(_, need, have)| (*need, *have)).collect::<Vec<_>>(), vec![(1, 3), (1, 0)]);
        assert!(craft(&world, RECIPE, forge).unwrap_err().contains("not enough"));
        assert_eq!(world.get::<Inventory>(player).unwrap().count_of(IRON_INGOT), 3, "a refusal takes nothing");

        give(&mut world, player, LEATHER, 1);
        assert!(craft(&world, RECIPE, cookpot).is_err(), "wrong bench");
        assert!(craft(&world, RECIPE, forge).is_ok());
        let inventory = world.get::<Inventory>(player).unwrap();
        assert_eq!((inventory.count_of(IRON_INGOT), inventory.count_of(LEATHER), inventory.count_of(IRON_DAGGER)), (2, 0, 1));
        drop(inventory);
        let event = world
            .query::<byroredux_scripting::story_manager::StoryEvent>()
            .and_then(|events| events.iter().map(|(_, e)| *e).find(|e| e.mnemonic == *b"CRFT"))
            .expect("CRFT raised");
        assert_eq!(event.slots().reference_1, Some(forge));
    }

    /// UESP *Skyrim:Smithing* works two examples: an iron dagger (value 10) gives 38.4 XP and iron armor
    /// (value 125) gives 94.2; every item carries the flat 25.
    #[test]
    fn smithing_xp_follows_the_uesp_formula() {
        assert!((smithing_xp(10) - 38.4).abs() < 0.05, "the page's iron dagger example");
        assert!((smithing_xp(125) - 94.2).abs() < 0.05, "the page's iron armor example");
        assert!((smithing_xp(0) - 25.0).abs() < f32::EPSILON);
    }

    /// A forge craft queues Smithing XP for the item's value; a craft at any other bench queues none.
    #[test]
    fn a_forge_craft_banks_smithing_xp_and_other_benches_do_not() {
        let (mut world, player, forge, _cookpot) = world_with_bench();
        world.insert_resource(crate::inventory::InventoryCatalog::with_pickup_bases(&[IRON_DAGGER]));
        world.insert_resource(byroredux_scripting::PendingSkillAdvances::default());
        give(&mut world, player, IRON_INGOT, 1);
        give(&mut world, player, LEATHER, 1);
        craft(&world, RECIPE, forge).unwrap();
        let queued = world.resource::<byroredux_scripting::PendingSkillAdvances>().0.clone();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].0, "Smithing");
        assert!((queued[0].1 - smithing_xp(1)).abs() < 1e-4, "the catalog's value for the dagger is 1");
    }
}
