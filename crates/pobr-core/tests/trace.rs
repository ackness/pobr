use pobr_core::{TraceGraph, TraceOperation};
use pobr_data::prelude::*;

#[test]
fn trace_graph_records_nodes_edges_and_sources() {
    let source = SourceId::new(SourceKind::ItemAffix, "weapon.explicit.1");
    let mut trace = TraceGraph::new();

    let input = trace.add_source_node("weapon explicit physical damage", 50.0, source.clone());
    let sum = trace.add_node("PhysicalDamage Inc sum", 50.0, TraceOperation::QuerySum);
    trace.add_edge(input, sum);

    assert_eq!(trace.nodes().len(), 2);
    assert_eq!(trace.edges().len(), 1);
    assert_eq!(trace.node(input).unwrap().source.as_ref(), Some(&source));
    assert_eq!(trace.node(sum).unwrap().operation, TraceOperation::QuerySum);
    assert_eq!(trace.incoming(sum), vec![input]);
    assert_eq!(trace.outgoing(input), vec![sum]);
}
