#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeterministicContract {
    StableTraversalOrdering,
    ImmutableRealizationSequence,
    ReplayEquivalentExecution,
    StableTopologyTraversal,
    StablePropagationOrdering,
    StableDecayScheduling,
    StableSnapshotGeneration,
}

pub trait DeterministicValidated {
    fn deterministic_contracts(&self) -> &'static [DeterministicContract];
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;
    impl DeterministicValidated for Dummy {
        fn deterministic_contracts(&self) -> &'static [DeterministicContract] {
            static CONTRACTS: [DeterministicContract; 1] = [DeterministicContract::StableTraversalOrdering];
            &CONTRACTS
        }
    }

    #[test]
    fn deterministic_validated_trait_works() {
        let d = Dummy;
        let cs = d.deterministic_contracts();
        assert_eq!(cs.len(), 1);
    }
}