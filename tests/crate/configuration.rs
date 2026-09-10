use super::*;
use std::fs;
use tempfile::tempdir;

fn metadata_scanner() -> &'static crate::secret_scanning::SecretScanner {
    static SCANNER: std::sync::OnceLock<
        crate::secret_scanning::SecretScanner,
    > = std::sync::OnceLock::new();
    SCANNER.get_or_init(|| {
        crate::secret_scanning::SecretScanner::from_bundled().unwrap()
    })
}

fn validate_sources(
    global: &str,
    local: &str,
) -> Result<std::collections::BTreeMap<String, String>, MetadataError> {
    let sources = [
        (global, ConfigSourceIdentity::Global),
        (local, ConfigSourceIdentity::RepositoryLocal),
    ]
    .into_iter()
    .filter_map(|(text, identity)| parse_metadata(text, identity).unwrap())
    .collect::<Vec<_>>();
    validate_and_merge_metadata(&sources, metadata_scanner())
}

#[test]
fn test_metadata_validation_rejects_empty_and_xml_invalid_keys_and_values() {
    for entry in [
        r#""" = "safe""#,
        r#""   " = "safe""#,
        r#""a\tkey" = "safe""#,
        r#""a\u000Bkey" = "safe""#,
        r#"key = "bad\u000Bvalue""#,
    ] {
        let error = metadata_error(validate_sources(
            "",
            &format!("[metadata]\n{entry}\n"),
        ));
        assert!(error.to_string().contains("line 2"));
        assert!(!error.to_string().contains("false positive"));
    }
}

#[test]
fn test_metadata_merge_preserves_exact_content_and_removes_only_empty_values()
{
    let map = validate_sources(
        "[metadata]\nshared = 'global'\nremove = 'global'\nGlobal = 'yes'\nCase = 'upper'\n",
        "[metadata]\nshared = 'local'\nremove = '  '\nempty = ''\ncase = 'lower'\n' <&雪> ' = \"  <&\\\"'雪\\n\\t  \"\n",
    ).unwrap();
    assert_eq!(map.len(), 5);
    assert_eq!(map["shared"], "local");
    assert_eq!(map["Global"], "yes");
    assert_eq!(map["Case"], "upper");
    assert_eq!(map["case"], "lower");
    assert_eq!(map[" <&雪> "], "  <&\"'雪\n\t  ");
    assert!(!map.contains_key("remove"));
    assert!(!map.contains_key("empty"));
}

#[test]
fn test_metadata_validates_all_source_entries_before_override_or_suppression()
{
    let secret = crate::secret_scanning::synthetic_github_pat();
    let cases = [
        (format!("name = '{secret}'"), "name = 'safe'".to_string()),
        (format!("'{secret}' = 'safe'"), "name = 'safe'".to_string()),
        (format!("name = '{secret}'"), "name = ''".to_string()),
        ("name = 'safe'".to_string(), format!("'{secret}' = ''")),
        ("name = 42".to_string(), "name = 'safe'".to_string()),
    ];
    for (global, local) in cases {
        let error = metadata_error(validate_sources(
            &format!("[metadata]\n{global}"),
            &format!("[metadata]\n{local}"),
        ));
        assert!(!format!("{error} {error:?}").contains(&secret));
    }
}

#[test]
fn test_metadata_secret_diagnostics_are_anonymous_or_use_a_scanned_key() {
    let secret = crate::secret_scanning::synthetic_github_pat();
    let guidance = "If you believe this detection is a false positive, please report it at https://github.com/seapagan/bundle-repo/issues.";
    for (entry, key_visible) in [
        (format!("'{secret}' = 42"), false),
        (format!("safe_name = '{secret}'"), true),
        (format!("safe_name = '''\nordinary\n{secret}\n'''"), true),
    ] {
        let error = metadata_error(validate_sources(
            "",
            &format!("[metadata]\n{entry}"),
        ));
        let display = error.to_string();
        assert!(!format!("{error} {error:?}").contains(&secret));
        assert_eq!(display.contains("safe_name"), key_visible);
        assert!(
            display
                .contains("repository-local .bundlerepo.toml configuration")
        );
        assert!(display.contains("line 2"));
        assert!(display.ends_with(guidance));
        assert_eq!(display.matches(guidance).count(), 1);
    }
}

