use crate::progress::ProgressReporter;
use crate::structs::{DEFAULT_OUTPUT_FILE, Params};
use crate::timings::ProcessingTimings;
use crate::tokenizer::TokenizerType;
use arboard::Clipboard;
use dirs_next::home_dir;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn validate_output_options(flags: &Params) -> io::Result<()> {
    validate_output_options_for(flags, io::stdout().is_terminal())
}

pub(super) fn validate_output_options_for(
    flags: &Params,
    stdout_is_terminal: bool,
) -> io::Result<()> {
    if flags.gzip && flags.clipboard && !flags.stdout {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "gzip output cannot be copied to the clipboard; use --no-gzip --clipboard",
        ));
    }

    if flags.gzip && flags.stdout && stdout_is_terminal {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to write gzip data to a terminal; redirect stdout or use --no-gzip",
        ));
    }

    Ok(())
}

pub(super) fn finish_output<N: Write, D: Write>(
    flags: &Params,
    number_of_files: usize,
    xml_bytes: Vec<u8>,
    tokenizer: &TokenizerType,
    model_name: &str,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
) -> io::Result<(usize, u64, usize)> {
    if flags.stdout {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        write_stdout(
            &mut output,
            &xml_bytes,
            flags.gzip,
            flags.gzip_level,
            timings,
        )?;
        return Ok((number_of_files, 0, 0));
    }

    let xml_content = String::from_utf8(xml_bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let token_count =
        count_tokens(&xml_content, tokenizer, model_name, reporter, timings)?;
    let total_size = write_destination(flags, xml_content, reporter, timings)?;
    Ok((number_of_files, total_size, token_count))
}

fn count_tokens<N: Write, D: Write>(
    xml_content: &str,
    tokenizer: &TokenizerType,
    model_name: &str,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
) -> io::Result<usize> {
    reporter.phase_with_accent("Counting tokens with ", model_name, "")?;
    let token_start = Instant::now();
    let token_count = tokenizer
        .count_tokens(xml_content)
        .map_err(io::Error::other)?;
    timings.token_count += token_start.elapsed();
    Ok(token_count)
}

fn write_destination<N: Write, D: Write>(
    flags: &Params,
    xml_content: String,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
) -> io::Result<u64> {
    write_destination_with_clipboard(
        flags,
        xml_content,
        reporter,
        timings,
        write_clipboard,
    )
}

pub(super) fn write_destination_with_clipboard<N: Write, D: Write, C>(
    flags: &Params,
    xml_content: String,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
    clipboard_writer: C,
) -> io::Result<u64>
where
    C: FnOnce(&str, usize, &mut ProcessingTimings) -> io::Result<u64>,
{
    report_destination(flags, reporter)?;

    if flags.clipboard {
        let content_length = xml_content.len();
        return clipboard_writer(&xml_content, content_length, timings);
    }

    let output_path = effective_output_file(flags);
    write_file(flags, &output_path, xml_content.into_bytes(), timings)
}

pub(super) fn report_destination<N: Write, D: Write>(
    flags: &Params,
    reporter: &mut ProgressReporter<N, D>,
) -> io::Result<()> {
    if flags.clipboard {
        return reporter.phase("Copying result to clipboard");
    }

    let output_path = effective_output_file(flags);
    let action = if flags.gzip {
        "Compressing and writing result to '"
    } else {
        "Writing result to '"
    };
    reporter.phase_with_accent(action, &output_path.display().to_string(), "'")
}

fn write_clipboard(
    xml_content: &str,
    content_length: usize,
    timings: &mut ProcessingTimings,
) -> io::Result<u64> {
    let write_start = Instant::now();
    let mut clipboard = Clipboard::new().map_err(io::Error::other)?;
    clipboard
        .set_text(xml_content.to_owned())
        .map_err(io::Error::other)?;
    timings.output_write_or_copy += write_start.elapsed();
    Ok(content_length as u64)
}

fn write_file(
    flags: &Params,
    output_path: &Path,
    xml_bytes: Vec<u8>,
    timings: &mut ProcessingTimings,
) -> io::Result<u64> {
    let output_bytes = if flags.gzip {
        let compression_start = Instant::now();
        let compressed = compress_gzip(&xml_bytes, flags.gzip_level)?;
        timings.compression += compression_start.elapsed();
        compressed
    } else {
        xml_bytes
    };
    let write_start = Instant::now();
    let mut file = create_output_file(output_path)?;
    file.write_all(&output_bytes)?;
    timings.output_write_or_copy += write_start.elapsed();
    Ok(output_bytes.len() as u64)
}

pub(super) fn create_output_file(output_path: &Path) -> io::Result<File> {
    File::create(output_path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "failed to create output file '{}': {error}",
                output_path.display()
            ),
        )
    })
}

pub fn effective_output_file(flags: &Params) -> PathBuf {
    effective_output_file_with_home(flags, home_dir().as_deref())
}

pub(super) fn effective_output_file_with_home(
    flags: &Params,
    home_directory: Option<&Path>,
) -> PathBuf {
    let output_file = PathBuf::from(
        flags
            .output_file
            .clone()
            .unwrap_or_else(|| DEFAULT_OUTPUT_FILE.to_string()),
    );
    let output_file = home_directory
        .and_then(|home| {
            let relative_path = output_file.strip_prefix("~").ok()?;
            (!relative_path.as_os_str().is_empty())
                .then(|| home.join(relative_path))
        })
        .unwrap_or(output_file);
    let has_gzip_suffix = output_file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"));
    if flags.gzip && !has_gzip_suffix {
        let mut compressed_path = output_file.into_os_string();
        compressed_path.push(".gz");
        PathBuf::from(compressed_path)
    } else {
        output_file
    }
}

fn compress_gzip(content: &[u8], level: u32) -> io::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(content)?;
    encoder.finish()
}

pub(super) fn write_stdout<W: Write>(
    output: &mut W,
    content: &[u8],
    gzip: bool,
    level: u32,
    timings: &mut ProcessingTimings,
) -> io::Result<()> {
    if gzip {
        let compression_start = Instant::now();
        let compressed = compress_gzip(content, level)?;
        timings.compression += compression_start.elapsed();
        let write_start = Instant::now();
        output.write_all(&compressed)?;
        timings.output_write_or_copy += write_start.elapsed();
    } else {
        std::str::from_utf8(content).map_err(|error| {
            io::Error::new(io::ErrorKind::InvalidData, error)
        })?;
        let write_start = Instant::now();
        output.write_all(content)?;
        timings.output_write_or_copy += write_start.elapsed();
    }
    output.flush()
}
