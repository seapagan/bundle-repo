use git2::{Repository, Signature};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;
use xml::reader::ParserConfig;

const COLOR_ENVIRONMENT: [&str; 6] = [
    "NO_COLOR",
    "FORCE_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "COLORTERM",
    "TERM",
];

fn initialize_repository(content: &str) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("example.txt"), content).unwrap();
    let repository = Repository::init(directory.path()).unwrap();
    repository.set_head("refs/heads/test-branch").unwrap();
    let mut index = repository.index().unwrap();
    index.add_path(Path::new("example.txt")).unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repository.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test", "test@example.com").unwrap();
    repository
        .commit(Some("HEAD"), &signature, &signature, "test", &tree, &[])
        .unwrap();
    drop(tree);
    drop(repository);
    directory
}

fn command(repository: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bundlerepo"));
    command.current_dir(repository);
    command.env("HOME", repository);
    command.env("USERPROFILE", repository);
    for variable in COLOR_ENVIRONMENT {
        command.env_remove(variable);
    }
    command
}

fn run_to_file(
    repository: &Path,
    output_path: &Path,
    environment: &[(&str, &str)],
) -> Output {
    let _ = fs::remove_file(output_path);
    let mut command = command(repository);
    command.arg("--file").arg(output_path);
    command.envs(environment.iter().copied());
    command.output().unwrap()
}

fn strip_sgr(bytes: &[u8]) -> Vec<u8> {
    let mut plain = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"\x1b[") {
            let mut end = index + 2;
            while end < bytes.len()
                && (bytes[end].is_ascii_digit() || bytes[end] == b';')
            {
                end += 1;
            }
            if bytes.get(end) == Some(&b'm') {
                index = end + 1;
                continue;
            }
        }
        plain.push(bytes[index]);
        index += 1;
    }
    plain
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn output_path(repository: &Path) -> PathBuf {
    repository.join("bundle.xml")
}

