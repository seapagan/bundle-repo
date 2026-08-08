use crate::filelist::{FileTree, FolderNode};
use crate::progress::ProgressReporter;
use crate::structs::{DEFAULT_OUTPUT_FILE, Params};
use crate::text_processing::{
    DecodedText, ProcessedFile, read_classify_and_decode,
};
use crate::timings::ProcessingTimings;
use crate::tokenizer::TokenizerType;
use arboard::Clipboard;
use dirs_next::home_dir;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::borrow::Cow;
use std::fs::{File, metadata};
use std::io::{self, Cursor, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use xml::common::{XmlVersion, is_xml10_char};
use xml::writer::{EmitterConfig, EventWriter, XmlEvent};

#[derive(Debug, Eq, PartialEq)]
struct InvalidXml10Char {
    byte_index: usize,
    character: char,
}

/// Function to output the repository structure and files list to XML
#[cfg(test)]
pub fn output_repo_as_xml(
    flags: &Params,
    file_tree: FileTree,
    base_path: &Path,
    tokenizer: &TokenizerType,
) -> Result<(usize, u64, usize), std::io::Error> {
    let mut reporter =
        ProgressReporter::new(io::sink(), io::sink(), flags.stdout);
    output_repo_as_xml_with_timings(
        flags,
        file_tree,
        base_path,
        tokenizer,
        "GPT-4",
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
}

pub fn output_repo_as_xml_with_timings<N: Write, D: Write>(
    flags: &Params,
    file_tree: FileTree,
    base_path: &Path,
    tokenizer: &TokenizerType,
    model_name: &str,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
) -> Result<(usize, u64, usize), std::io::Error> {
    validate_output_options(flags)?;
    let classification_before = timings.file_classification_and_read;
    let utf8_before = timings.utf8_validation_or_transcode;
    let xml_start = Instant::now();

    let xml_bytes = serialize_repository_xml(
        flags, &file_tree, base_path, reporter, timings,
    )?;
    let classification_elapsed = timings
        .file_classification_and_read
        .checked_sub(classification_before)
        .unwrap_or_default();
    let utf8_elapsed = timings
        .utf8_validation_or_transcode
        .checked_sub(utf8_before)
        .unwrap_or_default();
    timings.xml_generation += xml_start
        .elapsed()
        .checked_sub(classification_elapsed)
        .and_then(|duration| duration.checked_sub(utf8_elapsed))
        .unwrap_or_default();

    finish_output(
        flags,
        file_tree.file_paths.len(),
        xml_bytes,
        tokenizer,
        model_name,
        reporter,
        timings,
    )
}

fn serialize_repository_xml<N: Write, D: Write>(
    flags: &Params,
    file_tree: &FileTree,
    base_path: &Path,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
) -> io::Result<Vec<u8>> {
    validate_file_tree_xml_metadata(file_tree)?;

    let mut writer = EmitterConfig::new()
        .perform_indent(true)
        .write_document_declaration(false)
        .create_writer(Cursor::new(Vec::new()));
    writer
        .write(XmlEvent::StartDocument {
            version: XmlVersion::Version10,
            encoding: Some("utf-8"),
            standalone: None,
        })
        .map_err(map_xml_error)?;
    writer
        .write(XmlEvent::start_element("repository"))
        .map_err(map_xml_error)?;
    write_file_summary(&mut writer, flags)?;
    write_repository_structure(&mut writer, &file_tree.folder_node)?;
    write_repository_files_to_xml(
        &mut writer,
        &file_tree.file_paths,
        base_path,
        flags,
        reporter,
        timings,
    )?;
    writer
        .write(XmlEvent::end_element())
        .map_err(map_xml_error)?;
    write_characters(&mut writer, "\n", "document terminator")?;

    Ok(writer.into_inner().into_inner())
}

fn first_invalid_xml10_char(value: &str) -> Option<InvalidXml10Char> {
    value
        .char_indices()
        .find(|(_, character)| !is_xml10_char(*character))
        .map(|(byte_index, character)| InvalidXml10Char {
            byte_index,
            character,
        })
}

fn validate_file_tree_xml_metadata(file_tree: &FileTree) -> io::Result<()> {
    for path in &file_tree.file_paths {
        validate_xml_attribute(path, "repository file path")?;
    }
    validate_folder_xml_metadata(&file_tree.folder_node)
}

fn validate_folder_xml_metadata(folder: &FolderNode) -> io::Result<()> {
    for basename in &folder.files {
        validate_xml_attribute(basename, "repository structure file path")?;
    }
    for (name, child) in &folder.subfolders {
        validate_xml_attribute(name, "repository structure folder name")?;
        validate_folder_xml_metadata(child)?;
    }
    Ok(())
}

fn validate_xml_attribute(value: &str, role: &str) -> io::Result<()> {
    let invalid = value.char_indices().find(|(_, character)| {
        *character == '\t' || !is_xml10_char(*character)
    });
    let Some((byte_index, character)) = invalid else {
        return Ok(());
    };
    let reason = if character == '\t' {
        "cannot round-trip through XML attributes with the resolved writer"
    } else {
        "cannot be represented in XML 1.0"
    };
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "{role} \"{}\" contains {} at byte index {byte_index}, which {reason}",
            value.escape_debug(),
            format_code_point(character),
        ),
    ))
}

