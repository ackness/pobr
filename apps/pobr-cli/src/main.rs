//! pobr-cli：命令行入口（calculate / parse-mod / parse-item / decode-code / encode-code）。
//!
//! `main` 只做 IO 粘合：clap 解析参数、读文件 / stdin、调用 `pobr_cli` 库函数、打印 JSON。
//! 所有命令逻辑（纯函数 + 可序列化输出）位于 `pobr_cli` 库层，便于单测。

use std::fs;
use std::io::Read;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use pobr_cli::{CalculateBuildRequest, CalculateRequest, ParseItemRequest};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;

#[derive(Debug, Parser)]
#[command(
    name = "pobr",
    about = "Path of Building in Rust — 计算 / 解析 CLI",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 从基础属性 + modifier 文本执行最小计算，输出关键字段 JSON。
    Calculate(CalculateArgs),
    /// 从 PoB Build Code 端到端计算（装备 / 天赋 / 宝石 / 角色基础 / 敌人），输出 JSON。
    CalculateBuild(CalculateBuildArgs),
    /// 解析单条 modifier 文本，输出解析报告。
    ParseMod(ParseModArgs),
    /// 解析 raw item text（--text / --file / stdin）。当前为占位，返回未实现错误。
    ParseItem(ParseItemArgs),
    /// 解码 PoB Build Code → XML。
    DecodeCode(CodeArgs),
    /// 编码 XML（--text / --file / stdin）→ PoB Build Code。
    EncodeCode(TextSourceArgs),
}

#[derive(Debug, Args)]
struct CalculateArgs {
    /// 基础生命值。
    #[arg(long, default_value_t = 0.0)]
    base_life: f64,
    /// 基础魔力值。
    #[arg(long, default_value_t = 0.0)]
    base_mana: f64,
    /// 基础火焰抗性。
    #[arg(long, default_value_t = 0.0)]
    base_fire_resistance: f64,
    /// 基础冰霜抗性。
    #[arg(long, default_value_t = 0.0)]
    base_cold_resistance: f64,
    /// 基础闪电抗性。
    #[arg(long, default_value_t = 0.0)]
    base_lightning_resistance: f64,
    /// 基础命中值。
    #[arg(long, default_value_t = 0.0)]
    base_accuracy: f64,
    /// 敌人闪避值。
    #[arg(long, default_value_t = 0.0)]
    enemy_evasion: f64,
    /// 基础最小命中伤害。
    #[arg(long, default_value_t = 0.0)]
    base_hit_min: f64,
    /// 基础最大命中伤害。
    #[arg(long, default_value_t = 0.0)]
    base_hit_max: f64,
    /// 基础动作频率（每秒次数）。
    #[arg(long, default_value_t = 0.0)]
    base_action_rate: f64,
    /// 内联 modifier 文本（可重复）。
    #[arg(long = "mod", value_name = "TEXT")]
    mods: Vec<String>,
    /// 从文件读取 modifier 文本（每行一条；与 --mod 合并）。
    #[arg(long)]
    mods_file: Option<String>,
}

#[derive(Debug, Args)]
struct CalculateBuildArgs {
    /// PoB Build Code（内联）；与 --file 二选一，缺省从 stdin 读取。
    code: Option<String>,
    /// 从文件读取 PoB Build Code。
    #[arg(long)]
    file: Option<String>,
    /// 游戏数据版本目录（含入库 JSON）。缺省使用仓库内置 `data/4.5.0.3.4`。
    #[arg(long)]
    data_dir: Option<String>,
    /// 敌人等级（0 = 跟随角色等级）。
    #[arg(long, default_value_t = 0)]
    enemy_level: u32,
    /// 敌人档位：none / boss / pinnacle / uber。
    #[arg(long, default_value = "pinnacle")]
    enemy_tier: String,
    /// 面板口径（不计敌人交互）。缺省为有效 DPS 口径。
    #[arg(long, default_value_t = false)]
    panel: bool,
}