fn synthetic_github_pat() -> String {
    ["ghp_", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8"].concat()
}

#[test]
fn metadata_secrets_fail_before_repository_and_output_work_with_both_scan_modes()
 {
    let secret = synthetic_github_pat();
    for scan_mode in ["--secret-scan", "--no-secret-scan"] {
        for entry in [
            format!("safe_name = '{secret}'"),
            format!("'{secret}' = ''"),
        ] {
            let parent = tempfile::tempdir().unwrap();
            let directory = parent.path().join(&secret);
            fs::create_dir(&directory).unwrap();
            fs::write(
                directory.join(".bundlerepo.toml"),
                format!("[metadata]\n{entry}\n"),
            )
            .unwrap();
            let target = directory.join("missing/output.xml");
            let output = command(&directory)
                .arg(scan_mode)
                .arg("--file")
                .arg(&target)
                .env("TMPDIR", directory.join("missing/temp"))
                .env("TMP", directory.join("missing/temp"))
                .env("TEMP", directory.join("missing/temp"))
                .env("BUNDLEREPO_PHASE_TIMINGS", "1")
                .output()
                .unwrap();
            assert!(!contains_bytes(&output.stdout, secret.as_bytes()));
            assert!(!contains_bytes(&output.stderr, secret.as_bytes()));
            assert_eq!(output.status.code(), Some(1));
            assert!(contains_bytes(
                &output.stderr,
                b"Detected a secret in metadata"
            ));
            assert!(!contains_bytes(&output.stdout, b"Loading tokenizer"));
            assert!(!contains_bytes(&output.stdout, b"BundleRepo"));
            assert!(!contains_bytes(&output.stderr, b"Not a git repository"));
            assert!(!contains_bytes(&output.stderr, b"Failed to write XML"));
            assert!(!target.exists());
        }
    }
}

#[test]
fn metadata_secrets_fail_before_clone_and_stdout_output() {
    let directory = tempfile::tempdir().unwrap();
    let secret = synthetic_github_pat();
    fs::write(
        directory.path().join(".bundlerepo.toml"),
        format!("[metadata]\nname = '{secret}'\n"),
    )
    .unwrap();
    let output = command(directory.path())
        .args(["invalid-repository", "--stdout", "--no-secret-scan"])
        .env("BUNDLEREPO_PHASE_TIMINGS", "1")
        .output()
        .unwrap();
    assert!(!contains_bytes(&output.stdout, secret.as_bytes()));
    assert!(!contains_bytes(&output.stderr, secret.as_bytes()));
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(contains_bytes(
        &output.stderr,
        b"Detected a secret in metadata"
    ));
    assert!(!contains_bytes(&output.stderr, b"Loading tokenizer"));
    assert!(!contains_bytes(&output.stderr, b"Cloning"));
    assert!(!contains_bytes(&output.stderr, b"panicked"));
}

#[test]
fn default_captured_output_is_plain() {
    let repository = initialize_repository("example content");
    let output =
        run_to_file(repository.path(), &output_path(repository.path()), &[]);

    assert!(output.status.success());
    assert!(!contains_bytes(&output.stdout, b"\x1b["));
    assert!(!contains_bytes(&output.stderr, b"\x1b["));
}

#[test]
fn forced_colour_styles_only_human_output() {
    let repository = initialize_repository("example content");
    let path = output_path(repository.path());
    let plain = run_to_file(repository.path(), &path, &[]);
    let coloured =
        run_to_file(repository.path(), &path, &[("FORCE_COLOR", "1")]);

    assert!(plain.status.success());
    assert!(coloured.status.success());
    assert!(contains_bytes(
        &coloured.stdout,
        b"\x1b[1;36mBundleRepo Version"
    ));
    assert!(contains_bytes(&coloured.stdout, b"\x1b[36m->\x1b[0m"));
    assert!(contains_bytes(
        &coloured.stdout,
        b"\x1b[1;32mSuccessfully\x1b[0m"
    ));
    let styled_path = format!("\x1b[36m{}\x1b[0m", path.display());
    assert!(contains_bytes(&coloured.stdout, styled_path.as_bytes()));
    assert_eq!(strip_sgr(&coloured.stdout), plain.stdout);
    assert_eq!(strip_sgr(&coloured.stderr), plain.stderr);
}

#[test]
fn no_color_wins_over_forced_colour() {
    let repository = initialize_repository("example content");
    let path = output_path(repository.path());
    let plain = run_to_file(repository.path(), &path, &[]);
    let disabled = run_to_file(
        repository.path(),
        &path,
        &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")],
    );

    assert!(disabled.status.success());
    assert!(!contains_bytes(&disabled.stdout, b"\x1b["));
    assert!(!contains_bytes(&disabled.stderr, b"\x1b["));
    assert_eq!(disabled.stdout, plain.stdout);
    assert_eq!(disabled.stderr, plain.stderr);
}

#[test]
fn stderr_errors_use_stderr_colour_policy() {
    let repository = initialize_repository("example content");
    let missing = repository.path().join("missing/bundle.xml");
    let plain = run_to_file(repository.path(), &missing, &[]);
    let coloured =
        run_to_file(repository.path(), &missing, &[("FORCE_COLOR", "1")]);

    assert_eq!(plain.status.code(), Some(4));
    assert_eq!(coloured.status.code(), Some(4));
    assert!(!contains_bytes(&plain.stdout, b"X "));
    assert!(!contains_bytes(&coloured.stdout, b"X "));
    assert!(contains_bytes(&coloured.stderr, b"\x1b[1;31mX  \x1b[0m"));
    assert_eq!(strip_sgr(&coloured.stderr), plain.stderr);
}

#[test]
fn config_load_errors_use_the_error_prefix_style() {
    let repository = initialize_repository("example content");
    fs::write(repository.path().join(".bundlerepo.toml"), "model = [")
        .unwrap();
    let output = run_to_file(
        repository.path(),
        &output_path(repository.path()),
        &[("FORCE_COLOR", "1")],
    );

    assert!(output.status.success());
    assert!(contains_bytes(
        &output.stderr,
        b"\x1b[1;31mError:\x1b[0m loading config:"
    ));
    assert!(strip_sgr(&output.stderr).starts_with(b"Error: loading config:"));
}

#[test]
fn repository_discovery_diagnostic_remains_visible_in_quiet_mode() {
    let directory = tempfile::tempdir().unwrap();
    let output = command(directory.path())
        .arg("--stdout")
        .env("FORCE_COLOR", "1")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert_eq!(
        strip_sgr(&output.stderr),
        b"X  No git repository found in the current directory.\n\
          Error: Not a git repository\n"
    );
}

#[test]
fn warnings_are_coloured_only_on_stderr() {
    let repository = initialize_repository("before\u{0001}after");
    let path = output_path(repository.path());
    let coloured =
        run_to_file(repository.path(), &path, &[("FORCE_COLOR", "1")]);
    let plain = run_to_file(
        repository.path(),
        &path,
        &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")],
    );

    assert!(coloured.status.success());
    assert!(plain.status.success());
    assert!(!contains_bytes(&coloured.stdout, b"warning:"));
    assert!(contains_bytes(
        &coloured.stderr,
        b"\x1b[1;33mwarning:\x1b[0m"
    ));
    assert!(!contains_bytes(&plain.stderr, b"\x1b["));
    assert_eq!(strip_sgr(&coloured.stderr), plain.stderr);
}

#[test]
fn stdout_xml_never_contains_presentation() {
    let repository = initialize_repository("example content");
    let output = command(repository.path())
        .arg("--stdout")
        .env("FORCE_COLOR", "3")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.starts_with(b"<?xml version=\"1.0\""));
    assert!(!contains_bytes(&output.stdout, b"\x1b["));
    assert!(!contains_bytes(&output.stdout, b"BundleRepo"));
    assert!(!contains_bytes(&output.stdout, b"Summary:"));
    assert!(output.stderr.is_empty());
    for event in ParserConfig::new().create_reader(output.stdout.as_slice()) {
        event.unwrap();
    }
}

