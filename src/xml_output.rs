use crate::filelist::{FileTree, FolderNode};
use crate::structs::{Params, DEFAULT_OUTPUT_FILE};
use crate::timings::ProcessingTimings;
use crate::tokenizer::TokenizerType;
use arboard::Clipboard;
use dirs_next::home_dir;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{metadata, File};
use std::io::{self, BufReader, Cursor, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use xml::writer::{EmitterConfig, EventWriter, XmlEvent};

/// Function to output the repository structure and files list to XML
#[cfg(test)]
pub fn output_repo_as_xml(
    flags: &Params,
    file_tree: FileTree,
    base_path: &Path,
    tokenizer: &TokenizerType,
) -> Result<(usize, u64, usize), std::io::Error> {
    output_repo_as_xml_with_timings(
        flags,
        file_tree,
        base_path,
        tokenizer,
        &mut ProcessingTimings::default(),
    )
}

pub fn output_repo_as_xml_with_timings(
    flags: &Params,
    file_tree: FileTree,
    base_path: &Path,
    tokenizer: &TokenizerType,
    timings: &mut ProcessingTimings,
) -> Result<(usize, u64, usize), std::io::Error> {
    validate_output_options(flags)?;
    let xml_start = Instant::now();

    // Use an in-memory buffer instead of a physical file
    let mut buffer = Cursor::new(Vec::new());

    // Generate the XML content in memory
    buffer.write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n")?;
    buffer.write_all(b"<repository>\n")?;
    append_file_summary(&mut buffer, flags)?;

    // Write repository structure and repository files nodes
    {
        let mut writer = EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(false)
            .create_writer(&mut buffer);

        writer
            .write(XmlEvent::start_element("repository_structure"))
            .map_err(map_xml_error)?;
        writer
            .write(XmlEvent::start_element("summary"))
            .map_err(map_xml_error)?;
        writer
            .write(XmlEvent::characters(
                "This node contains the hierarchical structure of the repository's files and folders.",
            ))
            .map_err(map_xml_error)?;
        writer
            .write(XmlEvent::end_element())
            .map_err(map_xml_error)?; // Close <summary>

        write_folder_to_xml(&mut writer, &file_tree.folder_node)?;
        writer
            .write(XmlEvent::end_element())
            .map_err(map_xml_error)?; // Close <repository_structure>
    }

    buffer.write_all(b"\n\n")?;
    buffer.write_all(b"<repository_files>\n")?;
    buffer.write_all(b"<summary>This node contains a list of files with their full paths and raw contents.</summary>\n")?;
    write_repository_files_to_xml(
        &mut buffer,
        &file_tree.file_paths,
        base_path,
        flags,
        timings,
    )?;
    buffer.write_all(b"</repository_files>\n")?;
    buffer.write_all(b"</repository>\n")?;
    timings.xml_generation += xml_start
        .elapsed()
        .checked_sub(timings.file_classification_and_read)
        .and_then(|duration| {
            duration.checked_sub(timings.utf8_validation_or_transcode)
        })
        .unwrap_or_default();

    finish_output(
        flags,
        file_tree.file_paths.len(),
        buffer.into_inner(),
        tokenizer,
        timings,
    )
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

fn finish_output(
    flags: &Params,
    number_of_files: usize,
    xml_bytes: Vec<u8>,
    tokenizer: &TokenizerType,
    timings: &mut ProcessingTimings,
) -> Result<(usize, u64, usize), io::Error> {
    if flags.stdout {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        if flags.gzip {
            let compression_start = Instant::now();
            let compressed = compress_gzip(&xml_bytes, flags.gzip_level)?;
            timings.compression += compression_start.elapsed();
            let write_start = Instant::now();
            output.write_all(&compressed)?;
            timings.output_write_or_copy += write_start.elapsed();
        } else {
            std::str::from_utf8(&xml_bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let write_start = Instant::now();
            output.write_all(&xml_bytes)?;
            output.write_all(b"\n")?;
            timings.output_write_or_copy += write_start.elapsed();
        }
        output.flush()?;
        return Ok((number_of_files, 0, 0));
    }

    let xml_content = String::from_utf8(xml_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let token_start = Instant::now();
    let token_count = tokenizer
        .count_tokens(&xml_content)
        .map_err(io::Error::other)?;
    timings.token_count += token_start.elapsed();
    let total_size = if flags.clipboard {
        let write_start = Instant::now();
        let mut clipboard = Clipboard::new().map_err(io::Error::other)?;
        clipboard
            .set_text(xml_content.clone())
            .map_err(io::Error::other)?;
        timings.output_write_or_copy += write_start.elapsed();
        xml_content.len() as u64
    } else {
        let output_bytes = if flags.gzip {
            let compression_start = Instant::now();
            let compressed =
                compress_gzip(xml_content.as_bytes(), flags.gzip_level)?;
            timings.compression += compression_start.elapsed();
            compressed
        } else {
            xml_content.into_bytes()
        };
        let write_start = Instant::now();
        let mut file = File::create(effective_output_file(flags))?;
        file.write_all(&output_bytes)?;
        timings.output_write_or_copy += write_start.elapsed();
        output_bytes.len() as u64
    };

    Ok((number_of_files, total_size, token_count))
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

#[cfg(test)]
fn write_stdout<W: Write>(
    output: &mut W,
    content: &[u8],
    gzip: bool,
    level: u32,
) -> io::Result<()> {
    if gzip {
        output.write_all(&compress_gzip(content, level)?)
    } else {
        std::str::from_utf8(content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        output.write_all(content)?;
        output.write_all(b"\n")
    }
}

/// Function to write folder structure to XML using EventWriter
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

fn read_file_contents_with_timings(
    path: &Path,
    force_utf8: bool,
    timings: &mut ProcessingTimings,
) -> io::Result<String> {
    let read_start = Instant::now();
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    timings.file_classification_and_read += read_start.elapsed();

    let validation_start = Instant::now();
    let result = if force_utf8 {
        // Try to decode as UTF-8
        match String::from_utf8(buffer.clone()) {
            Ok(content) => Ok(content),
            Err(_) => {
                // If UTF-8 decoding fails, try to convert to UTF-8
                Ok(encoding_rs::UTF_8.decode(&buffer).0.into_owned())
            }
        }
    } else {
        // Default behavior - try to read as is
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    };
    timings.utf8_validation_or_transcode += validation_start.elapsed();
    if result.is_ok() {
        timings.valid_files += 1;
        timings.valid_bytes += buffer.len() as u64;
    }
    result
}

/// Function to write the repository files with contents to XML without escaping
fn write_repository_files_to_xml<W: Write>(
    writer: &mut W,
    file_paths: &Vec<String>,
    base_path: &Path,
    flags: &Params,
    timings: &mut ProcessingTimings,
) -> Result<(), std::io::Error> {
    for file_path in file_paths {
        let full_path = base_path.join(file_path);

        // Calculate file size
        let file_size = metadata(&full_path)?.len();

        // Check if file is binary using infer
        let classification_start = Instant::now();
        let is_binary = is_binary_file(&full_path)?;
        timings.file_classification_and_read += classification_start.elapsed();
        if is_binary {
            writer.write_all(
                format!(
                    r#"<file path="{}" size="{}" lines="0">"#,
                    file_path, file_size
                )
                .as_bytes(),
            )?;
            writer.write_all(
                b"\n<!-- This file is a binary file and not included -->\n",
            )?;
            writer.write_all(b"</file>\n\n")?;
            continue;
        }

        // Try to read the file contents using the full path
        match read_file_contents_with_timings(&full_path, flags.utf8, timings)
        {
            Ok(mut contents) => {
                // Apply line numbering if the lnumbers flag is set
                if flags.line_numbers {
                    contents = add_line_numbers(&contents);
                }

                // Calculate number of lines
                let line_count = contents.lines().count();

                // Write the <file> node with size and line attributes
                writer.write_all(
                    format!(
                        r#"<file path="{}" size="{}" lines="{}">"#,
                        file_path, file_size, line_count
                    )
                    .as_bytes(),
                )?;
                writer.write_all(b"\n")?; // Proper newline after the opening <file> tag

                // Write raw file contents without escaping
                writer.write_all(contents.as_bytes())?;
                writer.write_all(b"</file>\n\n")?; // Close the <file> node
            }
            Err(err) => {
                // For other types of errors, write a general failure message with the error description
                let error_message = err.to_string();
                eprintln!(
                    "Error reading file '{}': {}",
                    full_path.display(),
                    error_message
                );
                writer.write_all(
                    format!(
                        r#"<file path="{}" size="0" lines="0">"#,
                        file_path
                    )
                    .as_bytes(),
                )?;
                writer.write_all(
                    format!(
                        "<!-- Failed to read file: {} -->\n</file>\n\n",
                        error_message
                    )
                    .as_bytes(),
                )?;
            }
        }
    }

    Ok(())
}

/// Map XML writing errors to IO errors
fn map_xml_error(err: xml::writer::Error) -> std::io::Error {
    std::io::Error::other(err)
}

/// Function to append the file summary section to the head of the XML output.
/// This section provides information about the content and usage of the XML file,
/// and dynamically adjusts the instructions if line numbers are present.
///
/// Args:
///     writer: The writer to which the summary will be written.
///     flags: The CLI flags to determine if line numbering is active.
///
/// Returns:
///     A result with any IO errors encountered.
fn append_file_summary<W: Write>(
    writer: &mut W,
    flags: &Params,
) -> Result<(), std::io::Error> {
    // First part of the file summary up to the optional line number instructions
    let first_part = r#"<file_summary>
  <purpose>
    This file contains a packed representation of the entire repository's contents.
    It is designed to be easily consumable by AI systems for analysis, code review,
    or other automated processes.
  </purpose>

  <file_format>
    The content is organized as follows:
    1. This summary section
    2. Repository structure: A hierarchical listing of all folders and files in the repository.
    3. Repository files: Each file is listed with:
      - File path as an attribute
      - Full contents of the file, excluding binary files.
  </file_format>

  <instructions>
    - The LLM is instructed to focus solely on the repository's contents, including
      the code, file structure, and purpose of the files.
    - Do not comment on the XML format, structure, or encoding of THIS FILE. Focus
      your analysis on the functionality, structure, and organization of the
      repository contents."#;

    // if the --lnumbers flag is set, add line number instructions
    let optional_part = if flags.line_numbers {
        r#"
    - Line numbers have been added to the code for reference. Please use them for
      referring to specific lines of code when needed. However, do NOT include line
      numbers when outputting or displaying code in responses."#
    } else {
        ""
    };

    // Final part: Everything after the optional instructions
    let final_part = r#"
    - Each <file> should be interpreted based on its file extension. For example:
      - ".py" for Python
      - ".md" for Markdown
      - ".rs" for Rust
      - ".cpp" for C++
  </instructions>

  <usage_guidelines>
    - This file should be treated as read-only. Any changes should be made to the
      original repository files, not this packed version.
    - When processing this file, use the file path to distinguish
      between different files in the repository.
    - Be aware that this file may contain sensitive information. Handle it with
      the same level of security as you would the original repository.
  </usage_guidelines>

  <notes>
    - Some files may have been excluded based on .gitignore rules and bundlerepo's
      configuration.
    - Binary files are not included in this packed representation. Please refer to
      the Repository Structure section for a complete list of file paths, including
      binary files.
  </notes>

  <additional_info>
    For more information about bundlerepo, visit: https://github.com/seapagan/bundle-repo
  </additional_info>
</file_summary>
"#;

    // Concatenate the parts and write to the writer in one go
    writer.write_all(
        format!("{}{}{}", first_part, optional_part, final_part).as_bytes(),
    )?;

    Ok(())
}

/// Determines if a file is binary by using magic number detection.
fn is_binary_file(path: &Path) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut buffer = [0; 1024];

    // Read the first chunk of the file
    let bytes_read = file.read(&mut buffer)?;

    // Use the infer crate to detect the type
    if let Some(kind) = infer::get(&buffer[..bytes_read]) {
        // Check if the file is not recognized as a text format
        if kind.mime_type().starts_with("text/") {
            return Ok(false);
        } else {
            return Ok(true);
        }
    }

    // If infer can't determine, fall back to heuristic approach
    let mut non_printable_count = 0;
    for &byte in &buffer[..bytes_read] {
        if byte < 0x09 || (byte > 0x0D && byte < 0x20) || byte > 0x7E {
            non_printable_count += 1;
        }
    }
    let threshold = (bytes_read as f32) * 0.3;
    Ok(non_printable_count as f32 > threshold)
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
    use crate::tokenizer::Model;
    use flate2::read::GzDecoder;
    use std::fs;
    use tempfile::tempdir;

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
    fn test_is_binary_file() {
        let temp_dir = tempdir().unwrap();

        // Create a text file
        let text_path = temp_dir.path().join("test.txt");
        fs::write(&text_path, "Hello, World!").unwrap();
        assert!(!is_binary_file(&text_path).unwrap());

        // Create a binary file
        let binary_path = temp_dir.path().join("test.bin");
        fs::write(&binary_path, [0u8, 159u8, 146u8, 150u8]).unwrap();
        assert!(is_binary_file(&binary_path).unwrap());
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

        let xml_content = fs::read_to_string(output_file).unwrap();
        assert!(xml_content
            .contains("<!-- This file is a binary file and not included -->"));
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
        write_stdout(&mut output, xml, true, 6).unwrap();
        assert_eq!(&output[..2], &[0x1f, 0x8b]);

        let mut decoded = Vec::new();
        GzDecoder::new(output.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, xml);
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
    fn test_uncompressed_stdout_preserves_trailing_newline() {
        let mut output = Vec::new();
        write_stdout(&mut output, b"xml\n", false, 6).unwrap();
        assert_eq!(output, b"xml\n\n");
    }
}
