//! pobr-desktop: the desktop GUI entry point (a minimal compilable skeleton).
//!
//! The GUI framework (planned to be [egui](https://github.com/emilk/egui) /
//! `eframe`) hasn't been introduced yet: headless CI can't verify a GUI, and
//! a heavy GUI dependency would slow down builds, so this is deferred.
//!
//! `main` currently only serves as a placeholder/smoke test for the future
//! GUI: it builds a built-in example Build, runs one `pobr-build`
//! orchestrated calculation, translates the title via `pobr-i18n`, and
//! prints a result summary to stdout. The testable construction/summary
//! logic lives in the [`pobr_desktop`] library.

use pobr_desktop::{build_summary, example_build};

fn main() {
    let build = example_build();
    match build_summary(&build, "en-US") {
        Ok(summary) => print!("{summary}"),
        Err(err) => eprintln!("calculation failed: {err}"),
    }
}
