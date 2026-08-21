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

/// Translate against an explicit language rather than the current global locale.
pub(crate) fn text(language: &str, key: &'static str) -> Cow<'static, str> {
    let locale = normalize_locale(language);
    rust_i18n::t!(key, locale = locale.as_ref())
}

fn is_simplified_chinese(language: &str) -> bool {
    let normalized = language.to_ascii_lowercase();
    normalized == "zh" || normalized == "zh-cn" || normalized.starts_with("zh-hans")
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use regex::Regex;
    use serde_json::Value;

    use super::{normalize_locale, text};

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

    #[test]
    fn english_and_chinese_catalogs_have_identical_keys() {
        let english = flatten(EN_JSON).into_keys().collect::<HashSet<_>>();
        let chinese = flatten(ZH_CN_JSON).into_keys().collect::<HashSet<_>>();
        assert_eq!(english, chinese);
    }

    #[test]
    fn connection_editor_static_translation_keys_exist_in_both_catalogs() {
        let sources = [
            include_str!("../features/pages/connections/editor/mod.rs"),
            include_str!("../features/pages/connections/editor/connection/mod.rs"),
            include_str!("../features/pages/connections/editor/connection/local.rs"),
            include_str!("../features/pages/connections/editor/connection/rdp.rs"),
            include_str!("../features/pages/connections/editor/connection/recording.rs"),
            include_str!("../features/pages/connections/editor/connection/serial.rs"),
            include_str!("../features/pages/connections/editor/connection/ssh.rs"),
            include_str!("../features/pages/connections/editor/connection/telnet.rs"),
            include_str!("../features/connections/connection_runtime/editor.rs"),
            include_str!("../features/connections/connection_runtime/window.rs"),
            include_str!("../features/connections/state/editor_logic.rs"),
        ];
        let key_pattern =
            Regex::new(r#"(?:\btr|self\.tr|I18n)\(\s*"([^"]+)""#).expect("translation-key regex");
        let english = flatten(EN_JSON);
        let chinese = flatten(ZH_CN_JSON);
        let mut missing = Vec::new();
        for source in sources {
            for captures in key_pattern.captures_iter(source) {
                let key = captures.get(1).expect("key capture").as_str();
                if !english.contains_key(key) || !chinese.contains_key(key) {
                    missing.push(key.to_string());
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "missing connection editor keys: {missing:?}"
        );
    }

    #[test]
    fn connection_editor_algorithm_and_telnet_labels_are_localized() {
        assert_eq!(text("en", "dialog.sshAlgorithms"), "SSH algorithms");
        assert_eq!(text("zh-CN", "dialog.sshAlgorithms"), "SSH 算法");
        assert_eq!(text("en", "dialog.telnetAutoLogin"), "Auto Login");
        assert_eq!(text("zh-CN", "dialog.telnetAutoLogin"), "自动登录");
        assert_eq!(
            text("en", "dialog.algorithmUnsupportedError")
                .replace("{{algorithm}}", "future-kex")
                .replace("{{category}}", "key exchanges"),
            "future-kex is not supported in key exchanges."
        );
    }
}
