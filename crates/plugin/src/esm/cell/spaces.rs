//! Spaces and the load doors between them — the routing graph of cross-cell
//! actor travel (PRD `cross-cell-actor-travel`, `docs/engine/cross-cell-travel.md`).
//!
//! A *space* is an interior cell or a whole worldspace with its exterior cells
//! (CS *SubSpace*: an actor bound for another world space "will assume that he
//! has to go through one or more load doors"). A *load door* is a placed
//! reference carrying an `XTEL`: it leads from its own space into the space of
//! the door it names, landing at the `XTEL` pose. This module builds that
//! graph once per load order and answers two questions: how many doors lie
//! between two spaces (the §8 census), and which doors, one after another, an
//! actor walks through to get from where it stands to a destination
//! (XCT-ROUTE-001 / XCT-ROUTE-003) and how far that walk is (XCT-OFF-001).
//!
//! Both the `inspect cross-space-census` probe and the engine's router read
//! this one graph, so the census measures the routes the engine takes.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{CellRef, EsmCellIndex};
#[cfg(test)]
use super::PlacedRef;

/// Which space a placed reference is in. Interiors are keyed by their
/// lower-cased editor ID, or by the cell map's own key when the cell has none
/// (Fallout 4 leaves some interiors unnamed, and two of them must stay two
/// spaces); worldspaces by their lower-cased worldspace key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpaceKey {
    Interior(String),
    Worldspace(String),
}

impl EsmCellIndex {
    /// The space `refr_form_id` is placed in, by the load order — see
    /// [`SpaceKey`]. `None` for a reference no cell holds.
    pub fn space_of_refr(&self, refr_form_id: u32) -> Option<SpaceKey> {
        Some(match self.locate_refr(refr_form_id)?.0 {
            CellRef::Interior { editor_id } if !editor_id.is_empty() => {
                SpaceKey::Interior(editor_id.to_ascii_lowercase())
            }
            CellRef::Interior { .. } => SpaceKey::Interior(self.interior_cell_key_of_refr(refr_form_id)?.to_string()),
            CellRef::Exterior { worldspace, .. } => SpaceKey::Worldspace(worldspace.to_ascii_lowercase()),
        })
    }
}

/// Dense index of a space in a [`SpaceGraph`].
pub type SpaceId = usize;

/// One load door: a reference in space `from` whose `XTEL` lands in space `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadDoor {
    /// The door reference.
    pub door: u32,
    /// The door it leads to (its `XTEL` destination reference).
    pub partner: u32,
    pub from: SpaceId,
    pub to: SpaceId,
    /// Where an actor walks to pass it (Z-up): the floor in front of the door,
    /// which is the partner's own `XTEL` pose (where the engine places someone
    /// coming the other way), or the door itself when the partner has none.
    pub approach: [f32; 3],
    /// The `XTEL` pose on the far side (Z-up position, Euler radians).
    pub landing: [f32; 3],
    pub landing_rotation: [f32; 3],
}

/// A route through load doors: the doors in walking order and the length of
/// the walk (straight legs from the start to each door's approach, from each
/// landing to the next door, and from the last landing to the goal).
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceRoute {
    /// Indices into [`SpaceGraph::doors`].
    pub doors: Vec<usize>,
    pub length: f32,
}

