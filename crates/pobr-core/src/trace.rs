use pobr_data::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceNodeId(usize);

impl TraceNodeId {
    pub fn as_usize(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceOperation {
    Input,
    QuerySum,
    QueryMore,
    QueryFlag,
    QueryOverride,
    Add,
    Multiply,
    MoreProduct,
    Clamp,
    Cap,
    Floor,
    Convert,
    Mitigate,
    Average,
    Chance,
    SelectMax,
    Stack,
    Aggregate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceNode {
    pub id: TraceNodeId,
    pub label: String,
    pub value: f64,
    pub operation: TraceOperation,
    pub source: Option<SourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEdge {
    pub from: TraceNodeId,
    pub to: TraceNodeId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracedValue {
    pub value: f64,
    pub node_id: TraceNodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceOutput {
    pub stat: DisplayStatId,
    pub node_id: TraceNodeId,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TraceGraph {
    nodes: Vec<TraceNode>,
    edges: Vec<TraceEdge>,
}

impl TraceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(
        &mut self,
        label: impl Into<String>,
        value: f64,
        operation: TraceOperation,
    ) -> TraceNodeId {
        let id = TraceNodeId(self.nodes.len());
        self.nodes.push(TraceNode {
            id,
            label: label.into(),
            value,
            operation,
            source: None,
        });
        id
    }

    pub fn add_source_node(
        &mut self,
        label: impl Into<String>,
        value: f64,
        source: SourceId,
    ) -> TraceNodeId {
        let id = TraceNodeId(self.nodes.len());
        self.nodes.push(TraceNode {
            id,
            label: label.into(),
            value,
            operation: TraceOperation::Input,
            source: Some(source),
        });
        id
    }

    pub fn add_edge(&mut self, from: TraceNodeId, to: TraceNodeId) {
        self.edges.push(TraceEdge { from, to });
    }

    pub fn node(&self, id: TraceNodeId) -> Option<&TraceNode> {
        self.nodes.get(id.as_usize())
    }

    pub fn nodes(&self) -> &[TraceNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[TraceEdge] {
        &self.edges
    }

    pub fn incoming(&self, to: TraceNodeId) -> Vec<TraceNodeId> {
        self.edges
            .iter()
            .filter(|edge| edge.to == to)
            .map(|edge| edge.from)
            .collect()
    }

    pub fn outgoing(&self, from: TraceNodeId) -> Vec<TraceNodeId> {
        self.edges
            .iter()
            .filter(|edge| edge.from == from)
            .map(|edge| edge.to)
            .collect()
    }

    pub fn source_ancestors(&self, node_id: TraceNodeId) -> Vec<&SourceId> {
        let mut sources = Vec::new();
        let mut stack = self.incoming(node_id);

        while let Some(current) = stack.pop() {
            if let Some(node) = self.node(current) {
                if let Some(source) = &node.source {
                    sources.push(source);
                }
                stack.extend(self.incoming(current));
            }
        }

        sources
    }
}
