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
        //
        // #4082 — collect EVERY candidate at the max overlap, not just the
        // first one seen. Keeping only the first-seen winner broke ties at
        // the max by `plugins` slice order — plugin registration order,
        // precisely the load-order dependence this tier exists to remove.
        // Two mods that both depend on the same base master and both edit
        // one record are a diamond with overlap 1 each; the pre-fix code
        // reported that as an intentional `DepthResolved` override decided
        // by registration order, and the genuine ambiguity never reached
        // `TieBreak` for review (that arm only fired when the max overlap
        // across the whole set was 0).
        let mut best_overlap = 0usize;
        let mut best_candidates: Vec<PluginId> = Vec::new();

        for &candidate in plugins {
            let deps = self.transitive_deps(candidate);
            let overlap = plugins
                .iter()
                .filter(|&&p| p != candidate && deps.contains(&p))
                .count();

            match overlap.cmp(&best_overlap) {
                std::cmp::Ordering::Greater => {
                    best_overlap = overlap;
                    best_candidates.clear();
                    best_candidates.push(candidate);
                }
                std::cmp::Ordering::Equal => best_candidates.push(candidate),
                std::cmp::Ordering::Less => {}
            }
        }

        if best_overlap > 0 && best_candidates.len() == 1 {
            // Exactly one plugin reaches the max overlap and it's a real
            // dependency relationship — an intentional override, not a tie.
            let winner = best_candidates[0];
            (winner, ConflictResolution::DepthResolved { winner })
        } else {
            // Either no dependency relationship exists at all (max overlap
            // 0), or more than one candidate is tied at the max — order-
            // independent deterministic tiebreak by PluginId (UUID
            // lexicographic order), flagged for user review either way.
            let winner = *best_candidates
                .iter()
                .min()
                .expect("best_candidates is non-empty: every plugin in a non-empty slice \
                         contributes at least one (candidate, overlap) pair");
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

    /// Regression for #4082. B and C both depend on A (a diamond) and
    /// neither depends on the other — within the conflict set {A, B, C},
    /// B and C are TIED at overlap 1 (each depends on exactly one other
    /// set member, A), while A has overlap 0. Pre-fix, `resolve_winner`
    /// kept only the first-seen max, so whichever of B/C came first in
    /// `plugins` (== `plugins` slice order == plugin registration order)
    /// won as an "intentional" `DepthResolved` override — the load-order
    /// dependence this whole tier exists to remove. The genuine ambiguity
    /// must surface as `TieBreak`, and the winner must be order-independent
    /// (`min(PluginId)`, not "whichever appeared first").
    #[test]
    fn diamond_dependency_ties_surface_as_tiebreak_not_depth_resolved() {
        let manifests = vec![
            manifest("A.esm", &[]),
            manifest("B.esm", &["A.esm"]),
            manifest("C.esm", &["A.esm"]),
        ];
        let resolver = DependencyResolver::new(&manifests);
        let a = PluginId::from_filename("A.esm");
        let b = PluginId::from_filename("B.esm");
        let c = PluginId::from_filename("C.esm");
        let expected_winner = b.min(c);

        // Try both registration orders for the tied pair — the winner and
        // resolution kind must not depend on which one is listed first.
        for plugins in [vec![a, b, c], vec![a, c, b]] {
            let (winner, resolution) = resolver.resolve_winner(&plugins);
            assert_eq!(
                winner, expected_winner,
                "winner must be min(PluginId) among the tied candidates, \
                 order-independent of {plugins:?}"
            );
            assert!(
                matches!(resolution, ConflictResolution::TieBreak { .. }),
                "a genuine tie at the max overlap must surface as TieBreak \
                 for review, not DepthResolved (got {resolution:?} for {plugins:?})"
            );
        }
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
