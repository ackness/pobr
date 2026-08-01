//! Attribution layer — "trace every output back to its modifiers".
//!
//! The final stop in the modifier lifecycle narrative (see the overview in the
//! crate root `lib.rs`), and **PoBR's core value-add over PoB**: PoB only
//! produces the panel numbers, while PoBR can also tell you which modifiers
//! contributed to a given DPS and by how much.
//! - [`trace`]: the computation DAG [`TraceGraph`](trace::TraceGraph) — the
//!   `*_traced` variants of aggregate queries build this graph contribution by
//!   contribution, linking each output back to a
//!   [`SourceId`](pobr_data::source::SourceId).
//! - [`attribution`]: on top of the trace, produces an
//!   [`AttributionReport`](attribution::AttributionReport) in direct / marginal
//!   / interaction terms (marginal is computed by recalculating with a source
//!   removed via [`ModDb::filtered`](crate::ModDb::filtered)).

pub mod attribution;
pub mod trace;