#[test]
fn test_contextual_bundled_secret_with_full_match_is_anonymous() {
    let value = "a9b8c7d6e5f4g3h2i1j0k9l8m7n6o5p4";
    let source = format!("[metadata]\nadafruit_api_key = '{value}'\n");
    let loaded = load_local(&source);
    let error = metadata_error(validate_and_merge_metadata(
        &loaded.metadata_sources,
        metadata_scanner(),
    ));
    let diagnostic = format!("{error} {error:?}");

    assert!(!diagnostic.contains("adafruit_api_key"));
    assert!(diagnostic.contains("line 2"));
    assert!(!diagnostic.contains(value));
    assert!(!diagnostic.contains(&format!("adafruit_api_key = {value}")));
}

#[test]
fn test_contextual_value_secret_reports_safe_key() {
    let rules = r#"
[[rules]]
id = 'contextual-value'
regex = 'safe_name = ([A-Z]{8})'
keywords = ['safe_name']
secretGroup = 1
"#;
    let scanner =
        crate::secret_scanning::SecretScanner::from_rules_for_workers(
            rules, 1,
        )
        .unwrap();
    let loaded = load_local("[metadata]\nsafe_name = 'SECRETAA'\n");
    let error = metadata_error(validate_and_merge_metadata(
        &loaded.metadata_sources,
        &scanner,
    ));
    let diagnostic = format!("{error} {error:?}");

    assert!(diagnostic.contains("entry 'safe_name'"));
    assert!(diagnostic.contains("line 2"));
    assert!(!diagnostic.contains("SECRETAA"));
}

#[test]
fn test_contextual_key_secret_diagnostic_is_anonymous_everywhere() {
    let rules = r#"
[[rules]]
id = 'contextual-key'
regex = '(contextual_secret_key) = ordinary'
keywords = ['contextual_secret_key']
secretGroup = 1
"#;
    let scanner =
        crate::secret_scanning::SecretScanner::from_rules_for_workers(
            rules, 1,
        )
        .unwrap();
    let loaded =
        load_local("[metadata]\ncontextual_secret_key = 'ordinary'\n");
    let error = metadata_error(validate_and_merge_metadata(
        &loaded.metadata_sources,
        &scanner,
    ));
    let display = error.to_string();
    let debug = format!("{error:?}");
    let mut reporter =
        crate::progress::ProgressReporter::new(Vec::new(), Vec::new(), false);
    reporter.error(&display).unwrap();
    let (stdout, stderr) = reporter.into_parts();

    for diagnostic in [display.as_bytes(), debug.as_bytes(), &stdout, &stderr]
    {
        assert!(
            !diagnostic
                .windows(b"contextual_secret_key".len())
                .any(|window| window == b"contextual_secret_key")
        );
    }
    assert!(display.contains("Detected a secret in metadata key"));
    assert!(display.contains("line 2"));
}

#[test]
fn test_metadata_validation_order_protects_keys_and_reports_first_error() {
    let secret = crate::secret_scanning::synthetic_github_pat();
    for (entry, reason) in [
        (format!("'{secret}' = 42"), "secret"),
        ("'' = 42".to_string(), "empty"),
        ("\"a\\tkey\" = 42".to_string(), "U+0009"),
        ("key = [1]".to_string(), "array"),
        (format!("key = \"{secret}\\u000B\""), "secret"),
        ("key = \"bad\\u000Bvalue\"".to_string(), "U+000B"),
    ] {
        let error = metadata_error(validate_sources(
            &format!("[metadata]\n{entry}"),
            "[metadata]\nother = 42",
        ));
        let message = error.to_string();
        assert!(!format!("{error} {error:?}").contains(&secret));
        assert!(message.contains(reason));
        assert!(message.contains("global BundleRepo configuration"));
        assert!(!message.contains("other"));
    }
}

fn load_local(source: &str) -> LoadedConfig {
    let temp_dir = tempdir().unwrap();
    let local = temp_dir.path().join(".bundlerepo.toml");
    fs::write(&local, source).unwrap();
    load_config_from_paths(None, &local).unwrap()
}

fn metadata_error<T>(result: Result<T, MetadataError>) -> MetadataError {
    match result {
        Ok(_) => panic!("expected metadata error"),
        Err(error) => error,
    }
}

