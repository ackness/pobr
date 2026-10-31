//! Test-only observations at shared hit and excluded dependency boundaries.
//!
//! RuntimeConstants getters are observed separately through pobr-data's optional
//! runtime-constant-query-probe feature. The companion tests combine both probes;
//! direct public-field reads still require call-graph inspection.

use std::cell::RefCell;

#[derive(Debug, Default)]
pub(crate) struct Reads {
    pub dependencies: Vec<&'static str>,
    pub queries: usize,
    pub damage_bodies: usize,
    pub components: usize,
    pub critical_resolutions: usize,
}

thread_local! {
    static READS: RefCell<Option<Reads>> = const { RefCell::new(None) };
}

fn record(f: impl FnOnce(&mut Reads)) {
    READS.with_borrow_mut(|slot| {
        if let Some(reads) = slot {
            f(reads);
        }
    });
}

pub(crate) fn dependency(name: &'static str) {
    record(|reads| reads.dependencies.push(name));
}

pub(crate) fn query(name: &str) {
    record(|reads| {
        reads.queries += 1;
        if matches!(
            name,
            "Life"
                | "Mana"
                | "EnergyShield"
                | "ManaUnreserved"
                | "LifeUnreserved"
                | "Accuracy"
                | "Armour"
                | "Evasion"
        ) {
            reads.dependencies.push("actor pool/defence query");
        }
    });
}

pub(crate) fn damage_body() {
    record(|reads| reads.damage_bodies += 1);
}

pub(crate) fn components() {
    record(|reads| reads.components += 1);
}

pub(crate) fn critical_resolution() {
    record(|reads| reads.critical_resolutions += 1);
}

pub(crate) fn observe<T>(f: impl FnOnce() -> T) -> (T, Reads) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            READS.with_borrow_mut(|slot| *slot = None);
        }
    }
    READS.with_borrow_mut(|slot| {
        assert!(slot.is_none(), "hit dependency probes cannot nest");
        *slot = Some(Reads::default());
    });
    let _reset = Reset;
    let value = f();
    let reads = READS.with_borrow_mut(|slot| slot.take().expect("probe is active"));
    (value, reads)
}
