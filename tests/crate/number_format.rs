use super::*;
use std::cell::RefCell;
use std::collections::HashMap;

#[test]
fn test_grouped_integers_follow_explicit_locales() {
    let cases = [
        ("en-US", "21,344,532"),
        ("de-DE", "21.344.532"),
        ("fr-FR", "21\u{202f}344\u{202f}532"),
        ("en-IN", "2,13,44,532"),
    ];

    for (locale, expected) in cases {
        assert_eq!(
            NumberFormatter::from_locale(locale).format_count(21_344_532),
            expected
        );
    }
}

#[test]
fn test_decimal_separator_matches_exact_integer_locale() {
    assert_eq!(
        NumberFormatter::from_locale("en-US")
            .format_output_size(61_997_276, false),
        "61,997,276 (59.1 MiB)"
    );
    assert_eq!(
        NumberFormatter::from_locale("de-DE")
            .format_output_size(61_997_276, false),
        "61.997.276 (59,1 MiB)"
    );
    assert_eq!(
        NumberFormatter::from_locale("fr-FR")
            .format_output_size(61_997_276, false),
        "61\u{202f}997\u{202f}276 (59,1 MiB)"
    );
}

#[test]
fn test_c_and_posix_locales_are_ungrouped() {
    for locale in ["C", "C.UTF-8", "POSIX", "posix"] {
        assert_eq!(
            NumberFormatter::from_locale(locale)
                .format_output_size(61_997_276, false),
            "61997276 (59.1 MiB)"
        );
    }
}

#[test]
fn test_posix_locale_normalization_uses_core_parser() {
    assert_eq!(
        NumberFormatter::from_locale("de_DE.UTF-8@euro")
            .format_count(21_344_532),
        "21.344.532"
    );
}

#[test]
fn test_latn_numbering_system_overrides_locale_extension() {
    assert_eq!(
        NumberFormatter::from_locale("th-u-nu-thai").format_count(1_000_007),
        "1,000,007"
    );
}

#[test]
fn test_unix_numeric_locale_precedence_and_message_isolation() {
    let environment = HashMap::from([
        ("LC_ALL", "  "),
        ("LC_NUMERIC", "de_DE.UTF-8"),
        ("LANG", "fr_FR.UTF-8"),
        ("LANGUAGE", "hi_IN"),
        ("LC_MESSAGES", "en_US"),
    ]);
    let queried = RefCell::new(Vec::new());

    let selected = resolve_locale_candidate(
        PlatformLocaleSource::NativeUnix,
        |key| {
            queried.borrow_mut().push(key.to_string());
            environment.get(key).map(ToString::to_string)
        },
        || Some("it-IT".to_string()),
    );

    assert_eq!(selected.as_deref(), Some("de_DE.UTF-8"));
    assert_eq!(&*queried.borrow(), &["LC_ALL", "LC_NUMERIC"]);
}

#[test]
fn test_lang_wins_over_message_locale_variables() {
    let environment = HashMap::from([
        ("LC_ALL", ""),
        ("LC_NUMERIC", ""),
        ("LANG", "fr_FR.UTF-8"),
        ("LANGUAGE", "hi_IN"),
        ("LC_MESSAGES", "de_DE"),
    ]);

    let selected = resolve_locale_candidate(
        PlatformLocaleSource::GenericUnix,
        |key| environment.get(key).map(ToString::to_string),
        || Some("it-IT".to_string()),
    );

    assert_eq!(selected.as_deref(), Some("fr_FR.UTF-8"));
}

#[test]
fn test_unix_lc_all_has_highest_precedence() {
    let environment = HashMap::from([
        ("LC_ALL", "en_IN"),
        ("LC_NUMERIC", "de_DE"),
        ("LANG", "fr_FR"),
    ]);

    let selected = resolve_locale_candidate(
        PlatformLocaleSource::GenericUnix,
        |key| environment.get(key).map(ToString::to_string),
        || None,
    );

    assert_eq!(selected.as_deref(), Some("en_IN"));
}

