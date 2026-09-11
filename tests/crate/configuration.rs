use super::*;
use std::fs;
use tempfile::tempdir;

fn load_local(source: &str) -> LoadedConfig {
    let temp_dir = tempdir().unwrap();
    let local = temp_dir.path().join(".bundlerepo.toml");
    fs::write(&local, source).unwrap();
    load_config_from_paths(None, &local).unwrap()
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

#[path = "configuration/metadata.rs"]
mod metadata;