#[test]
fn test_metadata_unvalidated_type_error_never_owns_a_secret_key() {
    let secret = crate::secret_scanning::synthetic_github_pat();
    let loaded = load_local(&format!("[metadata]\n'{secret}' = 42\n"));
    let source = &loaded.metadata_sources[0];
    let error = source.entries[0].string_value(source.identity).unwrap_err();
    assert!(!format!("{error} {error:?}").contains(&secret));
}

#[test]
fn test_metadata_preserves_exact_keys_values_lines_and_source_order() {
    let temp_dir = tempdir().unwrap();
    let global = temp_dir.path().join("global.toml");
    let local = temp_dir.path().join("local.toml");
    fs::write(
        &global,
        concat!(
            "model = \"gpt4\"\n",
            "[metadata]\n",
            "bare-key = \" global value \"\n",
            "\"quoted.key\" = \"quoted\\nvalue\"\n",
        ),
    )
    .unwrap();
    fs::write(
        &local,
        concat!(
            "[metadata]\n",
            "basic = \"\"\"\nfirst\nsecond\"\"\"\n",
            "literal = '''\nliteral\nvalue'''\n",
        ),
    )
    .unwrap();

    let loaded = load_config_from_paths(Some(&global), &local).unwrap();

    assert_eq!(loaded.params.model.as_deref(), Some("gpt4"));
    assert_eq!(loaded.metadata_sources.len(), 2);
    let global = &loaded.metadata_sources[0];
    assert_eq!(global.identity, ConfigSourceIdentity::Global);
    assert_eq!(global.entries[0].key, "bare-key");
    assert_eq!(global.entries[0].key_line, 3);
    assert_eq!(global.entries[0].value_line, 3);
    assert_eq!(
        global.entries[0].string_value(global.identity).unwrap(),
        " global value "
    );
    assert_eq!(global.entries[1].key, "quoted.key");
    assert_eq!(
        global.entries[1].string_value(global.identity).unwrap(),
        "quoted\nvalue"
    );

    let local = &loaded.metadata_sources[1];
    assert_eq!(local.identity, ConfigSourceIdentity::RepositoryLocal);
    assert_eq!(local.entries[0].key, "basic");
    assert_eq!(local.entries[0].key_line, 2);
    assert_eq!(local.entries[0].value_line, 2);
    assert_eq!(
        local.entries[0].string_value(local.identity).unwrap(),
        "first\nsecond"
    );
    assert_eq!(local.entries[1].key, "literal");
    assert_eq!(local.entries[1].key_line, 5);
    assert_eq!(local.entries[1].value_line, 5);
    assert_eq!(
        local.entries[1].string_value(local.identity).unwrap(),
        "literal\nvalue"
    );
}

#[test]
fn test_source_location_is_one_based_and_unicode_aware() {
    let source = "alpha\nβeta\r\nomega";
    assert_eq!(source_location(source, 0), (1, 1));
    assert_eq!(source_location(source, 6), (2, 1));
    assert_eq!(source_location(source, 7), (2, 1));
    assert_eq!(source_location(source, 8), (2, 2));
    assert_eq!(source_location(source, source.len()), (3, 6));
}

#[test]
fn test_metadata_rejects_each_non_string_type_safely() {
    let secret = crate::secret_scanning::synthetic_github_pat();
    let cases = [
        ("integer", "count = 42".to_string(), "count"),
        ("float", "ratio = 1.5".to_string(), "ratio"),
        ("boolean", "enabled = true".to_string(), "enabled"),
        ("array", "items = [1, 2]".to_string(), "items"),
        (
            "table",
            format!("inline = {{ payload = \"{secret}\" }}"),
            "inline",
        ),
        (
            "table",
            format!("[metadata.nested]\npayload = \"{secret}\""),
            "nested",
        ),
        (
            "datetime",
            "offset_datetime = 1979-05-27T07:32:00Z".to_string(),
            "offset_datetime",
        ),
        (
            "datetime",
            "local_datetime = 1979-05-27T07:32:00".to_string(),
            "local_datetime",
        ),
        (
            "datetime",
            "local_date = 1979-05-27".to_string(),
            "local_date",
        ),
        (
            "datetime",
            "local_time = 07:32:00".to_string(),
            "local_time",
        ),
    ];

    for (category, entry, key) in cases {
        let loaded = load_local(&format!("[metadata]\n{entry}\n"));
        let error = metadata_error(validate_and_merge_metadata(
            &loaded.metadata_sources,
            metadata_scanner(),
        ));
        let message = error.to_string();

        assert!(
            message
                .contains("repository-local .bundlerepo.toml configuration")
        );
        assert!(message.contains("line 2"));
        assert!(message.contains(key));
        assert!(message.contains(category));
        assert!(!message.contains(&secret));
        assert!(!message.contains("false positive"));
        assert!(!message.contains("github.com/seapagan/bundle-repo/issues"));
    }
}

