use std::borrow::Cow;

/// The catalog id every unrecognised language falls back to. Matches the
/// `fallback` passed to `rust_i18n::i18n!` in the crate root.
const FALLBACK_LOCALE: &str = "en";

/// The single simplified-Chinese catalog. Several legacy ids map onto it.
const SIMPLIFIED_CHINESE_LOCALE: &str = "zh-CN";

/// Map a stored UI language onto a catalog id rust-i18n can resolve.
///
/// rust-i18n only de-specialises by stripping subtags, so `zh-Hans` would walk to
/// `zh` and then to English because no `zh` catalog exists. NyaTerm has persisted
/// `zh`, `zh_CN`, and `zh-Hans` since the Tauri builds, so those aliases have to be
/// folded onto `zh-CN` here or existing installs silently switch to English.
pub(crate) fn normalize_locale(language: &str) -> Cow<'static, str> {
    let requested = language.trim().replace('_', "-");
    if is_simplified_chinese(&requested) {
        return Cow::Borrowed(SIMPLIFIED_CHINESE_LOCALE);
    }

    rust_i18n::available_locales!()
        .into_iter()
        .find(|locale| locale.eq_ignore_ascii_case(&requested))
        .unwrap_or(Cow::Borrowed(FALLBACK_LOCALE))
}

/// Point rust-i18n's process-wide locale at the stored UI language.
///
/// The persisted setting stays authoritative; this is a write-through projection of
/// it, so it must be called from every writer of that setting. `rust_i18n`'s locale
/// is a single global shared by every crate in the graph, which is what makes
/// `gpui-component`'s own widget strings follow the same language.
pub(crate) fn apply_locale(language: &str) {
    rust_i18n::set_locale(&normalize_locale(language));
}

/// The locales NyaTerm ships a catalog for, in a stable order.
///
/// Adding a language is a data-only change: drop a `<locale>.json` into
/// `locales/` and it appears here, in the settings selector, and in the
/// title-bar menu.
pub(crate) fn available_locales() -> Vec<Cow<'static, str>> {
    rust_i18n::available_locales!()
}

/// A locale's name in its own language, for the language pickers.
pub(crate) fn locale_display_name(locale: &str) -> Cow<'static, str> {
    rust_i18n::t!("language.name", locale = locale)
}

/// Language picker options, one per shipped catalog.
pub(crate) fn language_options() -> Vec<nyaterm_ui::NyaSelectOption> {
    available_locales()
        .into_iter()
        .map(|locale| {
            let label = locale_display_name(&locale);
            nyaterm_ui::NyaSelectOption::new(locale.into_owned(), label)
        })
        .collect()
}

