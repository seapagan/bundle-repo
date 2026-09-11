use std::fmt;
use std::path::Path;
use std::process::exit;
use std::time::Instant;

use clap::Parser;
use configuration::load_config;
use structs::Params;
use tabled::{
    Table, Tabled,
    settings::{
        Alignment, Modify, Remove, Style,
        object::{Columns, Rows},
    },
};
use tempfile::tempdir;
use tokenizer::{Model, TokenizerType};

mod cli;
mod configuration;
mod embedded;
mod filelist;
mod number_format;
mod presentation;
mod progress;
mod repo;
mod secret_scanning;
mod structs;
#[cfg(test)]
#[path = "../tests/crate/test_fixtures.rs"]
mod test_fixtures;
mod text_processing;
mod timings;
mod tokenizer;
mod xml_output;

#[derive(Tabled)]
struct SummaryTable {
    // metric: &'static str,
    metric: String,
    value: String,
}

fn report_success<N: std::io::Write, D: std::io::Write>(
    params: &Params,
    model: Model,
    metrics: (usize, u64, usize),
    formatter: &number_format::NumberFormatter,
    reporter: &mut progress::ProgressReporter<N, D>,
) -> std::io::Result<()> {
    if params.stdout {
        return Ok(());
    }

    if params.clipboard {
        reporter.success(" copied XML to clipboard")?;
    } else {
        let output_path = xml_output::effective_output_file(params);
        reporter.success_with_accent(
            " wrote XML to '",
            &output_path.display().to_string(),
            "'",
        )?;
    }

    let (number_of_files, total_size, token_count) = metrics;
    let summary_values = [
        formatter.format_count(number_of_files),
        formatter.format_output_size(total_size, params.gzip),
        formatter.format_count(token_count),
    ];
    let summary_data = vec![
        SummaryTable {
            metric: "Total Files processed:".to_string(),
            value: summary_values[0].clone(),
        },
        SummaryTable {
            metric: "Total output size (bytes):".to_string(),
            value: summary_values[1].clone(),
        },
        SummaryTable {
            metric: format!("Token count ({}):", model.display_name()),
            value: summary_values[2].clone(),
        },
    ];

    let table = Table::new(summary_data)
        .with(Remove::row(Rows::first()))
        .with(Style::empty())
        .with(Modify::list(Columns::first(), Alignment::right()))
        .to_string();

    reporter.summary(&table, &summary_values)
}

fn prepare_tokenizer<N: std::io::Write, D: std::io::Write>(
    params: &Params,
    reporter: &mut progress::ProgressReporter<N, D>,
    timings: &mut timings::ProcessingTimings,
) -> Result<(Model, TokenizerType), String> {
    let model = params.model.as_ref().unwrap().parse::<Model>()?;
    reporter
        .phase_with_accent("Loading tokenizer for ", model.display_name(), "")
        .unwrap();

    let tokenizer_start = Instant::now();
    let tokenizer = model.to_tokenizer().map_err(|error| {
        format!("Error: Failed to create tokenizer: {error}")
    })?;
    timings.tokenizer_load = tokenizer_start.elapsed();

    Ok((model, tokenizer))
}

#[derive(Debug)]
enum ApplicationError {
    Metadata(configuration::MetadataError),
    Tokenizer(String),
    FileSelection(String),
    Clone(git2::Error),
    CurrentDirectory(git2::Error),
    Output(std::io::Error),
    SecretScanner(String),
}

impl ApplicationError {
    const fn exit_code(&self) -> i32 {
        match self {
            Self::Metadata(_) => 1,
            Self::Tokenizer(_) => 1,
            Self::FileSelection(_) => 6,
            Self::Clone(_) => 2,
            Self::CurrentDirectory(_) => 3,
            Self::Output(_) => 4,
            Self::SecretScanner(_) => 5,
        }
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata(error) => write!(formatter, "Error: {error}"),
            Self::Tokenizer(error) => formatter.write_str(error),
            Self::FileSelection(error) => write!(formatter, "Error: {error}"),
            Self::Clone(error) | Self::CurrentDirectory(error) => {
                write!(formatter, "Error: {error}")
            }
            Self::Output(error) => {
                write!(formatter, "X  Failed to write XML: {error}")
            }
            Self::SecretScanner(error) => {
                write!(formatter, "Error: secret scanning failed: {error}")
            }
        }
    }
}

