//! Vector clock implementation for tracking causal ordering of concurrent
//! modifications to medical records.
//!
//! Each record maintains a vector clock where entries map provider addresses
//! (represented as `u32` node identifiers) to monotonically increasing counters.
//! When two clocks are compared, the relationship is one of:
//! - **Equal** – identical histories.
//! - **Before / After** – one causally precedes the other.
//! - **Concurrent** – the modifications happened independently and may conflict.

use soroban_sdk::{contracttype, Env, Map};

/// Maximum number of distinct nodes (providers) tracked per record clock.
/// Keeps storage bounded in a smart-contract environment.
pub const MAX_VECTOR_CLOCK_NODES: u32 = 64;

/// Outcome of comparing two vector clocks.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClockOrdering {
    /// The two clocks are identical.
    Equal,
    /// The left clock causally precedes the right clock.
    Before,
    /// The left clock causally follows the right clock.
    After,
    /// The clocks are concurrent – neither precedes the other.
    Concurrent,
}

/// A vector clock stored on-chain as a map from node ID to counter.
///
/// Node IDs are compact `u32` identifiers assigned externally (e.g. derived
/// from a provider registry counter) to avoid storing full `Address` values
/// inside the clock.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorClock {
    pub entries: Map<u32, u64>,
}

impl VectorClock {
    /// Creates an empty vector clock.
    pub fn new(env: &Env) -> Self {
        Self {
            entries: Map::new(env),
        }
    }

    /// Increments the counter for `node_id` by one, returning the new value.
    ///
    /// If the clock already tracks [`MAX_VECTOR_CLOCK_NODES`] distinct nodes
    /// and `node_id` is not among them, the increment is ignored and `0` is
    /// returned to signal the limit was hit.
    pub fn increment(&mut self, env: &Env, node_id: u32) -> u64 {
        let current = self.entries.get(node_id).unwrap_or(0);

        // Enforce node-count cap for new entries.
        if current == 0 && self.entries.len() >= MAX_VECTOR_CLOCK_NODES {
            return 0;
        }

        let next = current.saturating_add(1);
        self.entries.set(node_id, next);
        let _ = env; // kept for future diagnostics
        next
    }

    /// Returns the counter value for a given node, or `0` if absent.
    pub fn get(&self, node_id: u32) -> u64 {
        self.entries.get(node_id).unwrap_or(0)
    }

    /// Merges `other` into `self` by taking the component-wise maximum.
    ///
    /// After merging, `self` dominates both its prior state and `other`.
    pub fn merge(&mut self, other: &VectorClock) {
        for node_id in other.entries.keys() {
            let other_val = other.entries.get(node_id).unwrap_or(0);
            let self_val = self.entries.get(node_id).unwrap_or(0);
            if other_val > self_val {
                self.entries.set(node_id, other_val);
            }
        }
    }

    /// Compares two vector clocks to determine causal ordering.
    pub fn compare(&self, other: &VectorClock) -> ClockOrdering {
        let mut self_has_greater = false;
        let mut other_has_greater = false;

        // Check all entries present in self.
        for node_id in self.entries.keys() {
            let s = self.entries.get(node_id).unwrap_or(0);
            let o = other.entries.get(node_id).unwrap_or(0);
            if s > o {
                self_has_greater = true;
            } else if o > s {
                other_has_greater = true;
            }
            if self_has_greater && other_has_greater {
                return ClockOrdering::Concurrent;
            }
        }

        // Check entries present only in other.
        for node_id in other.entries.keys() {
            if self.entries.get(node_id).is_some() {
                continue; // already compared above
            }
            // self implicitly has 0 for this node
            let o = other.entries.get(node_id).unwrap_or(0);
            if o > 0 {
                other_has_greater = true;
            }
            if self_has_greater && other_has_greater {
                return ClockOrdering::Concurrent;
            }
        }

        match (self_has_greater, other_has_greater) {
            (false, false) => ClockOrdering::Equal,
            (true, false) => ClockOrdering::After,
            (false, true) => ClockOrdering::Before,
            (true, true) => ClockOrdering::Concurrent,
        }
    }

