use pobr_data::catalog::PassiveNodeDef;
use pobr_tree::allocation::{Allocation, AllocationTree, EditAction, Grant};
use std::collections::BTreeSet;
fn nodes() -> Vec<PassiveNodeDef> {
    serde_json::from_value(serde_json::json!([
     {"skill":1,"id":"root","kind":"normal","connections":[2]},
     {"skill":2,"id":"travel","kind":"normal","connections":[3,5]},
     {"skill":3,"id":"branch","kind":"normal","connections":[4,6]},
     {"skill":4,"id":"leaf","kind":"notable"},
     {"skill":5,"id":"other","kind":"normal"},
     {"skill":6,"id":"socket","kind":"jewel_socket"},
     {"skill":7,"id":"island","kind":"normal"}
    ]))
    .unwrap()
}
fn tree(nodes: &[PassiveNodeDef]) -> AllocationTree<'_> {
    let mut t = AllocationTree::new(nodes.iter());
    t.roots.insert(1);
    t.class_starts.insert(1);
    t.points_max = 10;
    t.weapon_points_base = 3;
    t
}
fn set(ids: &[u32]) -> BTreeSet<u32> {
    ids.iter().copied().collect()
}
#[test]
fn set_paths_cannot_use_the_other_set_and_shared_promotion_connects() {
    let n = nodes();
    let t = tree(&n);
    let mut a = Allocation {
        shared: set(&[2]),
        exclusive: [set(&[3]), set(&[5])],
    };
    assert_eq!(
        t.edit(&a, 4, 2, EditAction::Allocate).unwrap_err(),
        "no_legal_path"
    );
    let result = t.edit(&a, 4, 1, EditAction::Allocate).unwrap();
    a = result.allocation;
    assert_eq!(a.exclusive[1], set(&[5]));
    assert_eq!(result.points_used, 3);
    let shared = t.edit(&a, 4, 0, EditAction::Promote).unwrap();
    assert_eq!(shared.allocation.shared, set(&[2, 3, 4]));
    assert_eq!(shared.promoted, vec![3, 4]);
    assert_eq!(shared.allocation.exclusive[1], set(&[5]));
}
#[test]
fn shared_refund_previews_both_sets_but_exclusive_refund_keeps_inactive_set() {
    let n = nodes();
    let t = tree(&n);
    let a = Allocation {
        shared: set(&[2]),
        exclusive: [set(&[3, 4]), set(&[5])],
    };
    let one = t.edit(&a, 3, 1, EditAction::Refund).unwrap();
    assert_eq!(one.removed, vec![3, 4]);
    assert_eq!(one.allocation.exclusive[1], set(&[5]));
    let both = t.edit(&a, 2, 0, EditAction::Refund).unwrap();
    assert_eq!(both.removed, vec![2, 3, 4, 5]);
}
#[test]
fn socket_paths_are_shared_and_budget_counts_the_larger_set() {
    let n = nodes();
    let mut t = tree(&n);
    let a = Allocation {
        shared: set(&[2]),
        exclusive: [set(&[3]), set(&[5])],
    };
    let socket = t.edit(&a, 6, 1, EditAction::Allocate).unwrap();
    assert_eq!(socket.allocation.shared, set(&[2, 3, 6]));
    assert_eq!(socket.promoted, vec![3]);
    t.weapon_points_base = 1;
    assert_eq!(
        t.edit(&a, 4, 1, EditAction::Allocate).unwrap_err(),
        "weapon_point_budget"
    );
    t.weapon_points_base = 3;
    t.points_max = 2;
    assert_eq!(
        t.edit(&a, 4, 1, EditAction::Allocate).unwrap_err(),
        "passive_point_budget"
    );
}
#[test]
fn disconnected_nodes_and_incompatible_shared_imports_are_rejected() {
    let n = nodes();
    let t = tree(&n);
    assert_eq!(
        t.edit(&Allocation::default(), 7, 1, EditAction::Allocate)
            .unwrap_err(),
        "no_legal_path"
    );
    let a = Allocation {
        shared: set(&[4]),
        exclusive: [set(&[2, 3]), set(&[])],
    };
    assert_eq!(t.validate(&a).unwrap_err(), "disconnected_shared_path");
}

