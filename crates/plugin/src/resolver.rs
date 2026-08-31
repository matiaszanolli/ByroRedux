//! Dependency DAG and conflict resolution.
//!
//! Builds a directed acyclic graph from plugin dependency declarations
//! and uses it to determine which plugin "wins" when multiple plugins
//! provide or modify the same record.
//!
//! Resolution rules:
//! 1. If plugin A transitively depends on plugin B, A wins (intentional
//!    override — `DepthResolved`).
//! 2. If neither depends on the other, the plugin with the lower
//!    [`PluginId`] (UUID lexicographic order) wins, and the conflict is
//!    flagged as `TieBreak` for user review.

use byroredux_core::form_id::PluginId;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::Hash;

use crate::manifest::PluginManifest;

/// Outcome of resolving a conflict between multiple plugins that touch
/// the same [`FormIdPair`](byroredux_core::form_id::FormIdPair).
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictResolution {
    /// The winner transitively depends on the loser — intentional override.
    DepthResolved { winner: PluginId },
    /// No dependency relationship exists — deterministic tiebreak by UUID,
    /// but flagged for user review.
    TieBreak { winner: PluginId },
    /// The user explicitly chose a winner.
    UserResolved { winner: PluginId },
}

/// Dependency DAG built from plugin manifests.
///
/// Adjacency is stored as `plugin → [its direct dependencies]`.
pub struct DependencyResolver {
    graph: DependencyGraph<PluginId>,
}

impl DependencyResolver {
    /// Build the DAG from a slice of manifests.
    pub fn new(manifests: &[PluginManifest]) -> Self {
        let graph = DependencyGraph::new(manifests.iter().map(|m| (m.id, m.dependencies.clone())));
        Self { graph }
    }

    /// Compute the full transitive dependency set for a plugin (BFS).
    pub fn transitive_deps(&self, plugin: PluginId) -> HashSet<PluginId> {
        self.graph.transitive_deps(&plugin)
    }

    /// Given a set of plugins that all touch the same record, determine
    /// which one wins and how the conflict was resolved.
    ///
    /// If any plugin transitively depends on another in the set, the
    /// deepest dependent wins (`DepthResolved`). Otherwise, the lowest
    /// `PluginId` wins (`TieBreak`).
    pub fn resolve_winner(&self, plugins: &[PluginId]) -> (PluginId, ConflictResolution) {
        assert!(
            !plugins.is_empty(),
            "resolve_winner called with empty slice"
        );

        if plugins.len() == 1 {
            return (
                plugins[0],
                ConflictResolution::DepthResolved { winner: plugins[0] },
            );
        }

        // For each plugin, check if it transitively depends on any other
        // plugin in the conflict set. The one that depends on the most
        // others is the "deepest" — it wins.
        let mut best: Option<(PluginId, usize)> = None;

        for &candidate in plugins {
            let deps = self.transitive_deps(candidate);
            let overlap = plugins
                .iter()
                .filter(|&&p| p != candidate && deps.contains(&p))
                .count();

            if let Some((_, best_overlap)) = best {
                if overlap > best_overlap {
                    best = Some((candidate, overlap));
                }
            } else {
                best = Some((candidate, overlap));
            }
        }

        let (winner, overlap) = best.unwrap();

        if overlap > 0 {
            // Winner depends on at least one other plugin in the set —
            // this is an intentional override.
            (winner, ConflictResolution::DepthResolved { winner })
        } else {
            // No dependency relationship — deterministic tiebreak.
            let winner = *plugins.iter().min().unwrap();
            (winner, ConflictResolution::TieBreak { winner })
        }
    }
}

/// Shared dependency primitive for record ancestry and executable-extension
/// activation. Edges point from a node to its direct dependencies.
pub(crate) struct DependencyGraph<K> {
    adjacency: HashMap<K, Vec<K>>,
}

