// Generated from Rust data-info DTOs by ts-rs. Do not edit manually.
// Regenerate: cargo test -p pobr-wasm --lib export_typescript_contract -- --ignored

export type LocaleInfo = {
/**
 * The locale code (`zh-CN`, ...).
 */
code: string,
/**
 * The fallbacks its `_meta.json` declares, in order.
 */
fallback: Array<string>,
/**
 * The locales that answer for it: itself, then its declared fallbacks
 * followed through the other declared locales (English ends every chain
 * implicitly and is not listed).
 */
chain: Array<string>,
/**
 * Whether its own pack publishes mod-line templates (`stat_lines.json`).
 */
templates: boolean, };

export type DataInfo = { data_version: string, tree_version: string, locales: Array<LocaleInfo>, };