#[test]
fn radius_grants_do_not_make_travel_bridges_and_refund_removes_both_sets() {
    let mut n = nodes();
    n[6].connections = vec![8];
    n.push(
        serde_json::from_value(serde_json::json!({"skill":8,"id":"outside","kind":"normal"}))
            .unwrap(),
    );
    let mut t = tree(&n);
    t.grants.push(Grant {
        source: 6,
        nodes: vec![7],
        roots: vec![],
    });
    let a = Allocation {
        shared: set(&[2, 3, 6]),
        exclusive: [set(&[7]), set(&[5])],
    };
    t.validate(&a).unwrap();
    assert_eq!(
        t.edit(&a, 8, 1, EditAction::Allocate).unwrap_err(),
        "no_legal_path"
    );
    let refund = t.edit(&a, 6, 0, EditAction::Refund).unwrap();
    assert_eq!(refund.removed, vec![6, 7]);
    assert_eq!(refund.allocation.exclusive[1], set(&[5]));
    t.grants[0].roots = vec![7];
    assert!(t.edit(&a, 8, 1, EditAction::Allocate).is_ok());
}
#[test]
fn prerequisites_are_validated_and_refunded_even_when_physically_connected() {
    let mut n = nodes();
    n[3].unlock_constraint = Some(
        serde_json::from_value(serde_json::json!({"nodes":[5],"ascendancy":"Test1"})).unwrap(),
    );
    let mut t = tree(&n);
    let a = Allocation {
        shared: set(&[2, 3, 5]),
        exclusive: [set(&[]), set(&[])],
    };
    assert_eq!(
        t.edit(&a, 4, 1, EditAction::Allocate).unwrap_err(),
        "no_legal_path"
    );
    t.ascendancy = Some("Test1".into());
    let a = t.edit(&a, 4, 1, EditAction::Allocate).unwrap().allocation;
    let result = t.edit(&a, 5, 0, EditAction::Refund).unwrap();
    assert_eq!(result.removed, vec![4, 5]);
}
#[test]
fn conversion_increases_only_weapon_budget_and_cannot_be_refunded_under_spent_points() {
    let mut n = nodes();
    n[4].stats = vec![
        "100 Passive Skill Points become [WeaponSetPassiveSkillPoints|Weapon Set Skill Points]"
            .into(),
    ];
    let mut t = tree(&n);
    t.weapon_points_base = 1;
    let a = Allocation {
        shared: set(&[2, 5]),
        exclusive: [set(&[3, 4]), set(&[])],
    };
    assert_eq!(
        t.edit(&a, 5, 0, EditAction::Refund).unwrap_err(),
        "weapon_point_budget"
    );
    let result = t.edit(&a, 4, 0, EditAction::Promote).unwrap();
    assert_eq!(result.points_used, 4);
    assert_eq!(result.weapon_points_max, 101);
    assert_eq!(result.points_max, 10);
    assert_eq!(result.promoted, vec![3, 4]);
}

#[test]
fn a_mastery_cannot_be_added_without_an_effect_selection_or_used_as_travel() {
    let mut n = nodes();
    n[2].kind = pobr_data::catalog::PassiveNodeKind::Mastery;
    let t = tree(&n);
    assert_eq!(
        t.edit(&Allocation::default(), 3, 0, EditAction::Allocate)
            .unwrap_err(),
        "unsupported_node"
    );
    assert_eq!(
        t.edit(&Allocation::default(), 4, 0, EditAction::Allocate)
            .unwrap_err(),
        "no_legal_path"
    );
}
