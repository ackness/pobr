//! Results shared by the calculations of one build whose skill selection is the only
//! difference between them.
//!
//! The per-skill overview and Full DPS calculate the same build once per active
//! effect, changing only which group and effect is the main skill. Some results are
//! read from the character's lines, passive tree and configuration, and never from
//! the socket groups or the selection; computing them again for every effect is waste.
//!
//! A [`SelectionScope`] names the build and data those calculations share. While it
//! is open, a result that declares itself independent of the selection is computed
//! once for that exact build (by address) and handed out again. A build the scope does
//! not name — a clone with other items, a trigger source, the other weapon set —
//! computes its own result as before.
//!
//! The caller of [`SelectionScope::enter`] keeps the contract: between entering and
//! dropping the scope it changes nothing of the build but its socket groups and main
//! group, and it does not move the build. Everything a memoized function reads is
//! listed at its use; in debug builds (so in every test) a memo hit is checked
//! against a fresh computation.

use crate::build::Build;
use crate::build_data::BuildData;
use pobr_core::skill_env::GemPropertyBonus;
use std::cell::RefCell;

struct Scope {
    build: *const Build,
    data: *const BuildData,
    gem_bonuses: Option<Vec<GemPropertyBonus>>,
}

impl Scope {
    fn names(&self, build: &Build, data: &BuildData) -> bool {
        std::ptr::eq(self.build, build) && std::ptr::eq(self.data, data)
    }
}

thread_local! {
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

/// Keeps the memo open for one build; see the module documentation.
pub(super) struct SelectionScope {
    outer: Option<Scope>,
}

impl SelectionScope {
    pub(super) fn enter(build: &Build, data: &BuildData) -> Self {
        let scope = Scope {
            build,
            data,
            gem_bonuses: None,
        };
        Self {
            outer: SCOPE.with(|cell| cell.replace(Some(scope))),
        }
    }
}

impl Drop for SelectionScope {
    fn drop(&mut self) {
        let outer = self.outer.take();
        SCOPE.with(|cell| *cell.borrow_mut() = outer);
    }
}

/// The gem level and quality bonuses of `build`. They are read from its equipment,
/// limbs, jewels, passive tree, tree version and configuration lines; never from its
/// socket groups.
pub(super) fn gem_property_bonuses(
    build: &Build,
    data: &BuildData,
    compute: impl Fn() -> Vec<GemPropertyBonus>,
) -> Vec<GemPropertyBonus> {
    let hit = SCOPE.with(|cell| {
        cell.borrow()
            .as_ref()
            .filter(|scope| scope.names(build, data))
            .and_then(|scope| scope.gem_bonuses.clone())
    });
    if let Some(hit) = hit {
        debug_assert_eq!(
            hit,
            compute(),
            "gem bonuses depend on more than the selection scope's contract allows"
        );
        return hit;
    }
    let value = compute();
    SCOPE.with(|cell| {
        if let Some(scope) = cell
            .borrow_mut()
            .as_mut()
            .filter(|scope| scope.names(build, data))
        {
            scope.gem_bonuses = Some(value.clone());
        }
    });
    value
}