#[test]
fn test_metadata_array_with_secret_like_content_remains_a_safe_type_error() {
    let secret = crate::secret_scanning::synthetic_github_pat();
    let loaded = load_local(&format!("[metadata]\nitems = [\"{secret}\"]\n"));
    let error = metadata_error(validate_and_merge_metadata(
        &loaded.metadata_sources,
        metadata_scanner(),
    ));
    let message = error.to_string();

    assert!(message.contains("items"));
    assert!(message.contains("array"));
    assert!(!message.contains(&secret));
    assert!(!message.contains("false positive"));
}

#[test]
fn test_parse_error_inside_metadata_is_fatal_and_safe() {
    let temp_dir = tempdir().unwrap();
    let local = temp_dir.path().join(".bundlerepo.toml");
    fs::write(&local, "[metadata]\nentry = [\n").unwrap();

    let message =
        metadata_error(load_config_from_paths(None, &local)).to_string();

    assert!(
        message.contains("repository-local .bundlerepo.toml configuration")
    );
    assert!(message.contains("line 2"));
    assert!(message.contains("column"));
    assert!(!message.contains("entry = ["));
    assert!(!message.contains(&local.display().to_string()));
}

#[test]
fn test_spanless_recursion_error_inside_metadata_is_fatal_and_safe() {
    let key = (0..81)
        .map(|index| format!("part{index}"))
        .collect::<Vec<_>>()
        .join(".");
    let input = format!("[metadata]\n{key} = 'safe'\n");
    let (_, errors) = DeTable::parse_recoverable(&input);

    assert!(errors.iter().any(|error| error.span().is_none()));
    let error = metadata_error(parse_metadata(
        &input,
        ConfigSourceIdentity::RepositoryLocal,
    ));
    let diagnostic = format!("{error} {error:?}");

    assert!(matches!(error, MetadataError::Parse { line: 2, .. }));
    assert!(diagnostic.contains("recursion limit"));
    assert!(!diagnostic.contains(&key));
    assert!(!diagnostic.contains("part80"));
}

#[test]
fn test_spanless_recursion_error_outside_metadata_keeps_legacy_fallback() {
    let key = (0..81)
        .map(|index| format!("part{index}"))
        .collect::<Vec<_>>()
        .join(".");
    let loaded = load_local(&format!("{key} = 'safe'\n"));

    assert_eq!(loaded.params, crate::structs::Params::default());
    assert!(loaded.legacy_error.is_some());
    assert!(loaded.metadata_sources.is_empty());
}

#[test]
fn test_parse_error_outside_metadata_keeps_legacy_fallback() {
    let loaded =
        load_local("[metadata]\nname = \"safe\"\n[other]\nvalue = [\n");

    assert_eq!(loaded.params, crate::structs::Params::default());
    assert!(loaded.legacy_error.is_some());
    assert_eq!(loaded.metadata_sources.len(), 1);
}

#[test]
fn test_interleaved_dotted_metadata_errors_are_fatal_in_both_sources() {
    let secret = crate::secret_scanning::synthetic_github_pat();
    for entry in [
        format!("name = '{secret}'"),
        "later = [".to_string(),
        "later = \"unfinished".to_string(),
        "later =".to_string(),
        "later 'missing separator'".to_string(),
    ] {
        let input = format!(
            "metadata.name = 'safe'\nmodel = 'gpt5'\nmetadata.{entry}\n"
        );
        assert_metadata_parse_error_in_both_sources(&input, 3, &secret);
    }
}

#[test]
fn test_malformed_root_metadata_headers_are_fatal_in_both_sources() {
    for header in ["[metadata", "['metadata'", r#"["meta\u0064ata""#] {
        assert_metadata_parse_error_in_both_sources(
            &format!("{header}\nname = 'safe'\n"),
            1,
            "name = 'safe'",
        );
    }
}

