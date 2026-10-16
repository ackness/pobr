//! Official monster-table joins, including separate resistance records.

use pobr_data::catalog::MonsterResistanceDef;
use pobr_gamedata::{GameData, data_version, repo_data_root};

#[test]
fn loads_official_base_resistances_for_monster_varieties() {
    let data = GameData::new(repo_data_root().join(data_version()))
        .monster_resistances()
        .unwrap();
    let cases = [
        // MonsterTypes links three separate resistance records.
        ("Metadata/Monsters/MudGolem/MudGolem", [30, 30, -30, 0]),
        (
            "Metadata/Monsters/Skeletons/PlayerSummoned/SkeletonArsonistPlayerSummoned",
            [50, 0, 0, 0],
        ),
        (
            "Metadata/Monsters/Skeletons/PlayerSummoned/SkeletonFrostMagePlayerSummoned",
            [0, 50, 0, 0],
        ),
        (
            "Metadata/Monsters/Skeletons/PlayerSummoned/SkeletonStormMagePlayerSummoned_",
            [0, 0, 50, 0],
        ),
        (
            "Metadata/Monsters/VoodooKing/VoodooKingBoss2",
            [0, 0, 0, 50],
        ),
        // An empty resistance list contributes zero.
        ("Metadata/Monsters/Clone/RipFromTimeClone", [0, 0, 0, 0]),
    ];
    for (id, [fire, cold, lightning, chaos]) in cases {
        assert_eq!(
            data.monsters[id],
            MonsterResistanceDef {
                fire,
                cold,
                lightning,
                chaos
            },
            "{id}"
        );
    }
}
