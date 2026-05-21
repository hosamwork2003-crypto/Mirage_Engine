#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForbiddenRuntimeAuthority {
    RuntimeScheduling,
    TopologyOwnership,
    ContinuityOwnership,
    EmergenceMutation,
    AsyncExecution,
    DynamicReordering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeterministicGuarantee {
    StableOrdering,
    ImmutableReplay,
    CanonicalExecution,
    ProvenanceSealing,
    DeterministicEquality,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeExecutionContract {
    pub subsystem: &'static str,
    pub allowed_mutations: Vec<&'static str>,
    pub forbidden_authority: Vec<ForbiddenRuntimeAuthority>,
    pub deterministic_guarantees: Vec<DeterministicGuarantee>,
    pub replay_guarantees: Vec<DeterministicGuarantee>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_determinism() {
        let contract1 = RuntimeExecutionContract {
            subsystem: "mirage-mts",
            allowed_mutations: vec!["topology"],
            forbidden_authority: vec![ForbiddenRuntimeAuthority::RuntimeScheduling],
            deterministic_guarantees: vec![DeterministicGuarantee::StableOrdering],
            replay_guarantees: vec![DeterministicGuarantee::ImmutableReplay],
        };
        let contract2 = RuntimeExecutionContract {
            subsystem: "mirage-mts",
            allowed_mutations: vec!["topology"],
            forbidden_authority: vec![ForbiddenRuntimeAuthority::RuntimeScheduling],
            deterministic_guarantees: vec![DeterministicGuarantee::StableOrdering],
            replay_guarantees: vec![DeterministicGuarantee::ImmutableReplay],
        };
        assert_eq!(contract1, contract2);
    }

    #[test]
    fn forbidden_authority_validation() {
        let contract = RuntimeExecutionContract {
            subsystem: "mirage-morphogenic",
            allowed_mutations: vec!["continuity"],
            forbidden_authority: vec![
                ForbiddenRuntimeAuthority::RuntimeScheduling,
                ForbiddenRuntimeAuthority::TopologyOwnership,
            ],
            deterministic_guarantees: vec![],
            replay_guarantees: vec![],
        };
        assert!(contract.forbidden_authority.contains(&ForbiddenRuntimeAuthority::TopologyOwnership));
        assert!(!contract.forbidden_authority.contains(&ForbiddenRuntimeAuthority::AsyncExecution));
    }

    #[test]
    fn immutable_contract_guarantees() {
        let contract = RuntimeExecutionContract {
            subsystem: "mirage-mkr-core",
            allowed_mutations: vec![],
            forbidden_authority: vec![],
            deterministic_guarantees: vec![DeterministicGuarantee::DeterministicEquality],
            replay_guarantees: vec![DeterministicGuarantee::DeterministicEquality],
        };
        assert_eq!(contract.deterministic_guarantees[0], DeterministicGuarantee::DeterministicEquality);
    }
}