#[derive(Debug, Args)]
struct ParseModArgs {
    /// 待解析的 modifier 文本。
    text: String,
    /// 版本数据目录（默认 `<repo>/data/4.5.0.3.4`）。从中编译 parser 规则。
    #[arg(long)]
    data_dir: Option<String>,
}

#[derive(Debug, Args)]
struct ParseItemArgs {
    /// 内联 raw item text。
    #[arg(long)]
    text: Option<String>,
    /// 从文件读取 raw item text。
    #[arg(long)]
    file: Option<String>,
}

#[derive(Debug, Args)]
struct CodeArgs {
    /// 待解码的 PoB Build Code。
    code: String,
}

#[derive(Debug, Args)]
struct TextSourceArgs {
    /// 内联文本。
    #[arg(long)]
    text: Option<String>,
    /// 从文件读取文本。
    #[arg(long)]
    file: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<String, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Calculate(args) => Ok(pobr_cli::calculate_json(&build_calculate_request(args)?)?),
        Command::CalculateBuild(args) => Ok(pobr_cli::calculate_build_json(
            &build_calc_build_request(args)?,
        )?),
        Command::ParseMod(args) => {
            let data_dir = match args.data_dir {
                Some(dir) => std::path::PathBuf::from(dir),
                None => pobr_gamedata::current_data_dir(),
            };
            Ok(pobr_cli::parse_mod_json(&args.text, &data_dir)?)
        }
        Command::ParseItem(args) => {
            let text = read_text_source(args.text, args.file)?;
            Ok(pobr_cli::parse_item_json(&ParseItemRequest { text })?)
        }
        Command::DecodeCode(args) => Ok(pobr_cli::decode_code(&args.code)?),
        Command::EncodeCode(args) => {
            let xml = read_text_source(args.text, args.file)?;
            Ok(pobr_cli::encode_code(&xml)?)
        }
    }
}

fn build_calculate_request(
    args: CalculateArgs,
) -> Result<CalculateRequest, Box<dyn std::error::Error>> {
    let input = MinimalInput {
        base_life: args.base_life,
        base_mana: args.base_mana,
        base_fire_resistance: args.base_fire_resistance,
        base_cold_resistance: args.base_cold_resistance,
        base_lightning_resistance: args.base_lightning_resistance,
        base_accuracy: args.base_accuracy,
        enemy_evasion: args.enemy_evasion,
        base_hit_min: args.base_hit_min,
        base_hit_max: args.base_hit_max,
        base_action_rate: args.base_action_rate,
    };

    let mut modifier_texts = args.mods;
    if let Some(path) = args.mods_file {
        let contents = fs::read_to_string(&path)?;
        modifier_texts.extend(
            contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string),
        );
    }

    Ok(CalculateRequest {
        input,
        modifier_texts,
    })
}

fn build_calc_build_request(
    args: CalculateBuildArgs,
) -> Result<CalculateBuildRequest, Box<dyn std::error::Error>> {
    let code = read_text_source(args.code, args.file)?;

    let data_dir = match args.data_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => pobr_gamedata::current_data_dir(),
    };

    let enemy_tier = match args.enemy_tier.to_ascii_lowercase().as_str() {
        "none" => EnemyTier::None,
        "boss" => EnemyTier::Boss,
        "pinnacle" => EnemyTier::Pinnacle,
        "uber" => EnemyTier::Uber,
        other => return Err(format!("unknown enemy tier: {other}").into()),
    };

    Ok(CalculateBuildRequest {
        code,
        data_dir,
        enemy_level: args.enemy_level,
        enemy_tier,
        mode_effective: !args.panel,
    })
}

/// 文本来源优先级：`--text` > `--file` > stdin。
fn read_text_source(
    text: Option<String>,
    file: Option<String>,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(text) = text {
        return Ok(text);
    }
    if let Some(path) = file {
        return Ok(fs::read_to_string(&path)?);
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}