fn run_application<N: std::io::Write, D: std::io::Write>(
    args: &cli::Flags,
    params: &Params,
    repository_path: &Path,
    scanner_slot: &mut Option<secret_scanning::SecretScanner>,
    reporter: &mut progress::ProgressReporter<N, D>,
    timings: &mut timings::ProcessingTimings,
) -> Result<(), ApplicationError> {
    let (model, tokenizer) = prepare_tokenizer(params, reporter, timings)
        .map_err(ApplicationError::Tokenizer)?;
    let scanner = if params.secret_scan {
        ensure_secret_scanner(scanner_slot, reporter, timings)?;
        scanner_slot.as_ref()
    } else {
        None
    };
    let temp_dir = tempdir().unwrap();

    let repo_folder = if let Some(ref repo_input) = args.repo {
        repo::clone_repo(
            params,
            repo_input,
            params.token.as_deref(),
            temp_dir.path(),
            reporter,
        )
        .map_err(ApplicationError::Clone)?
    } else {
        repo::check_repository_at(repository_path, reporter)
            .map_err(ApplicationError::CurrentDirectory)?;
        repository_path.to_path_buf()
    };

    let file_list = filelist::list_files_in_repo(
        &repo_folder,
        &filelist::FileSelectionOptions {
            extend_exclude: params.extend_exclude.as_deref(),
            exclude: params.exclude.as_deref(),
            include: params.include.as_deref(),
            legacy_excludes: params.legacy_excludes,
        },
        reporter,
    )
    .map_err(ApplicationError::FileSelection)?;
    let path_scan =
        scan_repository_paths(file_list, scanner, reporter, timings)?;
    let file_tree = filelist::group_files_by_directory(path_scan.included);

    reporter.phase("Reading files and generating XML").unwrap();
    let metrics = xml_output::output_repo_as_xml_with_inventory_and_timings(
        params,
        xml_output::RepositoryInventory::new(file_tree, path_scan.skipped),
        &repo_folder,
        &tokenizer,
        model.display_name(),
        scanner,
        reporter,
        timings,
    )
    .map_err(classify_output_error)?;
    if params.stdout {
        return Ok(());
    }
    let formatter = number_format::NumberFormatter::system();
    report_success(params, model, metrics, &formatter, reporter).unwrap();

    Ok(())
}

fn scan_repository_paths<N: std::io::Write, D: std::io::Write>(
    file_list: Vec<String>,
    scanner: Option<&secret_scanning::SecretScanner>,
    reporter: &mut progress::ProgressReporter<N, D>,
    timings: &mut timings::ProcessingTimings,
) -> Result<secret_scanning::RepositoryPathScan, ApplicationError> {
    let Some(scanner) = scanner else {
        return Ok(secret_scanning::RepositoryPathScan::unscanned(file_list));
    };
    reporter
        .phase("Scanning repository paths for secrets")
        .unwrap();
    let started = Instant::now();
    let scan = scanner.scan_repository_paths(file_list);
    timings.secret_scanning += started.elapsed();
    let scan = scan
        .map_err(|error| ApplicationError::SecretScanner(error.to_string()))?;
    timings.findings_redacted += scan.findings;
    timings.path_items_skipped += scan.skipped.len();
    Ok(scan)
}

