#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Default)]
pub struct PassiveTreeSpec {
    pub allocated_nodes: Vec<NodeId>,
}
