//! Continuation chain primitives — lightweight resumable continuations used by fibers and topology

pub type Continuation = Box<dyn FnMut() + Send>;

pub struct ContinuationChain {
    chain: Vec<Continuation>,
}

impl ContinuationChain {
    pub fn new() -> Self { Self { chain: Vec::new() } }
    pub fn push(&mut self, c: Continuation) { self.chain.push(c); }
    pub fn resume_all(&mut self) {
        for c in &mut self.chain { (c)(); }
    }
}