    /// Returns `true` when `self` dominates (is equal to or after) `other`.
    pub fn dominates(&self, other: &VectorClock) -> bool {
        matches!(
            self.compare(other),
            ClockOrdering::Equal | ClockOrdering::After
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_clocks_are_equal() {
        let env = Env::default();
        let a = VectorClock::new(&env);
        let b = VectorClock::new(&env);
        assert_eq!(a.compare(&b), ClockOrdering::Equal);
    }

    #[test]
    fn single_increment_is_after_empty() {
        let env = Env::default();
        let mut a = VectorClock::new(&env);
        a.increment(&env, 1);
        let b = VectorClock::new(&env);
        assert_eq!(a.compare(&b), ClockOrdering::After);
        assert_eq!(b.compare(&a), ClockOrdering::Before);
    }

    #[test]
    fn concurrent_detection() {
        let env = Env::default();
        let mut a = VectorClock::new(&env);
        let mut b = VectorClock::new(&env);
        a.increment(&env, 1);
        b.increment(&env, 2);
        assert_eq!(a.compare(&b), ClockOrdering::Concurrent);
    }

    #[test]
    fn merge_produces_dominating_clock() {
        let env = Env::default();
        let mut a = VectorClock::new(&env);
        let mut b = VectorClock::new(&env);
        a.increment(&env, 1);
        a.increment(&env, 1);
        b.increment(&env, 2);
        b.increment(&env, 2);
        b.increment(&env, 2);

        let mut merged = a.clone();
        merged.merge(&b);
        assert!(merged.dominates(&a));
        assert!(merged.dominates(&b));
        assert_eq!(merged.get(1), 2);
        assert_eq!(merged.get(2), 3);
    }

    #[test]
    fn node_cap_prevents_unbounded_growth() {
        let env = Env::default();
        let mut clock = VectorClock::new(&env);
        for i in 0..MAX_VECTOR_CLOCK_NODES {
            assert!(clock.increment(&env, i) > 0);
        }
        // One more distinct node should be rejected.
        assert_eq!(clock.increment(&env, MAX_VECTOR_CLOCK_NODES), 0);
        // Existing node still works.
        assert_eq!(clock.increment(&env, 0), 2);
    }

    #[test]
    fn causality_tracking_across_multiple_actors() {
        let env = Env::default();
        let mut clock_a = VectorClock::new(&env);
        clock_a.increment(&env, 1);

        let mut clock_b = clock_a.clone();
        clock_b.increment(&env, 2);

        let mut clock_c = clock_b.clone();
        clock_c.increment(&env, 3);

        // Transitive causal ordering: A < B < C
        assert_eq!(clock_a.compare(&clock_b), ClockOrdering::Before);
        assert_eq!(clock_b.compare(&clock_c), ClockOrdering::Before);
        assert_eq!(clock_a.compare(&clock_c), ClockOrdering::Before);

        // Reverse checks: C > B > A
        assert_eq!(clock_c.compare(&clock_b), ClockOrdering::After);
        assert_eq!(clock_b.compare(&clock_a), ClockOrdering::After);
        assert_eq!(clock_c.compare(&clock_a), ClockOrdering::After);

        // Domination checks
        assert!(clock_c.dominates(&clock_b));
        assert!(clock_c.dominates(&clock_a));
        assert!(clock_b.dominates(&clock_a));
        assert!(!clock_a.dominates(&clock_b));
        assert!(!clock_a.dominates(&clock_c));
    }

    #[test]
    fn merge_divergent_vector_clocks_multi_actor() {
        let env = Env::default();
        let mut base = VectorClock::new(&env);
        base.increment(&env, 1);
        base.increment(&env, 2);

        // Branch A: Actor 1 and 3 advance
        let mut branch_a = base.clone();
        branch_a.increment(&env, 1);
        branch_a.increment(&env, 3);

        // Branch B: Actor 2 and 4 advance
        let mut branch_b = base.clone();
        branch_b.increment(&env, 2);
        branch_b.increment(&env, 4);

        // Branch C: Actor 3 and 4 advance further
        let mut branch_c = base.clone();
        branch_c.increment(&env, 3);
        branch_c.increment(&env, 3);
        branch_c.increment(&env, 4);

        // All divergent branches are mutually concurrent
        assert_eq!(branch_a.compare(&branch_b), ClockOrdering::Concurrent);
        assert_eq!(branch_b.compare(&branch_c), ClockOrdering::Concurrent);
        assert_eq!(branch_a.compare(&branch_c), ClockOrdering::Concurrent);

        // Merge order 1: (A merge B) merge C
        let mut merged_1 = branch_a.clone();
        merged_1.merge(&branch_b);
        merged_1.merge(&branch_c);

        // Merge order 2: (C merge B) merge A (associativity and commutativity)
        let mut merged_2 = branch_c.clone();
        merged_2.merge(&branch_b);
        merged_2.merge(&branch_a);

        assert_eq!(merged_1.compare(&merged_2), ClockOrdering::Equal);

        // Verify component-wise maximums
        assert_eq!(merged_1.get(1), 2);
        assert_eq!(merged_1.get(2), 2);
        assert_eq!(merged_1.get(3), 2);
        assert_eq!(merged_1.get(4), 1);

        // Merged clock dominates all constituent branches
        assert!(merged_1.dominates(&branch_a));
        assert!(merged_1.dominates(&branch_b));
        assert!(merged_1.dominates(&branch_c));
    }

    #[test]
    fn conflict_detection_with_concurrent_divergent_clocks() {
        let env = Env::default();
        let mut c1 = VectorClock::new(&env);
        let mut c2 = VectorClock::new(&env);

        for _ in 0..5 {
            c1.increment(&env, 1);
        }
        c1.increment(&env, 2);

        for _ in 0..2 {
            c2.increment(&env, 1);
        }
        for _ in 0..4 {
            c2.increment(&env, 2);
        }

        assert_eq!(c1.compare(&c2), ClockOrdering::Concurrent);
        assert_eq!(c2.compare(&c1), ClockOrdering::Concurrent);
        assert!(!c1.dominates(&c2));
        assert!(!c2.dominates(&c1));
    }

    #[test]
    fn disjoint_node_clocks_are_concurrent() {
        let env = Env::default();
        let mut c1 = VectorClock::new(&env);
        let mut c2 = VectorClock::new(&env);

        c1.increment(&env, 10);
        c1.increment(&env, 20);

        c2.increment(&env, 30);
        c2.increment(&env, 40);

        assert_eq!(c1.compare(&c2), ClockOrdering::Concurrent);
        assert_eq!(c2.compare(&c1), ClockOrdering::Concurrent);
    }

    #[test]
    fn asymmetric_node_set_comparison() {
        let env = Env::default();
        let mut c1 = VectorClock::new(&env);
        let mut c2 = VectorClock::new(&env);

        c1.increment(&env, 1);
        c1.increment(&env, 2);

        c2.increment(&env, 1);
        c2.increment(&env, 2);
        c2.increment(&env, 3);

        assert_eq!(c1.compare(&c2), ClockOrdering::Before);
        assert_eq!(c2.compare(&c1), ClockOrdering::After);
        assert!(c2.dominates(&c1));
        assert!(!c1.dominates(&c2));

        c1.increment(&env, 1);
        assert_eq!(c1.compare(&c2), ClockOrdering::Concurrent);
        assert_eq!(c2.compare(&c1), ClockOrdering::Concurrent);
    }
}
