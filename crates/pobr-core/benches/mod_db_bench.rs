//! ModDb 聚合基准：5000 个 modifier 上的 sum / more 查询吞吐。
//!
//! 运行：`cargo bench -p pobr-core --bench mod_db_bench`。
//! 验证大规模 Modifier 聚合（PoBR 的核心性能目标）不退化。

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const MOD_COUNT: usize = 5000;

/// 构建一个含 5000 个 modifier 的库：Base / Inc / More 三类轮流，
/// 全部挂在同一个 `Damage` 名下（最坏情况：单桶顺序扫描）。
fn build_db() -> ModDb {
    let mut db = ModDb::new();
    for index in 0..MOD_COUNT {
        let mod_type = match index % 3 {
            0 => ModType::Base,
            1 => ModType::Inc,
            _ => ModType::More,
        };
        db.add_mod(Modifier::number(
            "Damage",
            mod_type,
            (index % 17) as f64 + 1.0,
        ));
    }
    db
}

fn bench_mod_db(c: &mut Criterion) {
    let db = build_db();
    let cfg = CalcConfig::attack();
    let names = [ModName::from("Damage")];

    c.bench_function("mod_db_sum_inc_5000", |b| {
        b.iter(|| {
            black_box(db.sum(ModType::Inc, black_box(&cfg), black_box(&names)));
        })
    });

    c.bench_function("mod_db_sum_base_5000", |b| {
        b.iter(|| {
            black_box(db.sum(ModType::Base, black_box(&cfg), black_box(&names)));
        })
    });

    c.bench_function("mod_db_more_5000", |b| {
        b.iter(|| {
            black_box(db.more(black_box(&cfg), black_box(&names)));
        })
    });
}

criterion_group!(benches, bench_mod_db);
criterion_main!(benches);