fn is_simplified_chinese(language: &str) -> bool {
    let normalized = language.to_ascii_lowercase();
    normalized == "zh" || normalized == "zh-cn" || normalized.starts_with("zh-hans")
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap, HashSet};
    use std::fs;
    use std::path::Path;

    use regex::Regex;
    use serde_json::Value;

    use super::{Cow, available_locales, locale_display_name, normalize_locale};

    /// Translate against an explicit language instead of the process-wide locale,
    /// so no test has to touch `rust_i18n::set_locale` and race the others.
    fn text(language: &str, key: &'static str) -> Cow<'static, str> {
        let locale = normalize_locale(language);
        rust_i18n::t!(key, locale = locale.as_ref())
    }

    const EN_JSON: &str = include_str!("../../locales/en.json");
    const ZH_CN_JSON: &str = include_str!("../../locales/zh-CN.json");

    /// Mirror of the flattening `rust_i18n` applies to a version-1 locale file, so
    /// these tests can reason about the on-disk files rather than the loaded catalog.
    fn flatten(json: &str) -> HashMap<String, String> {
        fn walk(prefix: Option<&str>, value: &Value, output: &mut HashMap<String, String>) {
            let Value::Object(entries) = value else {
                return;
            };
            for (name, value) in entries {
                let key = prefix.map_or_else(|| name.clone(), |prefix| format!("{prefix}.{name}"));
                match value {
                    Value::String(text) => {
                        output.insert(key, text.clone());
                    }
                    Value::Object(_) => walk(Some(&key), value, output),
                    _ => {}
                }
            }
        }

        let value: Value = serde_json::from_str(json).expect("locale JSON must be valid");
        let mut output = HashMap::new();
        walk(None, &value, &mut output);
        output
    }

    #[test]
    fn resolves_tauri_locale_keys_and_normalizes_chinese_ids() {
        assert_eq!(text("en", "menu.file"), "File");
        assert_eq!(text("zh-CN", "menu.file"), "文件");
        assert_eq!(text("zh_CN", "common.cancel"), "取消");
        assert_eq!(text("zh-Hans", "settings.title"), "设置");
        assert_eq!(text("en", "common.copyToClipboard"), "Copy");
        assert_eq!(text("zh-CN", "common.copyToClipboard"), "复制");
        assert_eq!(text("en", "common.retry"), "Retry");
        assert_eq!(text("zh-CN", "common.retry"), "重试");
    }

    #[test]
    fn falls_back_to_english_then_the_key() {
        assert_eq!(text("fr", "menu.help"), "Help");
        assert_eq!(
            text("zh-CN", "missing.translation.key"),
            "missing.translation.key"
        );
    }

    #[test]
    fn normalizes_legacy_language_ids_onto_shipped_catalogs() {
        for legacy in ["zh", "zh-CN", "zh_CN", "zh-Hans", "zh-hans-cn", " zh-cn "] {
            assert_eq!(normalize_locale(legacy), "zh-CN", "for {legacy:?}");
        }
        for other in ["en", "en-US", "fr", "", "nonsense"] {
            assert_eq!(normalize_locale(other), "en", "for {other:?}");
        }
    }

    #[test]
    fn normalized_locales_are_always_catalogs_rust_i18n_can_resolve() {
        let available = rust_i18n::available_locales!();
        for language in ["zh", "zh-CN", "zh-Hans", "en", "en-US", "fr", ""] {
            let normalized = normalize_locale(language);
            assert!(
                available.contains(&normalized),
                "{language:?} normalized to {normalized:?}, which is not in {available:?}"
            );
        }
    }

    /// Entries whose `{{..}}` is literal text rather than an i18n slot.
    ///
    /// `quickCommands.commandPlaceholder` documents NyaTerm's own quick-command
    /// variable syntax, which is handlebars-shaped and parsed by
    /// `parse_quick_command_variables`. It is never interpolated by `t!`.
    const HANDLEBARS_IS_LITERAL: &[&str] = &["quickCommands.commandPlaceholder"];

    #[test]
    fn every_shipped_locale_names_itself_for_the_language_pickers() {
        // The selector and the title-bar menu label each entry with
        // `language.name` read in that entry's own locale, so a catalog without
        // one would show a bare key.
        let locales = available_locales();
        assert_eq!(locales, ["en", "zh-CN"], "shipped catalogs changed");
        for locale in &locales {
            let name = locale_display_name(locale);
            assert_ne!(name, "language.name", "{locale} has no language.name");
            assert!(
                !name.trim().is_empty(),
                "{locale} has a blank language.name"
            );
        }
        assert_eq!(locale_display_name("en"), "English");
        assert_eq!(locale_display_name("zh-CN"), "中文 (简体)");
    }

    #[test]
    fn placeholders_use_the_rust_i18n_syntax() {
        // A leftover `{{name}}` would render literally: rust-i18n only substitutes
        // `%{name}`, and nothing else interpolates these strings any more.
        let mut stragglers = Vec::new();
        for (locale, json) in [("en", EN_JSON), ("zh-CN", ZH_CN_JSON)] {
            for (key, value) in flatten(json) {
                if value.contains("{{") && !HANDLEBARS_IS_LITERAL.contains(&key.as_str()) {
                    stragglers.push(format!("{locale}/{key}: {value}"));
                }
            }
        }
        stragglers.sort();
        assert!(
            stragglers.is_empty(),
            "handlebars placeholders left behind: {stragglers:#?}"
        );
    }

    #[test]
    fn translations_agree_on_their_placeholders() {
        // A translation that drops or renames a placeholder leaves the argument
        // unsubstituted for that locale only, which no other test would catch.
        let placeholder = Regex::new(r"%\{(\w+)\}").expect("placeholder regex");
        let names = |value: &str| {
            placeholder
                .captures_iter(value)
                .map(|c| c[1].to_string())
                .collect::<BTreeSet<_>>()
        };

        let english = flatten(EN_JSON);
        let chinese = flatten(ZH_CN_JSON);
        let mut mismatched = Vec::new();
        for (key, en_value) in &english {
            if HANDLEBARS_IS_LITERAL.contains(&key.as_str()) {
                continue;
            }
            let Some(zh_value) = chinese.get(key) else {
                continue; // covered by the key-parity test
            };
            let (want, got) = (names(en_value), names(zh_value));
            if want != got {
                mismatched.push(format!("{key}: en {want:?} vs zh-CN {got:?}"));
            }
        }
        mismatched.sort();
        assert!(
            mismatched.is_empty(),
            "placeholder mismatch: {mismatched:#?}"
        );
    }

    #[test]
    fn english_and_chinese_catalogs_have_identical_keys() {
        let english = flatten(EN_JSON).into_keys().collect::<HashSet<_>>();
        let chinese = flatten(ZH_CN_JSON).into_keys().collect::<HashSet<_>>();
        assert_eq!(english, chinese);
    }

    #[test]
    fn every_literal_translation_key_in_the_crate_exists_in_both_catalogs() {
        // Walks the crate at test time rather than listing files, so a new module
        // cannot quietly opt out. Only literal keys are visible here - the ones
        // reached through `i18n_key()` and friends are out of reach for any
        // source scan, and stay covered by the key-parity test above.
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let key_pattern = Regex::new(r#"\bt!\(\s*"([^"]+)""#).expect("translation-key regex");
        let english = flatten(EN_JSON);
        let chinese = flatten(ZH_CN_JSON);

        let mut pending = vec![source_root];
        let mut scanned = 0usize;
        let mut missing = Vec::new();
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(&dir).expect("crate sources must be readable") {
                let path = entry.expect("directory entry").path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                    continue;
                }
                let source = fs::read_to_string(&path).expect("source file must be UTF-8");
                scanned += 1;
                for captures in key_pattern.captures_iter(&source) {
                    let key = captures.get(1).expect("key capture").as_str();
                    if !english.contains_key(key) || !chinese.contains_key(key) {
                        missing.push(format!("{key} ({})", path.display()));
                    }
                }
            }
        }

        assert!(
            scanned > 100,
            "expected to scan the whole crate, saw {scanned} files"
        );
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "keys missing from a catalog: {missing:#?}"
        );
    }

    #[test]
    fn connection_editor_algorithm_and_telnet_labels_are_localized() {
        assert_eq!(text("en", "dialog.sshAlgorithms"), "SSH algorithms");
        assert_eq!(text("zh-CN", "dialog.sshAlgorithms"), "SSH 算法");
        assert_eq!(text("en", "dialog.telnetAutoLogin"), "Auto Login");
        assert_eq!(text("zh-CN", "dialog.telnetAutoLogin"), "自动登录");
        assert_eq!(
            rust_i18n::t!(
                "dialog.algorithmUnsupportedError",
                locale = "en",
                algorithm = "future-kex",
                category = "key exchanges"
            ),
            "future-kex is not supported in key exchanges."
        );
    }
}