#[test]
fn test_native_fallback_is_backend_specific() {
    let native = || Some("de-DE".to_string());
    let apple = resolve_locale_candidate(
        PlatformLocaleSource::NativeUnix,
        |_| None,
        native,
    );
    let generic = resolve_locale_candidate(
        PlatformLocaleSource::GenericUnix,
        |_| None,
        native,
    );

    assert_eq!(apple.as_deref(), Some("de-DE"));
    assert!(generic.is_none());
}

#[test]
fn test_generic_unix_ignores_message_and_native_fallbacks() {
    let environment =
        HashMap::from([("LANGUAGE", "hi_IN"), ("LC_MESSAGES", "de_DE")]);
    let queried = RefCell::new(Vec::new());

    let selected = resolve_locale_candidate(
        PlatformLocaleSource::GenericUnix,
        |key| {
            queried.borrow_mut().push(key.to_string());
            environment.get(key).map(ToString::to_string)
        },
        || panic!("generic Unix must not use native message-locale discovery"),
    );

    assert!(selected.is_none());
    assert_eq!(&*queried.borrow(), &["LC_ALL", "LC_NUMERIC", "LANG"]);
}

#[test]
fn test_windows_uses_only_native_locale() {
    let selected = resolve_locale_candidate(
        PlatformLocaleSource::Windows,
        |_| panic!("Windows must not query POSIX locale variables"),
        || Some("de-DE".to_string()),
    );

    assert_eq!(selected.as_deref(), Some("de-DE"));
}

#[test]
fn test_malformed_selected_locale_uses_english_fallback() {
    let environment = HashMap::from([
        ("LC_ALL", "malformed locale!"),
        ("LC_NUMERIC", "de_DE"),
        ("LANG", "fr_FR"),
    ]);
    let selected = resolve_locale_candidate(
        PlatformLocaleSource::NativeUnix,
        |key| environment.get(key).map(ToString::to_string),
        || Some("it-IT".to_string()),
    );
    let formatter = NumberFormatter::from_candidate(selected.as_deref());

    assert_eq!(selected.as_deref(), Some("malformed locale!"));
    assert_eq!(formatter.format_count(21_344_532), "21,344,532");
}

#[test]
fn test_absent_locale_uses_english_fallback() {
    assert_eq!(
        NumberFormatter::from_candidate(None).format_count(21_344_532),
        "21,344,532"
    );
}

#[test]
fn test_terminal_english_fallback_is_infallible() {
    let attempts = RefCell::new(Vec::new());
    let formatter =
        NumberFormatter::from_candidate_with(Some("de-DE"), |locale| {
            attempts.borrow_mut().push(locale.to_string());
            None
        });

    assert_eq!(&*attempts.borrow(), &["de-DE", "en-US"]);
    assert_eq!(formatter.format_count(21_344_532), "21,344,532");
    assert_eq!(
        formatter.format_output_size(61_997_276, false),
        "61,997,276 (59.1 MiB)"
    );
}

#[test]
fn test_byte_units_round_and_promote_without_overflow() {
    let formatter = NumberFormatter::from_locale("en-US");
    let cases = [
        (0, "0 (0 bytes)"),
        (1, "1 (1 byte)"),
        (1023, "1,023 (1,023 bytes)"),
        (1024, "1,024 (1.0 KiB)"),
        (1075, "1,075 (1.0 KiB)"),
        (1076, "1,076 (1.1 KiB)"),
        (1536, "1,536 (1.5 KiB)"),
        (1_048_575, "1,048,575 (1.0 MiB)"),
        (1_048_576, "1,048,576 (1.0 MiB)"),
        (1_073_741_823, "1,073,741,823 (1.0 GiB)"),
        (1_073_741_824, "1,073,741,824 (1.0 GiB)"),
        (u64::MAX, "18,446,744,073,709,551,615 (16.0 EiB)"),
    ];

    for (bytes, expected) in cases {
        assert_eq!(formatter.format_output_size(bytes, false), expected);
    }
}

#[test]
fn test_compressed_qualifier_changes_only_parenthetical() {
    let formatter = NumberFormatter::from_locale("en-US");
    assert_eq!(
        formatter.format_output_size(15_084_711, false),
        "15,084,711 (14.4 MiB)"
    );
    assert_eq!(
        formatter.format_output_size(15_084_711, true),
        "15,084,711 (14.4 MiB, compressed)"
    );
}
