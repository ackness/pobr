//! L3 eligibility is explicit metadata on the pinned reference, not a vendor guess.

use std::fmt::Display;
use std::io::Write;

pub(super) fn gates_current_data() -> bool {
    let pin: serde_json::Value =
        serde_json::from_str(include_str!("../../../../devs/reference-sources.json"))
            .expect("parse reference pin");
    let reference = pin["pob2"]["data_version"]
        .as_str()
        .expect("pob2.data_version must declare the reference's game data version");
    let current = pobr_gamedata::data_version();
    let gates = reference == current;
    if !gates {
        report(format_args!(
            "L3 PoB2 data {reference}, current data {current}: report only (data versions differ)"
        ));
    }
    gates
}

pub(super) fn report(message: impl Display) {
    // The test harness captures eprintln! on passing tests. Version-lag reports
    // must remain visible in ordinary CI test runs, without --nocapture.
    writeln!(std::io::stderr().lock(), "{message}").expect("write L3 report");
}