impl<K> DependencyGraph<K>
where
    K: Clone + Eq + Hash + Ord,
{
    pub(crate) fn new(edges: impl IntoIterator<Item = (K, Vec<K>)>) -> Self {
        Self {
            adjacency: edges.into_iter().collect(),
        }
    }

    pub(crate) fn transitive_deps(&self, node: &K) -> HashSet<K> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        if let Some(direct) = self.adjacency.get(node) {
            for dependency in direct {
                queue.push_back(dependency.clone());
            }
        }

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(deps) = self.adjacency.get(&current) {
                for dependency in deps {
                    if !visited.contains(dependency) {
                        queue.push_back(dependency.clone());
                    }
                }
            }
        }

        visited
    }

    /// Produce a deterministic dependency-first order or the first stable
    /// cycle path. Every dependency is expected to be present as a node.
    pub(crate) fn dependency_order(&self) -> Result<Vec<K>, Vec<K>> {
        let mut dependency_count = HashMap::with_capacity(self.adjacency.len());
        let mut dependents: HashMap<K, Vec<K>> = HashMap::new();
        for (node, dependencies) in &self.adjacency {
            dependency_count.insert(node.clone(), dependencies.len());
            for dependency in dependencies {
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .push(node.clone());
            }
        }

        let mut ready: BTreeSet<K> = dependency_count
            .iter()
            .filter_map(|(node, &count)| (count == 0).then_some(node.clone()))
            .collect();
        let mut ordered = Vec::with_capacity(self.adjacency.len());
        while let Some(node) = ready.pop_first() {
            ordered.push(node.clone());
            if let Some(children) = dependents.get_mut(&node) {
                children.sort();
                for child in children.iter() {
                    let count = dependency_count
                        .get_mut(child)
                        .expect("dependent must be an input node");
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(child.clone());
                    }
                }
            }
        }
        if ordered.len() == self.adjacency.len() {
            return Ok(ordered);
        }

        // Every node left after Kahn's algorithm depends on another remaining
        // node. Walk the lexically first such edge until a node repeats to
        // produce a deterministic cycle diagnostic without recursive stack
        // growth on hostile high-count manifests.
        let remaining: BTreeSet<K> = dependency_count
            .iter()
            .filter_map(|(node, &count)| (count != 0).then_some(node.clone()))
            .collect();
        let mut positions = HashMap::new();
        let mut path = Vec::new();
        let mut current = remaining
            .first()
            .expect("an incomplete order must leave at least one node")
            .clone();
        loop {
            if let Some(&start) = positions.get(&current) {
                let mut cycle = path[start..].to_vec();
                cycle.push(current);
                return Err(cycle);
            }
            positions.insert(current.clone(), path.len());
            path.push(current.clone());
            current = self.adjacency[&current]
                .iter()
                .filter(|dependency| remaining.contains(*dependency))
                .min()
                .expect("remaining node must depend on another remaining node")
                .clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str, deps: &[&str]) -> PluginManifest {
        PluginManifest {
            id: PluginId::from_filename(name),
            name: name.to_string(),
            version: semver::Version::new(1, 0, 0),
            dependencies: deps.iter().map(|d| PluginId::from_filename(d)).collect(),
        }
    }

    #[test]
    fn transitive_deps_single_chain() {
        // C depends on B, B depends on A
        let manifests = vec![
            manifest("A.esm", &[]),
            manifest("B.esm", &["A.esm"]),
            manifest("C.esm", &["B.esm"]),
        ];
        let resolver = DependencyResolver::new(&manifests);

        let c_deps = resolver.transitive_deps(PluginId::from_filename("C.esm"));
        assert!(c_deps.contains(&PluginId::from_filename("B.esm")));
        assert!(c_deps.contains(&PluginId::from_filename("A.esm")));
        assert_eq!(c_deps.len(), 2);

        let a_deps = resolver.transitive_deps(PluginId::from_filename("A.esm"));
        assert!(a_deps.is_empty());
    }

    #[test]
    fn depth_resolved_winner() {
        // B depends on A, both touch the same record → B wins
        let manifests = vec![manifest("A.esm", &[]), manifest("B.esm", &["A.esm"])];
        let resolver = DependencyResolver::new(&manifests);

        let plugins = vec![
            PluginId::from_filename("A.esm"),
            PluginId::from_filename("B.esm"),
        ];
        let (winner, resolution) = resolver.resolve_winner(&plugins);

        assert_eq!(winner, PluginId::from_filename("B.esm"));
        assert!(matches!(
            resolution,
            ConflictResolution::DepthResolved { .. }
        ));
    }

    #[test]
    fn tiebreak_no_dependency() {
        // A and B are independent — tiebreak by UUID order
        let manifests = vec![manifest("A.esm", &[]), manifest("B.esm", &[])];
        let resolver = DependencyResolver::new(&manifests);

        let plugins = vec![
            PluginId::from_filename("A.esm"),
            PluginId::from_filename("B.esm"),
        ];
        let (winner, resolution) = resolver.resolve_winner(&plugins);

        // Winner is whichever has the lower PluginId (UUID-based)
        let expected = *plugins.iter().min().unwrap();
        assert_eq!(winner, expected);
        assert!(matches!(resolution, ConflictResolution::TieBreak { .. }));
    }

    #[test]
    fn three_way_chain_deepest_wins() {
        // C → B → A, all touch the same record → C wins
        let manifests = vec![
            manifest("A.esm", &[]),
            manifest("B.esm", &["A.esm"]),
            manifest("C.esm", &["B.esm"]),
        ];
        let resolver = DependencyResolver::new(&manifests);

        let plugins = vec![
            PluginId::from_filename("A.esm"),
            PluginId::from_filename("B.esm"),
            PluginId::from_filename("C.esm"),
        ];
        let (winner, resolution) = resolver.resolve_winner(&plugins);

        assert_eq!(winner, PluginId::from_filename("C.esm"));
        assert!(matches!(
            resolution,
            ConflictResolution::DepthResolved { .. }
        ));
    }

    #[test]
    fn tiebreak_is_deterministic() {
        let manifests = vec![manifest("X.esm", &[]), manifest("Y.esm", &[])];
        let resolver = DependencyResolver::new(&manifests);

        let plugins = vec![
            PluginId::from_filename("X.esm"),
            PluginId::from_filename("Y.esm"),
        ];

        // Call twice — must produce the same winner
        let (w1, _) = resolver.resolve_winner(&plugins);
        let (w2, _) = resolver.resolve_winner(&plugins);
        assert_eq!(w1, w2);
    }
}
