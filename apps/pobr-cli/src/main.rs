//! pobr-cli：命令行入口（calculate / parse-mod / parse-item / decode-code / encode-code）。
//!
//! `main` 只做 IO 粘合：clap 解析参数、读文件 / stdin、调用 `pobr_cli` 库函数、打印 JSON。
//! 所有命令逻辑（纯函数 + 可序列化输出）位于 `pobr_cli` 库层，便于单测。

use std::fs;
use std::io::Read;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use pobr_cli::{CalculateRequest, ParseItemRequest};
use pobr_core::calc::MinimalInput;

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
struct ParseModArgs {
    /// 待解析的 modifier 文本。
    text: String,
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
        Command::ParseMod(args) => Ok(pobr_cli::parse_mod_json(&args.text)?),
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
