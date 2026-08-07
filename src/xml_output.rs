use crate::filelist::{FileTree, FolderNode};
use crate::progress::ProgressReporter;
use crate::structs::{DEFAULT_OUTPUT_FILE, Params};
use crate::text_processing::{ProcessedFile, read_classify_and_decode};
use crate::timings::ProcessingTimings;
use crate::tokenizer::TokenizerType;
use arboard::Clipboard;
use dirs_next::home_dir;
use flate2::Compression;
use flate2::write::GzEncoder;
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
            "{role} {:?} contains {} at byte index {byte_index}, which {reason}",
            value.escape_debug().to_string(),
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
        .count_tokens(&xml_content)
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
        let mut file = File::create(output_path)?;
        file.write_all(&output_bytes)?;
        timings.output_write_or_copy += write_start.elapsed();
        output_bytes.len() as u64
    };

    Ok((number_of_files, total_size, token_count))
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
            Ok(ProcessedFile::Text(mut decoded)) => {
                if let Some(ref conversion) = decoded.conversion {
                    reporter.conversion(file_path, conversion)?;
                }
                if decoded.utf8_had_replacements {
                    reporter.malformed_utf8_replacement(file_path)?;
                }
                if let Some(invalid) = first_invalid_xml10_char(&decoded.text)
                {
                    let comment = format!(
                        "Text content omitted: XML 1.0 cannot represent character {}",
                        format_code_point(invalid.character),
                    );
                    write_placeholder_file_entry(
                        writer, file_path, file_size, &comment,
                    )?;
                } else {
                    if flags.line_numbers {
                        decoded.text = add_line_numbers(&decoded.text);
                    }
                    write_text_file_entry(
                        writer,
                        file_path,
                        file_size,
                        &decoded.text,
                    )?;
                }
            }
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

