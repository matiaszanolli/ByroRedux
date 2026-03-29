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
use std::collections::{HashMap, HashSet, VecDeque};

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
    adjacency: HashMap<PluginId, Vec<PluginId>>,
}

impl DependencyResolver {
    /// Build the DAG from a slice of manifests.
    pub fn new(manifests: &[PluginManifest]) -> Self {
        let adjacency = manifests
            .iter()
            .map(|m| (m.id, m.dependencies.clone()))
            .collect();
        Self { adjacency }
    }

    /// Compute the full transitive dependency set for a plugin (BFS).
    pub fn transitive_deps(&self, plugin: PluginId) -> HashSet<PluginId> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        if let Some(direct) = self.adjacency.get(&plugin) {
            for &dep in direct {
                queue.push_back(dep);
            }
        }

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(deps) = self.adjacency.get(&current) {
                for &dep in deps {
                    if !visited.contains(&dep) {
                        queue.push_back(dep);
                    }
                }
            }
        }

        visited
    }

    /// Given a set of plugins that all touch the same record, determine
    /// which one wins and how the conflict was resolved.
    ///
    /// If any plugin transitively depends on another in the set, the
    /// deepest dependent wins (`DepthResolved`). Otherwise, the lowest
    /// `PluginId` wins (`TieBreak`).
    pub fn resolve_winner(&self, plugins: &[PluginId]) -> (PluginId, ConflictResolution) {
        assert!(!plugins.is_empty(), "resolve_winner called with empty slice");

        if plugins.len() == 1 {
            return (
                plugins[0],
                ConflictResolution::DepthResolved {
                    winner: plugins[0],
                },
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
            (
                winner,
                ConflictResolution::DepthResolved { winner },
            )
        } else {
            // No dependency relationship — deterministic tiebreak.
            let winner = *plugins.iter().min().unwrap();
            (
                winner,
                ConflictResolution::TieBreak { winner },
            )
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
        let manifests = vec![
            manifest("A.esm", &[]),
            manifest("B.esm", &["A.esm"]),
        ];
        let resolver = DependencyResolver::new(&manifests);

        let plugins = vec![
            PluginId::from_filename("A.esm"),
            PluginId::from_filename("B.esm"),
        ];
        let (winner, resolution) = resolver.resolve_winner(&plugins);

        assert_eq!(winner, PluginId::from_filename("B.esm"));
        assert!(matches!(resolution, ConflictResolution::DepthResolved { .. }));
    }

    #[test]
    fn tiebreak_no_dependency() {
        // A and B are independent — tiebreak by UUID order
        let manifests = vec![
            manifest("A.esm", &[]),
            manifest("B.esm", &[]),
        ];
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
        assert!(matches!(resolution, ConflictResolution::DepthResolved { .. }));
    }

    #[test]
    fn tiebreak_is_deterministic() {
        let manifests = vec![
            manifest("X.esm", &[]),
            manifest("Y.esm", &[]),
        ];
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
