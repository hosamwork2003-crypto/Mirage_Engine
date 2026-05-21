#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAuthorityDomain {
    Matrix,
    Topology,
    Realization,
    Executor,
    Morphogenic,
    RuntimeCoordinator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityBoundaryViolation {
    pub source_domain: RuntimeAuthorityDomain,
    pub target_domain: RuntimeAuthorityDomain,
    pub operation: &'static str,
}

pub trait AuthorityBounded {
    fn authority_domain(&self) -> RuntimeAuthorityDomain;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;
    impl AuthorityBounded for Dummy {
        fn authority_domain(&self) -> RuntimeAuthorityDomain { RuntimeAuthorityDomain::Matrix }
    }

    #[test]
    fn authority_bounded_trait_works() {
        let d = Dummy;
        assert_eq!(d.authority_domain(), RuntimeAuthorityDomain::Matrix);
    }
}