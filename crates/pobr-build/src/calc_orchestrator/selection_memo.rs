//! Results shared by the calculations of one build whose skill selection is the only
//! difference between them.
//!
//! The per-skill overview and Full DPS calculate the same build once per active
//! effect, changing only which group and effect is the main skill. A trigger's source
//! skills are calculated the same way: one clone of the build per candidate, with the
//! main skill pointed at the candidate. Some results are read from the character's
//! lines, passive tree and configuration, and never from the socket groups or the
//! selection; computing them again for every calculation is waste.
//!
//! A [`SelectionScope`] names the build and data those calculations share. While it
//! is open, a result that declares itself independent of the selection is computed
//! once for that exact build (by address) and handed out again. A build the scope does
//! not name — a clone with other items, the other weapon set — computes its own result
//! as before. [`adopt`] names one more build, a clone that differs from a named build
//! in its selection only, for as long as the clone lives.
//!
//! The caller of [`SelectionScope::enter`] keeps the contract: between entering and
//! dropping the scope it changes nothing of the build but its socket groups and main
//! group, and it does not move the build. Everything a memoized function reads is
//! listed at its use; in debug builds (so in every test) a memo hit is checked
//! against a fresh computation.

use crate::build::Build;
use crate::build_data::BuildData;
use pobr_core::Modifier;
use pobr_core::skill_env::GemPropertyBonus;
use pobr_data::item::EquipmentSlot;
use std::cell::RefCell;

struct Scope {
    /// The build the scope was entered for, then the clones adopted into it.
    builds: Vec<*const Build>,
    data: *const BuildData,
    gem_bonuses: Option<Vec<GemPropertyBonus>>,
    radius_grants: Option<Vec<Modifier>>,
    slot_scales: Option<Vec<(EquipmentSlot, f64)>>,
}

impl Scope {
    fn names(&self, build: &Build, data: &BuildData) -> bool {
        std::ptr::eq(self.data, data) && self.builds.iter().any(|named| std::ptr::eq(*named, build))
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
            builds: vec![build as *const Build],
            data,
            gem_bonuses: None,
            radius_grants: None,
            slot_scales: None,
        };
        Self {
            outer: SCOPE.with(|cell| cell.replace(Some(scope))),
        }
    }

    /// Opens a scope for `build` unless the open one already names it, so a caller
    /// that calculates one build repeatedly under its own scope keeps that scope's
    /// results.
    pub(super) fn enter_unless_named(build: &Build, data: &BuildData) -> Option<Self> {
        let named = SCOPE.with(|cell| {
            cell.borrow()
                .as_ref()
                .is_some_and(|scope| scope.names(build, data))
        });
        (!named).then(|| Self::enter(build, data))
    }
}

impl Drop for SelectionScope {
    fn drop(&mut self) {
        let outer = self.outer.take();
        SCOPE.with(|cell| *cell.borrow_mut() = outer);
    }
}

/// A clone named by the open scope; see [`adopt`].
pub(super) struct Adopted {
    child: Option<*const Build>,
}

impl Drop for Adopted {
    fn drop(&mut self) {
        let Some(child) = self.child.take() else {
            return;
        };
        SCOPE.with(|cell| {
            if let Some(scope) = cell.borrow_mut().as_mut()
                && let Some(at) = scope
                    .builds
                    .iter()
                    .rposition(|named| std::ptr::eq(*named, child))
            {
                scope.builds.remove(at);
            }
        });
    }
}

/// Names `child` in the open scope for as long as the returned guard lives, if the
/// scope names `parent`.
///
/// `child` must be a clone of `parent` that differs from it in nothing but its
/// socket groups and main group, as the scope's contract allows `parent` to change.
/// The guard has to be dropped before `child` is, or moved: the scope keeps
/// `child`'s address, which another build could take once `child` is gone.
pub(super) fn adopt(parent: &Build, child: &Build, data: &BuildData) -> Adopted {
    let adopted = SCOPE.with(|cell| {
        let mut slot = cell.borrow_mut();
        match slot.as_mut().filter(|scope| scope.names(parent, data)) {
            Some(scope) if !scope.names(child, data) => {
                scope.builds.push(child as *const Build);
                true
            }
            _ => false,
        }
    });
    Adopted {
        child: adopted.then_some(child as *const Build),
    }
}

/// Hands out the value `field` holds for the scope that names `build`, computing it
/// the first time. `compute` may use other memoized results, so no borrow of the
/// scope is held while it runs.
fn memoized<T: Clone + PartialEq + std::fmt::Debug>(
    build: &Build,
    data: &BuildData,
    field: fn(&mut Scope) -> &mut Option<T>,
    compute: impl Fn() -> T,
    what: &str,
) -> T {
    let hit = SCOPE.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .filter(|scope| scope.names(build, data))
            .and_then(|scope| field(scope).clone())
    });
    if let Some(hit) = hit {
        debug_assert_eq!(
            hit,
            compute(),
            "{what} depend on more than the selection scope's contract allows"
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
            *field(scope) = Some(value.clone());
        }
    });
    value
}

/// The gem level and quality bonuses of `build`. They are read from its equipment,
/// limbs, jewels, passive tree, tree version and configuration lines; never from its
/// socket groups.
pub(super) fn gem_property_bonuses(
    build: &Build,
    data: &BuildData,
    compute: impl Fn() -> Vec<GemPropertyBonus>,
) -> Vec<GemPropertyBonus> {
    memoized(
        build,
        data,
        |scope| &mut scope.gem_bonuses,
        compute,
        "gem bonuses",
    )
}

/// What the radius jewels of `build` grant the passives in their radius. It is read
/// from its passive tree, tree version, radius jewels, jewels, items and limbs (the
/// notables they allocate), and the game data; never from its socket groups.
pub(super) fn radius_jewel_grants(
    build: &Build,
    data: &BuildData,
    compute: impl Fn() -> Vec<Modifier>,
) -> Vec<Modifier> {
    memoized(
        build,
        data,
        |scope| &mut scope.radius_grants,
        compute,
        "radius jewel grants",
    )
}

/// How much each equipment slot's bonuses are scaled. They are read from the lines of
/// its passive tree, items, limbs and jewels, its radius jewel grants, and the game
/// data; never from its socket groups.
pub(super) fn slot_bonus_scales(
    build: &Build,
    data: &BuildData,
    compute: impl Fn() -> Vec<(EquipmentSlot, f64)>,
) -> Vec<(EquipmentSlot, f64)> {
    memoized(
        build,
        data,
        |scope| &mut scope.slot_scales,
        compute,
        "slot bonus scales",
    )
}