fn assert_metadata_parse_error_in_both_sources(
    input: &str,
    expected_line: usize,
    private_text: &str,
) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let missing = directory.path().join("missing.toml");
    fs::write(&path, input).unwrap();
    for (global, local, identity) in [
        (Some(path.as_path()), &missing, ConfigSourceIdentity::Global),
        (None, &path, ConfigSourceIdentity::RepositoryLocal),
    ] {
        let error = metadata_error(load_config_from_paths(global, local));
        assert!(matches!(error, MetadataError::Parse { line, .. }
            if line == expected_line));
        let diagnostic = format!("{error} {error:?}");
        assert!(diagnostic.contains(&identity.to_string()));
        assert!(!diagnostic.contains(private_text));
        assert!(!diagnostic.contains(&path.display().to_string()));
    }
}

#[test]
fn test_metadata_lookalikes_and_unrelated_errors_keep_legacy_fallback() {
    for input in [
        "# [metadata\nmodel = [\n",
        "model = '[metadata'\nother = [\n",
        "model = '''\n[metadata\n'''\nother = [\n",
        "[metadatax\nname = 'safe'\n",
        "[other.metadata\nname = 'safe'\n",
        "[other.metadata]\nname = [\n",
        "other = { metadata = 'safe' }\nmodel = [\n",
        "metadata.name = 'safe'\nmodel = [\n",
        "metadata.name = 'safe'\n[other.metadata]\nname = [\n",
        "model = [\n",
    ] {
        let loaded = load_local(input);
        assert!(loaded.legacy_error.is_some());
        assert_eq!(loaded.params, crate::structs::Params::default());
    }
}

#[test]
fn test_malformed_non_metadata_config_keeps_legacy_fallback() {
    let loaded = load_local("model = [\n");

    assert_eq!(loaded.params, crate::structs::Params::default());
    assert!(loaded.legacy_error.is_some());
    assert!(loaded.metadata_sources.is_empty());
}

#[test]
fn test_non_table_metadata_is_a_safe_type_error() {
    let temp_dir = tempdir().unwrap();
    let global = temp_dir.path().join("global.toml");
    fs::write(&global, "metadata = 42\n").unwrap();

    let message = metadata_error(load_config_from_paths(
        Some(&global),
        temp_dir.path().join("missing").as_path(),
    ))
    .to_string();

    assert!(message.contains("global BundleRepo configuration"));
    assert!(message.contains("line 1"));
    assert!(message.contains("metadata"));
    assert!(message.contains("integer"));
    assert!(!message.contains(&global.display().to_string()));
}

#[test]
fn test_string_metadata_root_is_a_safe_type_error() {
    let temp_dir = tempdir().unwrap();
    let local = temp_dir.path().join(".bundlerepo.toml");
    fs::write(&local, "metadata = \"not a map\"\n").unwrap();

    let message =
        metadata_error(load_config_from_paths(None, &local)).to_string();

    assert!(
        message.contains("repository-local .bundlerepo.toml configuration")
    );
    assert!(message.contains("line 1"));
    assert!(message.contains("metadata"));
    assert!(message.contains("table"));
    assert!(message.contains("string"));
    assert!(!message.contains("not a map"));
    assert!(!message.contains(&local.display().to_string()));
}

#[test]
fn test_metadata_error_never_discloses_user_controlled_source_path() {
    let temp_dir = tempdir().unwrap();
    let secret = crate::secret_scanning::synthetic_github_pat();
    let parent = temp_dir.path().join(&secret);
    fs::create_dir(&parent).unwrap();
    let local = parent.join(".bundlerepo.toml");
    fs::write(&local, "[metadata]\nentry = 42\n").unwrap();

    let loaded = load_config_from_paths(None, &local).unwrap();
    let error = metadata_error(validate_and_merge_metadata(
        &loaded.metadata_sources,
        metadata_scanner(),
    ));
    let message = format!("{error} {error:?}");

    assert!(
        message.contains("repository-local .bundlerepo.toml configuration")
    );
    assert!(message.contains("line 2"));
    assert!(!message.contains(&secret));
    assert!(!message.contains(&local.display().to_string()));
}