#[test]
fn stdout_redacts_secrets_without_stderr_leakage() {
    let secret = synthetic_github_pat();
    let repository = initialize_repository(&format!("token = {secret}"));
    let output = command(repository.path()).arg("--stdout").output().unwrap();

    assert!(output.status.success());
    assert!(!contains_bytes(&output.stdout, secret.as_bytes()));
    assert!(!contains_bytes(&output.stderr, secret.as_bytes()));
    assert!(contains_bytes(
        &output.stdout,
        b"[Secret removed: GitHub Personal Access Token]"
    ));
    assert!(output.stderr.is_empty());
    for event in ParserConfig::new().create_reader(output.stdout.as_slice()) {
        event.unwrap();
    }
}

#[test]
fn stdout_omits_secret_bearing_paths_without_leakage() {
    let secret = synthetic_github_pat();
    let repository = initialize_repository("safe content");
    fs::write(
        repository.path().join(format!("fixture-{secret}.txt")),
        "unsafe path",
    )
    .unwrap();
    let output = command(repository.path()).arg("--stdout").output().unwrap();

    assert!(output.status.success());
    assert!(!contains_bytes(&output.stdout, secret.as_bytes()));
    assert!(!contains_bytes(&output.stderr, secret.as_bytes()));
    assert!(contains_bytes(&output.stdout, b"<repository_skipped>"));
    assert!(contains_bytes(&output.stdout, b"reason=\"secret-in-path\""));
    assert!(output.stderr.is_empty());
    for event in ParserConfig::new().create_reader(output.stdout.as_slice()) {
        event.unwrap();
    }
}

#[test]
fn stdout_bundles_files_from_repository_with_unborn_head() {
    let repository = tempfile::tempdir().unwrap();
    Repository::init(repository.path()).unwrap();
    fs::write(repository.path().join("example.txt"), "unborn content")
        .unwrap();

    let output = command(repository.path()).arg("--stdout").output().unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(output.stdout.starts_with(b"<?xml version=\"1.0\""));
    assert!(contains_bytes(&output.stdout, b"example.txt"));
    assert!(contains_bytes(&output.stdout, b"unborn content"));
    for event in ParserConfig::new().create_reader(output.stdout.as_slice()) {
        event.unwrap();
    }
}

#[test]
fn generated_file_never_contains_presentation() {
    let repository = initialize_repository("example content");
    let path = output_path(repository.path());
    let output =
        run_to_file(repository.path(), &path, &[("FORCE_COLOR", "1")]);
    let xml = fs::read(path).unwrap();

    assert!(output.status.success());
    assert!(xml.starts_with(b"<?xml version=\"1.0\""));
    assert!(!contains_bytes(&xml, b"\x1b["));
    for event in ParserConfig::new().create_reader(xml.as_slice()) {
        event.unwrap();
    }
}

#[test]
fn timings_remain_plain_and_machine_readable() {
    let repository = initialize_repository("example content");
    let output = command(repository.path())
        .arg("--file")
        .arg(output_path(repository.path()))
        .env("FORCE_COLOR", "1")
        .env("BUNDLEREPO_PHASE_TIMINGS", "1")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains('\u{001b}'));
    let records = stderr.lines().collect::<Vec<_>>();
    assert!(!records.is_empty());
    for record in records {
        let remainder =
            record.strip_prefix("BUNDLEREPO_TIMING phase=").unwrap();
        let (phase, nanos) = remainder.split_once(" nanos=").unwrap();
        assert!(!phase.is_empty());
        nanos
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<u128>()
            .unwrap();
    }
}