fn write_text_file_entry<W: Write>(
    writer: &mut EventWriter<W>,
    path: &str,
    size: u64,
    content: &str,
) -> io::Result<()> {
    let size = size.to_string();
    let lines = content.lines().count().to_string();
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
///     followed by 4 spaces. Ensures the final content ends with a newline.
fn add_line_numbers(file_content: &str) -> String {
    let lines: Vec<&str> = file_content.lines().collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filelist::FileTree;
    use crate::test_fixtures::{
        ENCODING_FIXTURES, UTF16BE_BYTES, UTF16LE_BYTES, WINDOWS_1252_BYTES,
    };
    use crate::tokenizer::Model;
    use flate2::read::GzDecoder;
    use std::fs;
    use std::io::Read;
    use std::time::Duration;
    use tempfile::tempdir;
    use xml::reader::{ParserConfig, XmlEvent as ReaderXmlEvent};

    #[derive(Debug)]
    struct ParsedFile {
        attributes: Vec<(String, String)>,
        text: String,
        comments: Vec<String>,
    }

    fn parse_document(xml: &[u8]) -> Vec<ReaderXmlEvent> {
        let mut events = Vec::new();
        let mut reached_end = false;
        for event in ParserConfig::new()
            .ignore_comments(false)
            .create_reader(xml)
        {
            let event = event.unwrap();
            reached_end |= matches!(event, ReaderXmlEvent::EndDocument);
            events.push(event);
        }
        assert!(reached_end, "parser did not reach EndDocument");
        events
    }

    fn parse_file(xml: &[u8], expected_path: &str) -> ParsedFile {
        let events = parse_document(xml);
        let mut parsed = None;
        let mut in_file = false;
        for event in events {
            match event {
                ReaderXmlEvent::StartElement {
                    name, attributes, ..
                } if name.local_name == "file"
                    && attributes.iter().any(|attribute| {
                        attribute.name.local_name == "path"
                            && attribute.value == expected_path
                    }) =>
                {
                    in_file = true;
                    parsed = Some(ParsedFile {
                        attributes: attributes
                            .into_iter()
                            .map(|attribute| {
                                (attribute.name.local_name, attribute.value)
                            })
                            .collect(),
                        text: String::new(),
                        comments: Vec::new(),
                    });
                }
                ReaderXmlEvent::Characters(text)
                | ReaderXmlEvent::CData(text)
                    if in_file =>
                {
                    parsed.as_mut().unwrap().text.push_str(&text);
                }
                ReaderXmlEvent::Comment(comment) if in_file => {
                    parsed.as_mut().unwrap().comments.push(comment);
                }
                ReaderXmlEvent::EndElement { name }
                    if in_file && name.local_name == "file" =>
                {
                    break;
                }
                _ => {}
            }
        }
        parsed.unwrap_or_else(|| {
            panic!("missing file element for {expected_path:?}")
        })
    }

    fn attribute<'a>(file: &'a ParsedFile, name: &str) -> &'a str {
        file.attributes
            .iter()
            .find(|(attribute, _)| attribute == name)
            .map(|(_, value)| value.as_str())
            .unwrap()
    }

    fn serialize_single_file(content: &[u8], line_numbers: bool) -> Vec<u8> {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("test.txt"), content).unwrap();
        let mut tree = FileTree::default();
        tree.file_paths.push("test.txt".to_string());
        let flags = Params {
            line_numbers,
            ..Params::default()
        };
        let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
        serialize_repository_xml(
            &flags,
            &tree,
            temp_dir.path(),
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap()
    }

    fn serialize_text_entry(path: &str, content: &str) -> Vec<u8> {
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
            .unwrap();
        writer.write(XmlEvent::start_element("repository")).unwrap();
        write_text_file_entry(
            &mut writer,
            path,
            content.len() as u64,
            content,
        )
        .unwrap();
        writer.write(XmlEvent::end_element()).unwrap();
        writer.into_inner().into_inner()
    }

    fn serialize_read_error_entry(path: &str, diagnostic: &str) -> Vec<u8> {
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
            .unwrap();
        writer.write(XmlEvent::start_element("repository")).unwrap();
        write_read_error_file_entry(&mut writer, path, diagnostic).unwrap();
        writer.write(XmlEvent::end_element()).unwrap();
        writer.into_inner().into_inner()
    }

    #[test]
    fn test_xml10_character_boundaries() {
        for character in [
            '\0', '\u{0001}', '\u{0008}', '\u{000b}', '\u{000c}', '\u{000e}',
            '\u{001f}', '\u{fffe}', '\u{ffff}',
        ] {
            let value = format!("before{character}after");
            let invalid = first_invalid_xml10_char(&value).unwrap();
            assert_eq!(invalid.character, character);
            assert_eq!(invalid.byte_index, "before".len());
        }

        for character in [
            '\t',
            '\n',
            '\r',
            '\u{0020}',
            '\u{d7ff}',
            '\u{e000}',
            '\u{fffd}',
            '\u{10000}',
            '\u{10ffff}',
        ] {
            assert_eq!(first_invalid_xml10_char(&character.to_string()), None);
        }
    }

    #[test]
    fn test_metadata_validation_rejects_each_entry_point_before_file_access() {
        let cases = [
            ("repository file path", 0),
            ("repository structure file path", 1),
            ("repository structure folder name", 2),
            ("repository structure folder name", 3),
        ];
        for (expected_role, location) in cases {
            let mut tree = FileTree::default();
            match location {
                0 => tree.file_paths.push("missing\u{000b}file".to_string()),
                1 => tree
                    .folder_node
                    .files
                    .push("bad\u{000b}basename".to_string()),
                2 => {
                    tree.folder_node.subfolders.insert(
                        "bad\u{000b}folder".to_string(),
                        FolderNode::default(),
                    );
                }
                3 => {
                    let mut parent = FolderNode::default();
                    parent.subfolders.insert(
                        "nested\u{000b}folder".to_string(),
                        FolderNode::default(),
                    );
                    tree.folder_node
                        .subfolders
                        .insert("parent".to_string(), parent);
                }
                _ => unreachable!(),
            }

            let error = validate_file_tree_xml_metadata(&tree).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            let message = error.to_string();
            assert!(message.contains(expected_role));
            assert!(message.contains("\\u{b}"));
            assert!(message.contains("U+000B"));
            assert!(message.contains("byte index"));
        }
    }

    #[test]
    fn test_tab_metadata_rejection_records_writer_normalization_contract() {
        let mut output = Vec::new();
        {
            let mut writer = EmitterConfig::new()
                .perform_indent(false)
                .create_writer(&mut output);
            writer
                .write(XmlEvent::start_element("root").attr("path", "a\tb"))
                .unwrap();
            writer.write(XmlEvent::end_element()).unwrap();
        }
        let parsed = parse_document(&output);
        let value = parsed
            .iter()
            .find_map(|event| match event {
                ReaderXmlEvent::StartElement { attributes, .. } => attributes
                    .iter()
                    .find(|attribute| attribute.name.local_name == "path")
                    .map(|attribute| attribute.value.as_str()),
                _ => None,
            })
            .unwrap();
        assert_eq!(value, "a b");

        let mut tree = FileTree::default();
        tree.file_paths.push("a\tb".to_string());
        let error = validate_file_tree_xml_metadata(&tree).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("round-trip"));
        assert!(error.to_string().contains("U+0009"));
    }

    #[test]
    fn test_complete_document_has_one_root_and_literal_file_instruction() {
        let xml = serialize_single_file(b"ordinary text", false);
        let events = parse_document(&xml);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    ReaderXmlEvent::StartDocument { .. }
                ))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ReaderXmlEvent::EndDocument))
                .count(),
            1
        );
        let element_names = events
            .iter()
            .filter_map(|event| match event {
                ReaderXmlEvent::StartElement { name, .. } => {
                    Some(name.local_name.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for expected in [
            "repository",
            "file_summary",
            "purpose",
            "file_format",
            "instructions",
            "usage_guidelines",
            "notes",
            "additional_info",
            "repository_structure",
            "repository_files",
        ] {
            assert!(element_names.contains(&expected));
        }
        let prose = events
            .iter()
            .filter_map(|event| match event {
                ReaderXmlEvent::Characters(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert!(prose.contains("Each <file> should be interpreted"));
        assert!(!element_names.contains(&".py"));
    }

    #[test]
    fn test_cdata_content_matrix_round_trips_through_complete_documents() {
        let cases = [
            "ordinary text",
            "<tag attr='single' other=\"double\"> & text > tail",
            "literal </file><injected>markup</injected>",
            "]]>",
            "before]]>middle]]>after",
            "]]>at start and at end]]>",
            "日本語 العربية Кириллица café 😀 \u{fffd} \u{10000}",
            "",
            "first\r\nsecond\rthird\nfourth",
        ];

        for content in cases {
            let xml = serialize_single_file(content.as_bytes(), false);
            let file = parse_file(&xml, "test.txt");
            let expected = content.replace("\r\n", "\n").replace('\r', "\n");
            assert_eq!(file.text, expected, "failed content {content:?}");
            assert!(String::from_utf8(xml).unwrap().contains("<![CDATA["));
        }
    }

    #[test]
    fn test_embedded_cdata_end_tokens_use_adjacent_sections() {
        let content = "a]]>b]]>c";
        let xml = serialize_single_file(content.as_bytes(), false);
        let serialized = String::from_utf8(xml.clone()).unwrap();
        assert!(serialized.matches("<![CDATA[").count() >= 3);
        assert_eq!(parse_file(&xml, "test.txt").text, content);
    }

    #[test]
    fn test_line_numbered_content_round_trips_from_cdata() {
        let xml = serialize_single_file(b"Line 1\nLine 2\nLine 3", true);
        let file = parse_file(&xml, "test.txt");
        assert_eq!(file.text, "1  Line 1\n2  Line 2\n3  Line 3\n");
        assert_eq!(attribute(&file, "lines"), "3");
    }

    #[test]
    fn test_xml_forbidden_text_is_omitted_without_reclassification() {
        for character in ['\u{000b}', '\u{001f}', '\u{fffe}', '\u{ffff}'] {
            let content = format!(
                "a sufficiently long text prefix {character} and suffix"
            );
            let temp_dir = tempdir().unwrap();
            let path = temp_dir.path().join("test.txt");
            fs::write(&path, content.as_bytes()).unwrap();
            assert!(matches!(
                read_classify_and_decode(
                    &path,
                    false,
                    &mut ProcessingTimings::default()
                )
                .unwrap(),
                ProcessedFile::Text(_)
            ));

            let xml = serialize_single_file(content.as_bytes(), false);
            let serialized = String::from_utf8(xml.clone()).unwrap();
            let file = parse_file(&xml, "test.txt");
            assert_eq!(attribute(&file, "size"), content.len().to_string());
            assert_eq!(attribute(&file, "lines"), "0");
            assert!(file.text.is_empty());
            assert!(!serialized.contains("<![CDATA["));
            assert_eq!(file.comments.len(), 1);
            assert!(file.comments[0].contains("content omitted"));
            assert!(file.comments[0].contains(&format_code_point(character)));
        }
    }

    #[test]
    fn test_sparse_del_remains_text_and_round_trips_as_xml10() {
        let content = "before\u{007f}after";
        assert!(is_xml10_char('\u{007f}'));
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("test.txt");
        fs::write(&path, content).unwrap();
        assert!(matches!(
            read_classify_and_decode(
                &path,
                false,
                &mut ProcessingTimings::default()
            )
            .unwrap(),
            ProcessedFile::Text(_)
        ));
        let xml = serialize_single_file(content.as_bytes(), false);
        assert_eq!(parse_file(&xml, "test.txt").text, content);
    }

    #[test]
    fn test_xml_sensitive_metadata_round_trips_in_structure_and_file_entries()
    {
        let file_name = "file<&\"'.txt";
        let folder_name = "folder<&\"'";
        let mut tree = FileTree::default();
        tree.folder_node.files.push(file_name.to_string());
        tree.folder_node
            .subfolders
            .insert(folder_name.to_string(), FolderNode::default());
        let temp_dir = tempdir().unwrap();
        let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
        let xml = serialize_repository_xml(
            &Params::default(),
            &tree,
            temp_dir.path(),
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();
        let events = parse_document(&xml);
        assert!(events.iter().any(|event| matches!(
            event,
            ReaderXmlEvent::StartElement { name, attributes, .. }
                if name.local_name == "file"
                    && attributes.iter().any(|attribute|
                        attribute.name.local_name == "path"
                            && attribute.value == file_name)
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ReaderXmlEvent::StartElement { name, attributes, .. }
                if name.local_name == "folder"
                    && attributes.iter().any(|attribute|
                        attribute.name.local_name == "name"
                            && attribute.value == folder_name)
        )));

        let entry_xml = serialize_text_entry(file_name, "content");
        assert_eq!(parse_file(&entry_xml, file_name).text, "content");
    }

    #[test]
    fn test_lf_and_cr_metadata_round_trip_exactly() {
        let path = "line\nfeed.txt";
        let entry_xml = serialize_text_entry(path, "content");
        assert!(String::from_utf8_lossy(&entry_xml).contains("&#xA;"));
        validate_xml_attribute(path, "test path").unwrap();
        assert_eq!(attribute(&parse_file(&entry_xml, path), "path"), path);

        let folder_name = "carriage\rreturn";
        let mut tree = FileTree::default();
        tree.folder_node
            .subfolders
            .insert(folder_name.to_string(), FolderNode::default());
        validate_file_tree_xml_metadata(&tree).unwrap();
        let temp_dir = tempdir().unwrap();
        let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
        let xml = serialize_repository_xml(
            &Params::default(),
            &tree,
            temp_dir.path(),
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&xml).contains("&#xD;"));
        assert!(parse_document(&xml).iter().any(|event| matches!(
            event,
            ReaderXmlEvent::StartElement { name, attributes, .. }
                if name.local_name == "folder"
                    && attributes.iter().any(|attribute|
                        attribute.name.local_name == "name"
                            && attribute.value == folder_name)
        )));
    }

    #[test]
    fn test_read_error_comment_is_xml_safe_and_diagnostic() {
        let diagnostic = "bad -- <tag> & \"quote\" trailing-\u{000b}";
        let xml = serialize_read_error_entry("test.txt", diagnostic);
        let file = parse_file(&xml, "test.txt");
        assert_eq!(attribute(&file, "size"), "0");
        assert_eq!(attribute(&file, "lines"), "0");
        assert!(file.text.is_empty());
        assert_eq!(file.comments.len(), 1);
        let comment = &file.comments[0];
        for expected in ["bad", "<tag>", "&", "\"quote\"", "trailing-"] {
            assert!(
                comment.contains(expected),
                "missing {expected:?}: {comment:?}"
            );
        }
        assert!(comment.contains("[unrepresentable U+000B]"));
    }

    #[test]
    fn test_invalid_metadata_creates_no_destination_file() {
        let temp_dir = tempdir().unwrap();
        let output = temp_dir.path().join("must-not-exist.xml");
        let params = Params {
            output_file: Some(output.to_string_lossy().into_owned()),
            ..Params::default()
        };
        let mut tree = FileTree::default();
        tree.file_paths.push("missing\u{000b}file".to_string());
        let error = output_repo_as_xml(
            &params,
            tree,
            temp_dir.path(),
            &Model::GPT4.to_tokenizer().unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!output.exists());
    }

    #[test]
    fn test_add_line_numbers() {
        let content = "First line\nSecond line\nThird line";
        let numbered = add_line_numbers(content);
        assert!(numbered.contains("1  First line"));
        assert!(numbered.contains("2  Second line"));
        assert!(numbered.contains("3  Third line"));
        assert!(numbered.ends_with('\n'));
    }

    #[test]
    fn test_read_classify_and_decode_distinguishes_text_and_binary() {
        let temp_dir = tempdir().unwrap();
        let mut timings = ProcessingTimings::default();

        // Create a text file
        let text_path = temp_dir.path().join("test.txt");
        fs::write(&text_path, "Hello, World!").unwrap();
        assert!(matches!(
            read_classify_and_decode(&text_path, false, &mut timings).unwrap(),
            ProcessedFile::Text(_)
        ));

        // Create a binary file
        let binary_path = temp_dir.path().join("test.bin");
        fs::write(&binary_path, [0u8, 159u8, 146u8, 150u8]).unwrap();
        assert!(matches!(
            read_classify_and_decode(&binary_path, false, &mut timings)
                .unwrap(),
            ProcessedFile::Binary(_)
        ));
    }

    #[test]
    fn test_output_repo_as_xml() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");

        // Create the test file
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "Test content").unwrap();

        let params = Params {
            output_file: Some(output_file.to_str().unwrap().to_string()),
            ..Params::default()
        };

        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("test.txt".to_string());

        let tokenizer = Model::GPT4.to_tokenizer().unwrap();

        let result = output_repo_as_xml(
            &params,
            file_tree,
            temp_dir.path(),
            &tokenizer,
        );
        assert!(result.is_ok());

        let xml_content = fs::read_to_string(output_file).unwrap();
        assert!(
            xml_content.contains("<?xml version=\"1.0\" encoding=\"utf-8\"?>")
        );
        assert!(xml_content.contains("<repository>"));
        assert!(xml_content.contains("<repository_structure>"));
        assert!(xml_content.contains("<repository_files>"));
        assert!(xml_content.contains("<file path=\"test.txt\""));
        assert!(xml_content.contains("Test content"));
    }

    #[test]
    fn test_reused_timings_subtract_only_per_call_file_phase_deltas() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        let params = Params {
            output_file: Some(output_file.to_string_lossy().into_owned()),
            ..Params::default()
        };
        let tokenizer = Model::GPT4.to_tokenizer().unwrap();
        let mut reporter =
            ProgressReporter::new(Vec::new(), Vec::new(), false);
        let mut timings = ProcessingTimings {
            file_classification_and_read: Duration::from_secs(1),
            utf8_validation_or_transcode: Duration::from_secs(1),
            ..ProcessingTimings::default()
        };

        output_repo_as_xml_with_timings(
            &params,
            FileTree::default(),
            temp_dir.path(),
            &tokenizer,
            "GPT-4",
            &mut reporter,
            &mut timings,
        )
        .unwrap();
        let after_first_call = timings.xml_generation;

        output_repo_as_xml_with_timings(
            &params,
            FileTree::default(),
            temp_dir.path(),
            &tokenizer,
            "GPT-4",
            &mut reporter,
            &mut timings,
        )
        .unwrap();

        assert!(after_first_call > Duration::ZERO);
        assert!(timings.xml_generation > after_first_call);
    }

    #[test]
    fn test_output_repo_with_line_numbers() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");

        // Create the test file with multiple lines
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "Line 1\nLine 2\nLine 3").unwrap();

        let params = Params {
            output_file: Some(output_file.to_str().unwrap().to_string()),
            line_numbers: true,
            ..Params::default()
        };

        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("test.txt".to_string());

        let tokenizer = Model::GPT4.to_tokenizer().unwrap();

        let result = output_repo_as_xml(
            &params,
            file_tree,
            temp_dir.path(),
            &tokenizer,
        );
        assert!(result.is_ok());

        let xml_content = fs::read_to_string(output_file).unwrap();
        assert!(xml_content.contains("1  Line 1"));
        assert!(xml_content.contains("2  Line 2"));
        assert!(xml_content.contains("3  Line 3"));
    }

    #[test]
    fn test_binary_file_handling() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");

        // Create a binary file
        let test_file = temp_dir.path().join("test.bin");
        fs::write(&test_file, [0u8, 159u8, 146u8, 150u8]).unwrap();

        let params = Params {
            output_file: Some(output_file.to_str().unwrap().to_string()),
            ..Params::default()
        };

        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("test.bin".to_string());

        let tokenizer = Model::GPT4.to_tokenizer().unwrap();

        let result = output_repo_as_xml(
            &params,
            file_tree,
            temp_dir.path(),
            &tokenizer,
        );
        assert!(result.is_ok());

        let xml_content = fs::read(output_file).unwrap();
        let file = parse_file(&xml_content, "test.bin");
        assert_eq!(attribute(&file, "size"), "4");
        assert_eq!(attribute(&file, "lines"), "0");
        assert!(file.text.is_empty());
        assert!(file.comments[0].contains("binary file and not included"));
    }

    #[test]
    fn test_stdout_output() {
        let temp_dir = tempdir().unwrap();

        // Create the test file
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "Test content").unwrap();

        let params = Params {
            stdout: true,
            ..Params::default()
        };

        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("test.txt".to_string());

        let tokenizer = Model::GPT4.to_tokenizer().unwrap();

        let result = output_repo_as_xml(
            &params,
            file_tree,
            temp_dir.path(),
            &tokenizer,
        );
        assert!(result.is_ok());
        let (num_files, size, _) = result.unwrap();
        assert_eq!(num_files, 1);
        assert_eq!(size, 0); // Size is 0 for stdout output
    }

    #[test]
    fn test_utf8_encoding() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");

        // Create a test file with non-UTF8 content
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, b"Hello \xFF World").unwrap(); // Invalid UTF-8 sequence

        let params = Params {
            output_file: Some(output_file.to_str().unwrap().to_string()),
            utf8: true,
            ..Params::default()
        };

        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("test.txt".to_string());

        let tokenizer = Model::GPT4.to_tokenizer().unwrap();

        let result = output_repo_as_xml(
            &params,
            file_tree,
            temp_dir.path(),
            &tokenizer,
        );
        assert!(result.is_ok());

        let xml_content = fs::read_to_string(output_file).unwrap();
        assert!(xml_content.contains("<file path=\"test.txt\""));
        // The content should be readable as UTF-8
        assert!(String::from_utf8(xml_content.as_bytes().to_vec()).is_ok());
    }

    #[test]
    fn test_encoding_fixture_matrix_is_included_as_valid_utf8_xml() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        let mut file_tree = FileTree::default();
        for fixture in ENCODING_FIXTURES {
            fs::write(temp_dir.path().join(fixture.name), fixture.bytes)
                .unwrap();
            file_tree.file_paths.push(fixture.name.to_string());
        }
        let params = Params {
            output_file: Some(output_file.to_string_lossy().into_owned()),
            utf8: true,
            ..Params::default()
        };

        output_repo_as_xml(
            &params,
            file_tree,
            temp_dir.path(),
            &Model::GPT4.to_tokenizer().unwrap(),
        )
        .unwrap();

        let xml = fs::read_to_string(output_file).unwrap();
        for fixture in ENCODING_FIXTURES {
            let start = format!("<file path=\"{}\"", fixture.name);
            let file_xml = xml.split(&start).nth(1).unwrap();
            assert!(
                file_xml
                    .split("</file>")
                    .next()
                    .unwrap()
                    .contains(fixture.expected)
            );
        }
    }

    #[test]
    fn test_utf16_fixtures_are_excluded_when_conversion_is_disabled() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        let mut file_tree = FileTree::default();
        for (name, bytes) in [
            ("utf-16le.txt", UTF16LE_BYTES),
            ("utf-16be.txt", UTF16BE_BYTES),
        ] {
            fs::write(temp_dir.path().join(name), bytes).unwrap();
            file_tree.file_paths.push(name.to_string());
        }
        let params = Params {
            output_file: Some(output_file.to_string_lossy().into_owned()),
            utf8: false,
            ..Params::default()
        };

        output_repo_as_xml(
            &params,
            file_tree,
            temp_dir.path(),
            &Model::GPT4.to_tokenizer().unwrap(),
        )
        .unwrap();

        let xml = fs::read_to_string(output_file).unwrap();
        assert_eq!(xml.matches("binary file and not included").count(), 2);
        assert!(!xml.contains("UTF-16 text with"));
    }

    #[test]
    fn test_progress_phases_and_conversion_follow_execution_order() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        fs::write(temp_dir.path().join("legacy.txt"), WINDOWS_1252_BYTES)
            .unwrap();
        let params = Params {
            output_file: Some(output_file.to_string_lossy().into_owned()),
            utf8: true,
            ..Params::default()
        };
        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("legacy.txt".to_string());
        let tokenizer = Model::GPT4.to_tokenizer().unwrap();
        let mut reporter =
            ProgressReporter::new(Vec::new(), Vec::new(), false);
        let mut timings = ProcessingTimings::default();

        reporter.phase("Loading tokenizer for GPT-4").unwrap();
        reporter.phase("Reading files and generating XML").unwrap();
        output_repo_as_xml_with_timings(
            &params,
            file_tree,
            temp_dir.path(),
            &tokenizer,
            "GPT-4",
            &mut reporter,
            &mut timings,
        )
        .unwrap();
        reporter.normal_line("-> Successfully wrote XML").unwrap();

        let (normal, diagnostic) = reporter.into_parts();
        assert_eq!(
            String::from_utf8(normal).unwrap(),
            format!(
                "-> Loading tokenizer for GPT-4\n\
                 -> Reading files and generating XML\n\
                 -> Converted 'legacy.txt' from windows-1252 to UTF-8\n\
                 -> Counting tokens with GPT-4\n\
                 -> Writing result to '{}'\n\
                 -> Successfully wrote XML\n",
                output_file.display()
            )
        );
        assert!(diagnostic.is_empty());
    }

    #[test]
    fn test_utf16_replacement_warning_is_emitted_once() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        let mut malformed = UTF16LE_BYTES.to_vec();
        malformed.pop();
        fs::write(temp_dir.path().join("malformed.txt"), malformed).unwrap();
        let params = Params {
            output_file: Some(output_file.to_string_lossy().into_owned()),
            utf8: true,
            ..Params::default()
        };
        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("malformed.txt".to_string());
        let tokenizer = Model::GPT4.to_tokenizer().unwrap();
        let mut reporter =
            ProgressReporter::new(Vec::new(), Vec::new(), false);

        output_repo_as_xml_with_timings(
            &params,
            file_tree,
            temp_dir.path(),
            &tokenizer,
            "GPT-4",
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();

        let (normal, diagnostic) = reporter.into_parts();
        let normal = String::from_utf8(normal).unwrap();
        assert_eq!(normal.matches("-> Converted 'malformed.txt'").count(), 1);
        assert_eq!(
            String::from_utf8(diagnostic).unwrap(),
            "warning: 'malformed.txt' decoded as UTF-16LE with replacement characters; information was lost\n"
        );
    }

    #[test]
    fn test_malformed_utf8_bom_emits_one_replacement_warning() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        fs::write(
            temp_dir.path().join("malformed.txt"),
            b"\xef\xbb\xbfmalformed \xff text",
        )
        .unwrap();
        let params = Params {
            output_file: Some(output_file.to_string_lossy().into_owned()),
            utf8: true,
            ..Params::default()
        };
        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("malformed.txt".to_string());
        let mut reporter =
            ProgressReporter::new(Vec::new(), Vec::new(), false);

        output_repo_as_xml_with_timings(
            &params,
            file_tree,
            temp_dir.path(),
            &Model::GPT4.to_tokenizer().unwrap(),
            "GPT-4",
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();

        let (normal, diagnostic) = reporter.into_parts();
        assert!(!String::from_utf8(normal).unwrap().contains("Converted"));
        assert_eq!(
            String::from_utf8(diagnostic).unwrap(),
            "warning: 'malformed.txt' contained malformed UTF-8 and was decoded with replacement characters; information was lost\n"
        );
        let xml = fs::read_to_string(output_file).unwrap();
        assert!(xml.contains("\u{feff}malformed \u{fffd} text"));
    }

    #[test]
    fn test_clean_utf8_bom_emits_no_conversion_or_replacement_message() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        fs::write(temp_dir.path().join("clean.txt"), b"\xef\xbb\xbfclean")
            .unwrap();
        let params = Params {
            output_file: Some(output_file.to_string_lossy().into_owned()),
            utf8: true,
            ..Params::default()
        };
        let mut file_tree = FileTree::default();
        file_tree.file_paths.push("clean.txt".to_string());
        let mut reporter =
            ProgressReporter::new(Vec::new(), Vec::new(), false);

        output_repo_as_xml_with_timings(
            &params,
            file_tree,
            temp_dir.path(),
            &Model::GPT4.to_tokenizer().unwrap(),
            "GPT-4",
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();

        let (normal, diagnostic) = reporter.into_parts();
        assert!(!String::from_utf8(normal).unwrap().contains("Converted"));
        assert!(diagnostic.is_empty());
    }

    #[test]
    fn test_malformed_utf8_bom_is_silent_with_quiet_reporter() {
        let temp_dir = tempdir().unwrap();
        fs::write(
            temp_dir.path().join("malformed.txt"),
            b"\xef\xbb\xbfmalformed \xff text",
        )
        .unwrap();
        let paths = vec!["malformed.txt".to_string()];
        let params = Params {
            stdout: true,
            utf8: true,
            ..Params::default()
        };
        let mut writer = EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(false)
            .create_writer(Vec::new());
        let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);

        write_repository_files_to_xml(
            &mut writer,
            &paths,
            temp_dir.path(),
            &params,
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();

        let xml = writer.into_inner();
        assert!(
            String::from_utf8(xml)
                .unwrap()
                .contains("\u{feff}malformed \u{fffd} text")
        );
        let (normal, diagnostic) = reporter.into_parts();
        assert!(normal.is_empty());
        assert!(diagnostic.is_empty());
    }

    #[test]
    fn test_quiet_reporter_keeps_plain_and_gzip_stdout_bytes_clean() {
        let xml = b"<repository>legacy text</repository>\n";
        for gzip in [false, true] {
            let mut output = Vec::new();
            let mut timings = ProcessingTimings::default();
            let mut reporter =
                ProgressReporter::new(Vec::new(), Vec::new(), true);
            reporter.phase("Hidden phase").unwrap();
            reporter
                .conversion(
                    "legacy.txt",
                    &crate::text_processing::ConversionReport {
                        source_encoding: "windows-1252",
                        had_replacements: true,
                    },
                )
                .unwrap();
            reporter
                .malformed_utf8_replacement("malformed.txt")
                .unwrap();

            write_stdout(&mut output, xml, gzip, 6, &mut timings).unwrap();
            let (normal, diagnostic) = reporter.into_parts();
            assert!(normal.is_empty());
            assert!(diagnostic.is_empty());
            if gzip {
                assert_eq!(&output[..2], &[0x1f, 0x8b]);
                let mut decoded = Vec::new();
                GzDecoder::new(output.as_slice())
                    .read_to_end(&mut decoded)
                    .unwrap();
                assert_eq!(decoded, xml);
            } else {
                assert_eq!(output, xml);
            }
        }
    }

    #[test]
    fn test_destination_phase_messages_cover_all_destinations() {
        let file = Params {
            output_file: Some("result.xml".to_string()),
            ..Params::default()
        };
        assert_eq!(destination_phase(&file), "Writing result to 'result.xml'");

        let gzip = Params { gzip: true, ..file };
        assert_eq!(
            destination_phase(&gzip),
            "Compressing and writing result to 'result.xml.gz'"
        );

        let clipboard = Params {
            clipboard: true,
            gzip: false,
            ..Params::default()
        };
        assert_eq!(
            destination_phase(&clipboard),
            "Copying result to clipboard"
        );
    }

    #[test]
    fn test_gzip_file_round_trip_and_metrics() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("test.txt"), "Test content").unwrap();
        let tokenizer = Model::GPT4.to_tokenizer().unwrap();

        let file_tree = || {
            let mut tree = FileTree::default();
            tree.file_paths.push("test.txt".to_string());
            tree
        };

        let plain_path = temp_dir.path().join("plain.xml");
        let plain = Params {
            output_file: Some(plain_path.to_string_lossy().into_owned()),
            ..Params::default()
        };
        let (_, plain_size, plain_tokens) = output_repo_as_xml(
            &plain,
            file_tree(),
            temp_dir.path(),
            &tokenizer,
        )
        .unwrap();
        let expected_xml = fs::read(&plain_path).unwrap();
        assert_eq!(plain_size, expected_xml.len() as u64);

        for level in [1, 9] {
            let requested_path =
                temp_dir.path().join(format!("level-{level}.xml"));
            let compressed = Params {
                output_file: Some(
                    requested_path.to_string_lossy().into_owned(),
                ),
                gzip: true,
                gzip_level: level,
                ..Params::default()
            };

            let (_, compressed_size, compressed_tokens) = output_repo_as_xml(
                &compressed,
                file_tree(),
                temp_dir.path(),
                &tokenizer,
            )
            .unwrap();
            let effective_path = format!("{}.gz", requested_path.display());
            let gzip_bytes = fs::read(&effective_path).unwrap();
            assert_eq!(&gzip_bytes[..2], &[0x1f, 0x8b]);
            assert_eq!(compressed_size, gzip_bytes.len() as u64);
            assert_eq!(compressed_tokens, plain_tokens);

            let mut decoded = Vec::new();
            GzDecoder::new(gzip_bytes.as_slice())
                .read_to_end(&mut decoded)
                .unwrap();
            assert_eq!(decoded, expected_xml);
        }
    }

    #[test]
    fn test_gzip_effective_filename_keeps_existing_suffix() {
        let params = Params {
            output_file: Some("bundle.XML.GZ".to_string()),
            gzip: true,
            ..Params::default()
        };
        assert_eq!(effective_output_file(&params), Path::new("bundle.XML.GZ"));

        let default_name = Params {
            output_file: None,
            gzip: true,
            ..Params::default()
        };
        assert_eq!(
            effective_output_file(&default_name),
            Path::new(&format!("{DEFAULT_OUTPUT_FILE}.gz"))
        );
    }

    #[test]
    fn test_config_home_relative_output_path_is_expanded() {
        let home = tempdir().unwrap();
        let config = config::Config::builder()
            .set_override("output_file", "~/Documents/packed-repo.xml")
            .unwrap()
            .build()
            .unwrap();
        let params = Params::from(config);

        assert_eq!(
            effective_output_file_with_home(&params, Some(home.path())),
            home.path().join("Documents/packed-repo.xml")
        );
    }

    #[test]
    fn test_cli_home_relative_output_path_is_expanded() {
        let home = tempdir().unwrap();
        let params = Params {
            output_file: Some("~/quoted-output.xml".to_string()),
            ..Params::default()
        };

        assert_eq!(
            effective_output_file_with_home(&params, Some(home.path())),
            home.path().join("quoted-output.xml")
        );
    }

    #[test]
    fn test_non_home_relative_output_paths_are_unchanged() {
        let home = tempdir().unwrap();
        let absolute_path = home.path().join("absolute.xml");

        for output_file in [
            PathBuf::from("relative/output.xml"),
            absolute_path,
            PathBuf::from("~other/output.xml"),
        ] {
            let params = Params {
                output_file: Some(output_file.to_string_lossy().into_owned()),
                ..Params::default()
            };
            assert_eq!(
                effective_output_file_with_home(&params, Some(home.path())),
                output_file
            );
        }
    }

    #[test]
    fn test_gzip_suffix_is_applied_after_home_expansion() {
        let home = tempdir().unwrap();
        let params = Params {
            output_file: Some("~/packed-repo.xml".to_string()),
            gzip: true,
            ..Params::default()
        };
        assert_eq!(
            effective_output_file_with_home(&params, Some(home.path())),
            home.path().join("packed-repo.xml.gz")
        );

        let existing_suffix = Params {
            output_file: Some("~/packed-repo.XML.GZ".to_string()),
            gzip: true,
            ..Params::default()
        };
        assert_eq!(
            effective_output_file_with_home(
                &existing_suffix,
                Some(home.path())
            ),
            home.path().join("packed-repo.XML.GZ")
        );
    }

    #[test]
    fn test_home_relative_output_path_is_unchanged_without_home() {
        let params = Params {
            output_file: Some("~/packed-repo.xml".to_string()),
            ..Params::default()
        };

        assert_eq!(
            effective_output_file_with_home(&params, None),
            Path::new("~/packed-repo.xml")
        );
    }

    #[test]
    fn test_bare_home_component_is_not_expanded() {
        let home = tempdir().unwrap();

        for output_file in ["~", "~/"] {
            let params = Params {
                output_file: Some(output_file.to_string()),
                ..Params::default()
            };
            assert_eq!(
                effective_output_file_with_home(&params, Some(home.path()))
                    .as_os_str(),
                std::ffi::OsStr::new(output_file)
            );
        }
    }

    #[test]
    fn test_gzip_bare_home_component_stays_relative() {
        let home = tempdir().unwrap();

        for (output_file, expected) in [("~", "~.gz"), ("~/", "~/.gz")] {
            let params = Params {
                output_file: Some(output_file.to_string()),
                gzip: true,
                ..Params::default()
            };
            assert_eq!(
                effective_output_file_with_home(&params, Some(home.path())),
                Path::new(expected)
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_home_relative_output_path_is_expanded() {
        let home = tempdir().unwrap();
        let params = Params {
            output_file: Some(r"~\Documents\packed-repo.xml".to_string()),
            ..Params::default()
        };

        assert_eq!(
            effective_output_file_with_home(&params, Some(home.path())),
            home.path().join(r"Documents\packed-repo.xml")
        );
    }

    #[test]
    fn test_gzip_stdout_bytes_round_trip() {
        let xml = b"<repository />\n";
        let mut output = Vec::new();
        let mut timings = ProcessingTimings::default();
        write_stdout(&mut output, xml, true, 6, &mut timings).unwrap();
        assert_eq!(&output[..2], &[0x1f, 0x8b]);
        assert!(!timings.compression.is_zero());
        assert!(!timings.output_write_or_copy.is_zero());

        let mut decoded = Vec::new();
        GzDecoder::new(output.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, xml);
    }

    #[test]
    fn test_all_testable_destinations_use_canonical_serialization_bytes() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("test.txt"), "a <tag> & ]]> tail")
            .unwrap();
        let mut expected_tree = FileTree::default();
        expected_tree.file_paths.push("test.txt".to_string());
        let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
        let expected = serialize_repository_xml(
            &Params::default(),
            &expected_tree,
            temp_dir.path(),
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();

        let plain_path = temp_dir.path().join("plain.xml");
        let plain_params = Params {
            output_file: Some(plain_path.to_string_lossy().into_owned()),
            ..Params::default()
        };
        let mut plain_tree = FileTree::default();
        plain_tree.file_paths.push("test.txt".to_string());
        output_repo_as_xml(
            &plain_params,
            plain_tree,
            temp_dir.path(),
            &Model::GPT4.to_tokenizer().unwrap(),
        )
        .unwrap();
        assert_eq!(fs::read(plain_path).unwrap(), expected);

        let mut stdout = Vec::new();
        write_stdout(
            &mut stdout,
            &expected,
            false,
            6,
            &mut ProcessingTimings::default(),
        )
        .unwrap();
        assert_eq!(stdout, expected);

        let mut gzip_stdout = Vec::new();
        write_stdout(
            &mut gzip_stdout,
            &expected,
            true,
            6,
            &mut ProcessingTimings::default(),
        )
        .unwrap();
        let mut decoded = Vec::new();
        GzDecoder::new(gzip_stdout.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, expected);

        let clipboard_text = String::from_utf8(expected.clone()).unwrap();
        assert_eq!(clipboard_text.as_bytes(), expected);
    }

    #[test]
    fn test_gzip_clipboard_is_rejected() {
        let temp_dir = tempdir().unwrap();
        let params = Params {
            clipboard: true,
            gzip: true,
            ..Params::default()
        };
        let tokenizer = Model::GPT4.to_tokenizer().unwrap();
        let error = output_repo_as_xml(
            &params,
            FileTree::default(),
            temp_dir.path(),
            &tokenizer,
        )
        .unwrap_err();
        assert!(error.to_string().contains("--no-gzip --clipboard"));
    }

    #[test]
    fn test_gzip_stdout_takes_precedence_over_clipboard() {
        let params = Params {
            stdout: true,
            clipboard: true,
            gzip: true,
            ..Params::default()
        };
        assert!(validate_output_options_for(&params, false).is_ok());
    }

    #[test]
    fn test_gzip_stdout_rejects_terminal_output() {
        let params = Params {
            stdout: true,
            gzip: true,
            ..Params::default()
        };
        let error = validate_output_options_for(&params, true).unwrap_err();
        assert!(error.to_string().contains("redirect stdout"));
    }

    #[test]
    fn test_uncompressed_stdout_preserves_canonical_bytes() {
        let mut output = Vec::new();
        let mut timings = ProcessingTimings::default();
        write_stdout(&mut output, b"xml\n", false, 6, &mut timings).unwrap();
        assert_eq!(output, b"xml\n");
        assert!(timings.compression.is_zero());
        assert!(!timings.output_write_or_copy.is_zero());
    }
}