fn ensure_secret_scanner<N: std::io::Write, D: std::io::Write>(
    slot: &mut Option<secret_scanning::SecretScanner>,
    reporter: &mut progress::ProgressReporter<N, D>,
    timings: &mut timings::ProcessingTimings,
) -> Result<(), ApplicationError> {
    if slot.is_some() {
        return Ok(());
    }
    reporter.phase("Loading secret scanner").unwrap();
    let started = Instant::now();
    let scanner = secret_scanning::SecretScanner::from_bundled();
    timings.secret_scanner_load += started.elapsed();
    *slot = Some(scanner.map_err(|error| {
        ApplicationError::SecretScanner(error.to_string())
    })?);
    Ok(())
}

fn prepare_metadata<N: std::io::Write, D: std::io::Write>(
    sources: &[configuration::MetadataSource],
    params: &mut Params,
    scanner: &mut Option<secret_scanning::SecretScanner>,
    reporter: &mut progress::ProgressReporter<N, D>,
    timings: &mut timings::ProcessingTimings,
) -> Result<(), ApplicationError> {
    if !sources.iter().any(|source| !source.entries.is_empty()) {
        return Ok(());
    }
    ensure_secret_scanner(scanner, reporter, timings)?;
    let started = Instant::now();
    let metadata = configuration::validate_and_merge_metadata(
        sources,
        scanner.as_ref().unwrap(),
    );
    timings.secret_scanning += started.elapsed();
    params.metadata = metadata.map_err(|error| match error {
        configuration::MetadataError::Scanner { .. } => {
            ApplicationError::SecretScanner(error.to_string())
        }
        _ => ApplicationError::Metadata(error),
    })?;
    Ok(())
}

fn classify_output_error(error: std::io::Error) -> ApplicationError {
    let secret_scanner_error = error
        .get_ref()
        .is_some_and(|source| source.is::<secret_scanning::SecretScanError>());
    if secret_scanner_error {
        ApplicationError::SecretScanner(error.to_string())
    } else {
        ApplicationError::Output(error)
    }
}

fn load_config_or_exit() -> configuration::LoadedConfig {
    match load_config() {
        Ok(config) => config,
        Err(error) => {
            let mut reporter = progress::ProgressReporter::terminal(false);
            reporter.error(&format!("Error: {error}")).unwrap();
            exit(1);
        }
    }
}

fn main() {
    let args = cli::Flags::parse();
    let timing_enabled = timings::ProcessingTimings::enabled_from_env();
    let mut timings = timings::ProcessingTimings::default();

    if args.version {
        println!("{}", cli::version_info());
        exit(0);
    }

    // Load config values
    let loaded = load_config_or_exit();
    let mut params = Params::from_args_and_config(&args, loaded.params);
    let mut reporter = progress::ProgressReporter::terminal(params.stdout);

    if let Err(error) = xml_output::validate_output_options(&params) {
        reporter.error(&format!("Error: {error}")).unwrap();
        exit(1);
    }

    let mut scanner = None;
    if let Err(error) = prepare_metadata(
        &loaded.metadata_sources,
        &mut params,
        &mut scanner,
        &mut reporter,
        &mut timings,
    ) {
        reporter.error(&error.to_string()).unwrap();
        exit(error.exit_code());
    }

    if let Some(error) = loaded.legacy_error {
        reporter
            .error(&format!("Error: loading config: {error}"))
            .unwrap();
    }

    reporter
        .header(
            env!("CARGO_PKG_VERSION"),
            cli::AUTHOR,
            env!("CARGO_PKG_DESCRIPTION"),
        )
        .unwrap();

    match run_application(
        &args,
        &params,
        Path::new("."),
        &mut scanner,
        &mut reporter,
        &mut timings,
    ) {
        Ok(()) => {
            if timing_enabled {
                let _ = timings.write_records(&mut std::io::stderr().lock());
            }
        }
        Err(error) => {
            reporter.error(&error.to_string()).unwrap();
            exit(error.exit_code());
        }
    }
}

#[cfg(test)]
#[path = "../tests/crate/app.rs"]
mod tests;
