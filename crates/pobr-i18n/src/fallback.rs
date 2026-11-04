//! Fallback chains: which packs answer for a locale, in order.
//!
//! A chain is declared by data (each pack's `_meta.json` `fallback`) and may be
//! extended by the caller; nothing here knows any language. English is the
//! implicit end of every chain: text no pack answers stays English.

use std::collections::BTreeMap;

use crate::pack::{LocalePack, NameKind};

/// Resolves the chain for `code`: the pack itself, then its declared
/// fallbacks depth-first, then `extra` (caller-supplied codes). Each locale
/// appears once; a cycle or a code `load` cannot supply ends that branch.
///
/// `load` returns the pack for a code; it is called at most once per code.
pub fn resolve_chain<F>(code: &str, extra: &[&str], mut load: F) -> Vec<LocalePack>
where
    F: FnMut(&str) -> Option<LocalePack>,
{
    let mut chain = Vec::new();
    chain_codes(code, extra, |next| {
        let pack = load(next)?;
        let fallback = pack.fallback().to_vec();
        chain.push(pack);
        Some(fallback)
    });
    chain
}

/// The chain's codes, in the order of [`resolve_chain`]; `visit` returns a
/// present locale's declared fallbacks, or `None` for a locale that is not
/// available (it is then left out). `visit` is called at most once per code.
pub fn chain_codes<F>(code: &str, extra: &[&str], mut visit: F) -> Vec<String>
where
    F: FnMut(&str) -> Option<Vec<String>>,
{
    let mut seen: Vec<String> = Vec::new();
    let mut found = Vec::new();
    let mut pending: Vec<String> = extra.iter().rev().map(|c| c.to_string()).collect();
    pending.push(code.to_string());
    while let Some(next) = pending.pop() {
        if seen.contains(&next) {
            continue;
        }
        seen.push(next.clone());
        let Some(fallback) = visit(&next) else {
            continue;
        };
        pending.extend(fallback.into_iter().rev());
        found.push(next);
    }
    found
}

/// Locale packs by code, with chained lookup.
#[derive(Debug, Clone, Default)]
pub struct LocaleSet {
    packs: BTreeMap<String, LocalePack>,
}

impl LocaleSet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces a pack.
    pub fn insert(&mut self, pack: LocalePack) {
        self.packs.insert(pack.code().to_string(), pack);
    }

    /// The pack for exactly `code`.
    pub fn get(&self, code: &str) -> Option<&LocalePack> {
        self.packs.get(code)
    }

    /// The available locale codes, sorted.
    pub fn codes(&self) -> impl Iterator<Item = &str> {
        self.packs.keys().map(String::as_str)
    }

    /// The packs that answer for `code`, in order (see [`resolve_chain`]).
    pub fn chain(&self, code: &str, extra: &[&str]) -> Vec<&LocalePack> {
        let codes = chain_codes(code, extra, |c| {
            self.packs.get(c).map(|p| p.fallback().to_vec())
        });
        codes.iter().filter_map(|c| self.packs.get(c)).collect()
    }

    /// The first name the chain for `code` has for `key`.
    pub fn name(&self, code: &str, kind: NameKind, key: &str) -> Option<&str> {
        self.chain(code, &[])
            .into_iter()
            .find_map(|pack| pack.name(kind, key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::LocaleMeta;

    fn pack(code: &str, fallback: &[&str], word: Option<(&str, &str)>) -> LocalePack {
        let mut pack = LocalePack::new(code).unwrap();
        pack.set_meta(LocaleMeta {
            fallback: fallback.iter().map(|s| s.to_string()).collect(),
            ..LocaleMeta::default()
        });
        if let Some((en, text)) = word {
            pack.set_names(NameKind::Word, [(en.into(), text.into())].into());
        }
        pack
    }

    fn set() -> LocaleSet {
        let mut set = LocaleSet::new();
        set.insert(pack("aa-BB", &["aa", "cc"], Some(("Doom", "Doom-BB"))));
        set.insert(pack("aa", &["aa-BB"], Some(("Fate", "Fate-aa"))));
        set.insert(pack("cc", &["missing"], Some(("Fate", "Fate-cc"))));
        set.insert(pack("dd", &[], None));
        set
    }

    fn codes(chain: Vec<&LocalePack>) -> Vec<&str> {
        chain.into_iter().map(LocalePack::code).collect()
    }

    #[test]
    fn walks_declared_fallbacks_once_and_skips_cycles_and_gaps() {
        let set = set();
        assert_eq!(codes(set.chain("aa-BB", &[])), ["aa-BB", "aa", "cc"]);
        assert_eq!(codes(set.chain("aa", &[])), ["aa", "aa-BB", "cc"]);
        assert_eq!(codes(set.chain("dd", &["cc"])), ["dd", "cc"]);
        assert!(set.chain("zz", &[]).is_empty());
        assert_eq!(codes(set.chain("zz", &["dd"])), ["dd"]);
    }

    #[test]
    fn names_resolve_through_the_chain() {
        let set = set();
        assert_eq!(set.name("aa-BB", NameKind::Word, "Doom"), Some("Doom-BB"));
        assert_eq!(set.name("aa-BB", NameKind::Word, "Fate"), Some("Fate-aa"));
        assert_eq!(set.name("cc", NameKind::Word, "Fate"), Some("Fate-cc"));
        assert_eq!(set.name("dd", NameKind::Word, "Fate"), None);
    }

    #[test]
    fn resolve_chain_loads_each_locale_once() {
        let set = set();
        let mut loads = Vec::new();
        let chain = resolve_chain("aa-BB", &["dd", "cc"], |code| {
            loads.push(code.to_string());
            set.get(code).cloned()
        });
        let codes: Vec<&str> = chain.iter().map(LocalePack::code).collect();
        assert_eq!(codes, ["aa-BB", "aa", "cc", "dd"]);
        assert_eq!(loads, ["aa-BB", "aa", "cc", "missing", "dd"]);
    }
}