/// The spaces of a load order and the load doors between them.
#[derive(Debug, Default)]
pub struct SpaceGraph {
    spaces: Vec<SpaceKey>,
    ids: HashMap<SpaceKey, SpaceId>,
    doors: Vec<LoadDoor>,
    /// Per space, the doors leaving it (indices into `doors`).
    leaving: Vec<Vec<usize>>,
    /// Per space, the spaces with a door into it.
    entering: Vec<HashSet<SpaceId>>,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl SpaceGraph {
    /// Build the graph from every placed load door of `cells`. `door_links`
    /// is the set of door references the game's navmesh data links to a
    /// navmesh (`NAVI` `NVCI` on FO3 / FNV, `NAVM` door triangles on Skyrim and
    /// Starfield — [`crate::esm::records::EsmIndex::navmesh_door_links`]): when
    /// it is non-empty, only those doors are routes an actor can walk;
    /// empty (Oblivion, Fallout 4's undecoded navmeshes), every load door is.
    pub fn build(cells: &EsmCellIndex, door_links: &HashSet<u32>) -> Self {
        let mut graph = Self::default();
        let mut seen = HashSet::new();
        let references = cells
            .cells
            .values()
            .chain(cells.exterior_cells.values().flat_map(|grids| grids.values()))
            .chain(cells.worldspace_persistent_cells.values())
            .flat_map(|cell| cell.references.iter());
        for reference in references {
            let Some(teleport) = &reference.teleport else { continue };
            if !seen.insert(reference.form_id) {
                continue;
            }
            if !door_links.is_empty() && !door_links.contains(&reference.form_id) {
                continue;
            }
            let (Some(from), Some(to)) = (cells.space_of_refr(reference.form_id), cells.space_of_refr(teleport.destination))
            else {
                continue;
            };
            if from == to {
                continue;
            }
            let approach = cells
                .locate_refr(teleport.destination)
                .and_then(|(_, partner)| partner.teleport.as_ref())
                .map_or(reference.position, |back| back.position);
            let (from, to) = (graph.intern(from), graph.intern(to));
            graph.leaving[from].push(graph.doors.len());
            graph.entering[to].insert(from);
            graph.doors.push(LoadDoor {
                door: reference.form_id,
                partner: teleport.destination,
                from,
                to,
                approach,
                landing: teleport.position,
                landing_rotation: teleport.rotation,
            });
        }
        graph
    }

    fn intern(&mut self, key: SpaceKey) -> SpaceId {
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.spaces.len();
        self.spaces.push(key.clone());
        self.ids.insert(key, id);
        self.leaving.push(Vec::new());
        self.entering.push(HashSet::new());
        id
    }

    /// The id of `key`, when some load door touches it.
    pub fn id(&self, key: &SpaceKey) -> Option<SpaceId> {
        self.ids.get(key).copied()
    }

    pub fn key(&self, id: SpaceId) -> &SpaceKey {
        &self.spaces[id]
    }

    pub fn doors(&self) -> &[LoadDoor] {
        &self.doors
    }

    pub fn space_count(&self) -> usize {
        self.spaces.len()
    }

    /// Distinct `(from, to)` space pairs some door joins.
    pub fn edge_count(&self) -> usize {
        self.entering.iter().map(HashSet::len).sum()
    }

    /// Hop counts to `to` from every space that reaches it (`to` itself is 0).
    fn hops_to(&self, to: SpaceId) -> HashMap<SpaceId, usize> {
        let mut hops = HashMap::from([(to, 0usize)]);
        let mut queue = VecDeque::from([to]);
        while let Some(at) = queue.pop_front() {
            let next = hops[&at] + 1;
            for &from in &self.entering[at] {
                if !hops.contains_key(&from) {
                    hops.insert(from, next);
                    queue.push_back(from);
                }
            }
        }
        hops
    }

    /// The fewest load doors between two spaces, `None` when no chain of doors
    /// joins them.
    pub fn hops(&self, from: SpaceId, to: SpaceId) -> Option<usize> {
        self.hops_to(to).get(&from).copied()
    }

    /// The route from `start` (Z-up) in space `from` to `goal` in space `to`:
    /// the fewest doors, and at each hop the door nearest to where the actor
    /// stands among those that keep the route that short (XCT-ROUTE-001's
    /// "nearest when several do", applied hop by hop). `None` when no chain
    /// of doors leads there; an empty route when `from == to`.
    pub fn route(&self, from: SpaceId, start: [f32; 3], to: SpaceId, goal: [f32; 3]) -> Option<SpaceRoute> {
        let hops = self.hops_to(to);
        let mut remaining = *hops.get(&from)?;
        let (mut at, mut position) = (from, start);
        let mut route = SpaceRoute { doors: Vec::new(), length: 0.0 };
        while remaining > 0 {
            let &door = self.leaving[at]
                .iter()
                .filter(|&&door| hops.get(&self.doors[door].to) == Some(&(remaining - 1)))
                .min_by(|&&a, &&b| {
                    distance(self.doors[a].approach, position).total_cmp(&distance(self.doors[b].approach, position))
                })?;
            let door_ref = &self.doors[door];
            route.length += distance(door_ref.approach, position);
            route.doors.push(door);
            position = door_ref.landing;
            at = door_ref.to;
            remaining -= 1;
        }
        route.length += distance(position, goal);
        Some(route)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::cell::{CellData, TeleportDest};

    fn door(form_id: u32, at: [f32; 3], to: u32, landing: [f32; 3]) -> PlacedRef {
        let mut door = PlacedRef::at(form_id, 0x31, at, [0.0; 3]);
        door.teleport = Some(TeleportDest { destination: to, position: landing, rotation: [0.0, 0.0, 1.0] });
        door
    }

    fn interior(editor_id: &str, references: Vec<PlacedRef>) -> CellData {
        CellData { editor_id: editor_id.into(), references, ..Default::default() }
    }

    /// Saloon ↔ town (two doors out of the saloon), town ↔ store: the store is
    /// two hops from the saloon.
    fn town() -> EsmCellIndex {
        let mut cells = EsmCellIndex::default();
        cells.cells.insert(
            "saloon".into(),
            interior(
                "Saloon",
                vec![
                    door(0x10, [0.0, 0.0, 0.0], 0x20, [100.0, 0.0, 0.0]),
                    door(0x11, [500.0, 0.0, 0.0], 0x21, [900.0, 0.0, 0.0]),
                ],
            ),
        );
        cells.cells.insert("store".into(), interior("Store", vec![door(0x30, [0.0; 3], 0x22, [2000.0, 0.0, 0.0])]));
        let mut grids = HashMap::new();
        let mut cell = CellData::default();
        cell.references = vec![
            door(0x20, [100.0, 0.0, 0.0], 0x10, [10.0, 0.0, 0.0]),
            door(0x21, [900.0, 0.0, 0.0], 0x11, [490.0, 0.0, 0.0]),
            door(0x22, [2000.0, 0.0, 0.0], 0x30, [5.0, 0.0, 0.0]),
        ];
        grids.insert((0, 0), cell);
        cells.exterior_cells.insert("wasteland".into(), grids);
        cells
    }

    #[test]
    fn a_door_joins_its_space_to_its_partners() {
        let cells = town();
        let graph = SpaceGraph::build(&cells, &HashSet::new());
        assert_eq!(graph.space_count(), 3);
        assert_eq!(graph.doors().len(), 6);
        assert_eq!(graph.edge_count(), 4, "saloon<->wasteland, store<->wasteland");
        let id = |key: SpaceKey| graph.id(&key).unwrap();
        let (saloon, store, wasteland) = (
            id(SpaceKey::Interior("saloon".into())),
            id(SpaceKey::Interior("store".into())),
            id(SpaceKey::Worldspace("wasteland".into())),
        );
        assert_eq!(graph.hops(saloon, wasteland), Some(1));
        assert_eq!(graph.hops(saloon, store), Some(2));
        assert_eq!(graph.hops(store, store), Some(0));
    }

    /// The walk takes the door nearest the actor at each hop and is measured
    /// leg by leg, approaching each door from the floor in front of it.
    #[test]
    fn a_route_takes_the_nearest_door_at_each_hop() {
        let cells = town();
        let graph = SpaceGraph::build(&cells, &HashSet::new());
        let id = |key: SpaceKey| graph.id(&key).unwrap();
        let (saloon, store) = (id(SpaceKey::Interior("saloon".into())), id(SpaceKey::Interior("store".into())));
        let route = graph.route(saloon, [450.0, 0.0, 0.0], store, [0.0, 0.0, 0.0]).unwrap();
        let doors: Vec<u32> = route.doors.iter().map(|&d| graph.doors()[d].door).collect();
        assert_eq!(doors, vec![0x11, 0x22], "the east door (approach 490) beats the west one (approach 10)");
        // 450 -> 490 (40), land at 900, -> door 0x22's approach 2000 (1100), land at 5, -> goal 0 (5).
        assert!((route.length - 1145.0).abs() < 1e-3, "{}", route.length);
        assert_eq!(graph.route(store, [0.0; 3], store, [30.0, 40.0, 0.0]).unwrap().length, 50.0);
    }

    /// With navmesh door links known, a load door no navmesh links is no route.
    #[test]
    fn door_links_restrict_the_walkable_doors() {
        let cells = town();
        let graph = SpaceGraph::build(&cells, &HashSet::from([0x10, 0x20]));
        assert_eq!(graph.doors().len(), 2);
        let id = |key: SpaceKey| graph.id(&key).unwrap();
        assert!(graph.id(&SpaceKey::Interior("store".into())).is_none());
        let route = graph
            .route(id(SpaceKey::Interior("saloon".into())), [450.0, 0.0, 0.0], id(SpaceKey::Worldspace("wasteland".into())), [100.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(graph.doors()[route.doors[0]].door, 0x10, "the only linked door, though farther");
    }

    #[test]
    fn an_unnamed_interior_is_its_own_space() {
        let mut cells = EsmCellIndex::default();
        cells.cells.insert("cell_00000a01".into(), interior("", vec![PlacedRef::at(0x1, 0x2, [0.0; 3], [0.0; 3])]));
        cells.cells.insert("cell_00000a02".into(), interior("", vec![PlacedRef::at(0x3, 0x2, [0.0; 3], [0.0; 3])]));
        assert_eq!(cells.space_of_refr(0x1), Some(SpaceKey::Interior("cell_00000a01".into())));
        assert_eq!(cells.space_of_refr(0x3), Some(SpaceKey::Interior("cell_00000a02".into())));
        assert_eq!(cells.space_of_refr(0x9), None);
    }
}