fn write_characters<W: Write>(
    writer: &mut EventWriter<W>,
    text: &str,
    context: &str,
) -> io::Result<()> {
    if let Some(invalid) = first_invalid_xml10_char(text) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{context} contains {} at byte index {}, which cannot be represented in XML 1.0",
                format_code_point(invalid.character),
                invalid.byte_index,
            ),
        ));
    }
    writer
        .write(XmlEvent::characters(text))
        .map_err(map_xml_error)
}

fn format_code_point(character: char) -> String {
    format!("U+{:04X}", character as u32)
}

pub fn validate_output_options(flags: &Params) -> io::Result<()> {
    validate_output_options_for(flags, io::stdout().is_terminal())
}

fn validate_output_options_for(
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

fn finish_output<N: Write, D: Write>(
    flags: &Params,
    number_of_files: usize,
    xml_bytes: Vec<u8>,
    tokenizer: &TokenizerType,
    model_name: &str,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
) -> Result<(usize, u64, usize), io::Error> {
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

    let xml_content = std::str::from_utf8(&xml_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    reporter.phase(&format!("Counting tokens with {model_name}"))?;
    let token_start = Instant::now();
    let token_count = tokenizer
        .count_tokens(xml_content)
        .map_err(io::Error::other)?;
    timings.token_count += token_start.elapsed();
    let total_size = if flags.clipboard {
        reporter.phase(&destination_phase(flags))?;
        let write_start = Instant::now();
        let mut clipboard = Clipboard::new().map_err(io::Error::other)?;
        clipboard
            .set_text(xml_content.to_owned())
            .map_err(io::Error::other)?;
        timings.output_write_or_copy += write_start.elapsed();
        xml_bytes.len() as u64
    } else {
        let output_path = effective_output_file(flags);
        reporter.phase(&destination_phase(flags))?;
        let output_bytes = if flags.gzip {
            let compression_start = Instant::now();
            let compressed = compress_gzip(&xml_bytes, flags.gzip_level)?;
            timings.compression += compression_start.elapsed();
            compressed
        } else {
            xml_bytes
        };
        let write_start = Instant::now();
        let mut file = create_output_file(&output_path)?;
        file.write_all(&output_bytes)?;
        timings.output_write_or_copy += write_start.elapsed();
        output_bytes.len() as u64
    };

    Ok((number_of_files, total_size, token_count))
}

fn create_output_file(output_path: &Path) -> io::Result<File> {
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

fn destination_phase(flags: &Params) -> String {
    if flags.clipboard {
        "Copying result to clipboard".to_string()
    } else if flags.gzip {
        format!(
            "Compressing and writing result to '{}'",
            effective_output_file(flags).display()
        )
    } else {
        format!(
            "Writing result to '{}'",
            effective_output_file(flags).display()
        )
    }
}

pub fn effective_output_file(flags: &Params) -> PathBuf {
    effective_output_file_with_home(flags, home_dir().as_deref())
}

fn effective_output_file_with_home(
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

fn write_stdout<W: Write>(
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
        std::str::from_utf8(content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let write_start = Instant::now();
        output.write_all(content)?;
        timings.output_write_or_copy += write_start.elapsed();
    }
    output.flush()
}

fn write_repository_structure<W: Write>(
    writer: &mut EventWriter<W>,
    folder_node: &FolderNode,
) -> io::Result<()> {
    writer
        .write(XmlEvent::start_element("repository_structure"))
        .map_err(map_xml_error)?;
    write_text_element(
        writer,
        "summary",
        "This node contains the hierarchical structure of the repository's files and folders.",
    )?;
    write_folder_to_xml(writer, folder_node)?;
    writer.write(XmlEvent::end_element()).map_err(map_xml_error)
}

/// Writes the folder structure using prevalidated XML attributes.
fn write_folder_to_xml<W: Write>(
    writer: &mut EventWriter<W>,
    folder_node: &FolderNode,
) -> Result<(), std::io::Error> {
    for file in &folder_node.files {
        writer
            .write(XmlEvent::start_element("file").attr("path", file))
            .map_err(map_xml_error)?;
        writer
            .write(XmlEvent::end_element())
            .map_err(map_xml_error)?;
    }

    for (subfolder_name, subfolder_node) in &folder_node.subfolders {
        writer
            .write(
                XmlEvent::start_element("folder").attr("name", subfolder_name),
            )
            .map_err(map_xml_error)?;
        write_folder_to_xml(writer, subfolder_node)?;
        writer
            .write(XmlEvent::end_element())
            .map_err(map_xml_error)?;
    }

    Ok(())
}

/// Writes repository files and their contents using XML writer events.
fn write_repository_files_to_xml<W: Write, N: Write, D: Write>(
    writer: &mut EventWriter<W>,
    file_paths: &[String],
    base_path: &Path,
    flags: &Params,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
) -> Result<(), std::io::Error> {
    writer
        .write(XmlEvent::start_element("repository_files"))
        .map_err(map_xml_error)?;
    write_text_element(
        writer,
        "summary",
        "This node contains a list of files with their full paths and contents serialized as CDATA.",
    )?;

    for file_path in file_paths {
        let full_path = base_path.join(file_path);
        let file_size = metadata(&full_path)?.len();
        match read_classify_and_decode(&full_path, flags.utf8, timings) {
            Ok(ProcessedFile::Text(decoded)) => write_processed_text_file(
                writer, file_path, file_size, decoded, flags, reporter,
            )?,
            Ok(ProcessedFile::Binary(_)) => {
                write_placeholder_file_entry(
                    writer,
                    file_path,
                    file_size,
                    "This file is a binary file and not included",
                )?;
            }
            Err(err) => {
                let error_message = err.to_string();
                reporter.error(&format!(
                    "Error reading file '{}': {}",
                    full_path.display(),
                    error_message
                ))?;
                write_read_error_file_entry(
                    writer,
                    file_path,
                    &error_message,
                )?;
            }
        }
    }

    writer.write(XmlEvent::end_element()).map_err(map_xml_error)
}

fn write_processed_text_file<W: Write, N: Write, D: Write>(
    writer: &mut EventWriter<W>,
    path: &str,
    size: u64,
    mut decoded: DecodedText,
    flags: &Params,
    reporter: &mut ProgressReporter<N, D>,
) -> io::Result<()> {
    if let Some(ref conversion) = decoded.conversion {
        reporter.conversion(path, conversion)?;
    }
    if decoded.utf8_had_replacements {
        reporter.malformed_utf8_replacement(path)?;
    }
    if let Some(invalid) = first_invalid_xml10_char(&decoded.text) {
        let code_point = format_code_point(invalid.character);
        reporter.warning(&format!(
            "warning: '{path}' content was omitted because XML 1.0 cannot represent character {code_point}"
        ))?;
        let comment = format!(
            "Text content omitted: XML 1.0 cannot represent character {}",
            code_point,
        );
        return write_placeholder_file_entry(writer, path, size, &comment);
    }
    if flags.line_numbers {
        decoded.text = add_line_numbers(&decoded.text);
    }
    write_text_file_entry(writer, path, size, &decoded.text)
}

fn write_text_file_entry<W: Write>(
    writer: &mut EventWriter<W>,
    path: &str,
    size: u64,
    content: &str,
) -> io::Result<()> {
    let size = size.to_string();
    let lines = xml_logical_text(content).lines().count().to_string();
    writer
        .write(
            XmlEvent::start_element("file")
                .attr("path", path)
                .attr("size", &size)
                .attr("lines", &lines),
        )
        .map_err(map_xml_error)?;
    writer
        .write(XmlEvent::cdata(content))
        .map_err(map_xml_error)?;
    writer.write(XmlEvent::end_element()).map_err(map_xml_error)
}

fn write_placeholder_file_entry<W: Write>(
    writer: &mut EventWriter<W>,
    path: &str,
    size: u64,
    diagnostic: &str,
) -> io::Result<()> {
    let size = size.to_string();
    write_file_entry_with_comment(writer, path, &size, diagnostic)
}

fn write_read_error_file_entry<W: Write>(
    writer: &mut EventWriter<W>,
    path: &str,
    diagnostic: &str,
) -> io::Result<()> {
    write_file_entry_with_comment(
        writer,
        path,
        "0",
        &format!("Failed to read file: {diagnostic}"),
    )
}

fn write_file_entry_with_comment<W: Write>(
    writer: &mut EventWriter<W>,
    path: &str,
    size: &str,
    diagnostic: &str,
) -> io::Result<()> {
    writer
        .write(
            XmlEvent::start_element("file")
                .attr("path", path)
                .attr("size", size)
                .attr("lines", "0"),
        )
        .map_err(map_xml_error)?;
    let comment = xml_safe_diagnostic_comment(diagnostic);
    writer
        .write(XmlEvent::comment(&comment))
        .map_err(map_xml_error)?;
    writer.write(XmlEvent::end_element()).map_err(map_xml_error)
}

fn xml_safe_diagnostic_comment(diagnostic: &str) -> String {
    diagnostic
        .chars()
        .map(|character| {
            if is_xml10_char(character) {
                character.to_string()
            } else {
                format!("[unrepresentable {}]", format_code_point(character))
            }
        })
        .collect()
}

/// Map XML writing errors to IO errors
fn map_xml_error(err: xml::writer::Error) -> std::io::Error {
    std::io::Error::other(err)
}

fn write_file_summary<W: Write>(
    writer: &mut EventWriter<W>,
    flags: &Params,
) -> io::Result<()> {
    writer
        .write(XmlEvent::start_element("file_summary"))
        .map_err(map_xml_error)?;
    write_text_element(
        writer,
        "purpose",
        "This file contains a packed representation of the entire repository's contents.\nIt is designed to be easily consumable by AI systems for analysis, code review,\nor other automated processes.",
    )?;
    write_text_element(
        writer,
        "file_format",
        "The content is organized as follows:\n1. This summary section\n2. Repository structure: A hierarchical listing of all folders and files in the repository.\n3. Repository files: Each file is listed with:\n  - File path as an attribute\n  - Full contents of the file, excluding binary files and text that XML 1.0 cannot represent.",
    )?;

    let line_number_instruction = if flags.line_numbers {
        "\n- Line numbers have been added to the code for reference. Please use them for\n  referring to specific lines of code when needed. However, do NOT include line\n  numbers when outputting or displaying code in responses."
    } else {
        ""
    };
    let instructions = format!(
        "- The LLM is instructed to focus solely on the repository's contents, including\n  the code, file structure, and purpose of the files.\n- Do not comment on the XML format, structure, or encoding of THIS FILE. Focus\n  your analysis on the functionality, structure, and organization of the\n  repository contents.{line_number_instruction}\n- Each <file> should be interpreted based on its file extension. For example:\n  - \".py\" for Python\n  - \".md\" for Markdown\n  - \".rs\" for Rust\n  - \".cpp\" for C++"
    );
    write_text_element(writer, "instructions", &instructions)?;
    write_text_element(
        writer,
        "usage_guidelines",
        "- This file should be treated as read-only. Any changes should be made to the\n  original repository files, not this packed version.\n- When processing this file, use the file path to distinguish\n  between different files in the repository.\n- Be aware that this file may contain sensitive information. Handle it with\n  the same level of security as you would the original repository.",
    )?;
    write_text_element(
        writer,
        "notes",
        "- Some files may have been excluded based on .gitignore rules and bundlerepo's\n  configuration.\n- Binary files and text that XML 1.0 cannot represent are not included in this\n  packed representation. Please refer to the Repository Structure section for\n  a complete list of file paths, including omitted files.",
    )?;
    write_text_element(
        writer,
        "additional_info",
        "For more information about bundlerepo, visit: https://github.com/seapagan/bundle-repo",
    )?;
    writer.write(XmlEvent::end_element()).map_err(map_xml_error)
}

fn write_text_element<W: Write>(
    writer: &mut EventWriter<W>,
    name: &str,
    text: &str,
) -> io::Result<()> {
    writer
        .write(XmlEvent::start_element(name))
        .map_err(map_xml_error)?;
    write_characters(writer, text, name)?;
    writer.write(XmlEvent::end_element()).map_err(map_xml_error)
}

/// Adds line numbers to the given file content, ensuring the content ends
/// with a newline. The line numbers are dynamically padded to fit the largest
/// line number.
///
/// Args:
///     file_content: A string containing the raw content of the file.
///
/// Returns:
///     A string with line numbers added to each line, left-padded, and
///     followed by 4 spaces. Non-empty content ends with a newline.
fn add_line_numbers(file_content: &str) -> String {
    if file_content.is_empty() {
        return String::new();
    }

    let normalized = xml_logical_text(file_content);
    let lines: Vec<&str> = normalized.lines().collect();
    let total_lines = lines.len();

    // Determine the width needed for the largest line number
    let width = total_lines.to_string().len();

    // Add line numbers with dynamic width padding
    let mut numbered_content = lines
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>width$}  {}", i + 1, line, width = width))
        .collect::<Vec<_>>()
        .join("\n");

    // Ensure the content ends with a newline
    if !numbered_content.ends_with('\n') {
        numbered_content.push('\n');
    }

    numbered_content
}

fn xml_logical_text(content: &str) -> Cow<'_, str> {
    if content.contains('\r') {
        Cow::Owned(content.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(content)
    }
}

#[cfg(test)]
#[path = "../tests/crate/xml_output.rs"]
mod tests;
