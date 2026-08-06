use crate::filelist::{FileTree, FolderNode};
use crate::progress::ProgressReporter;
use crate::structs::{Params, DEFAULT_OUTPUT_FILE};
use crate::text_processing::{read_classify_and_decode, ProcessedFile};
use crate::timings::ProcessingTimings;
use crate::tokenizer::TokenizerType;
use arboard::Clipboard;
use dirs_next::home_dir;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{metadata, File};
use std::io::{self, Cursor, IsTerminal, Write};
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
        reporter,
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
        model_name,
        reporter,
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

    let xml_content = String::from_utf8(xml_bytes)
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
            .set_text(xml_content.clone())
            .map_err(io::Error::other)?;
        timings.output_write_or_copy += write_start.elapsed();
        xml_content.len() as u64
    } else {
        let output_path = effective_output_file(flags);
        reporter.phase(&destination_phase(flags))?;
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
        output.write_all(b"\n")?;
        timings.output_write_or_copy += write_start.elapsed();
    }
    output.flush()
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

/// Function to write the repository files with contents to XML without escaping
fn write_repository_files_to_xml<W: Write, N: Write, D: Write>(
    writer: &mut W,
    file_paths: &Vec<String>,
    base_path: &Path,
    flags: &Params,
    reporter: &mut ProgressReporter<N, D>,
    timings: &mut ProcessingTimings,
) -> Result<(), std::io::Error> {
    for file_path in file_paths {
        let full_path = base_path.join(file_path);

        // Calculate file size
        let file_size = metadata(&full_path)?.len();

        // Check if file is binary using infer
        match read_classify_and_decode(&full_path, flags.utf8, timings) {
            Ok(ProcessedFile::Text(mut decoded)) => {
                if let Some(ref conversion) = decoded.conversion {
                    reporter.conversion(file_path, conversion)?;
                }
                if decoded.utf8_had_replacements {
                    reporter.malformed_utf8_replacement(file_path)?;
                }
                // Apply line numbering if the lnumbers flag is set
                if flags.line_numbers {
                    decoded.text = add_line_numbers(&decoded.text);
                }

                // Calculate number of lines
                let line_count = decoded.text.lines().count();

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
                writer.write_all(decoded.text.as_bytes())?;
                writer.write_all(b"</file>\n\n")?; // Close the <file> node
            }
            Ok(ProcessedFile::Binary(_)) => {
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
            }
            Err(err) => {
                // For other types of errors, write a general failure message with the error description
                let error_message = err.to_string();
                reporter.error(&format!(
                    "Error reading file '{}': {}",
                    full_path.display(),
                    error_message
                ))?;
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
    use std::io::Read;
    use tempfile::tempdir;

    const JAPANESE: &str = "これは文字コード検出のための日本語の文章です。複数の文を含めて、短い入力による誤判定を避けます。古い文書も正しく読み取ります。\n";
    const SIMPLIFIED: &str = "这是用于字符编码检测的中文文本。它包含多个自然句子，以避免短输入造成误判。旧文件也应该被正确读取。\n";
    const GB18030: &str = "这是用于字符编码检测的中文文本。它包含多个自然句子，以避免短输入造成误判。扩展字符𠀀用于验证四字节编码。\n";
    const TRADITIONAL: &str = "這是用於字元編碼偵測的中文文字。它包含多個自然句子，以避免短輸入造成誤判。舊檔案也應該被正確讀取。\n";
    const RUSSIAN: &str = "Это русский текст для проверки определения кодировки. Он содержит несколько естественных предложений. Старые файлы должны читаться правильно.\n";
    const WESTERN: &str = "Voici un texte français pour vérifier la détection d’encodage. Il contient plusieurs phrases naturelles. Les fichiers anciens doivent être lus correctement.\n";
    const UTF16: &str = "UTF-16 text with 日本語, русский текст, and العربية. This fixture contains multiple natural sentences. It verifies byte-order-mark handling.\n";
    const ENCODING_FIXTURES: [(&str, &[u8], &str); 10] = [
        (
            "shift-jis.txt",
            include_bytes!("../tests/fixtures/encodings/shift-jis.txt"),
            JAPANESE,
        ),
        (
            "euc-jp.txt",
            include_bytes!("../tests/fixtures/encodings/euc-jp.txt"),
            JAPANESE,
        ),
        (
            "iso-2022-jp.txt",
            include_bytes!("../tests/fixtures/encodings/iso-2022-jp.txt"),
            JAPANESE,
        ),
        (
            "gbk.txt",
            include_bytes!("../tests/fixtures/encodings/gbk.txt"),
            SIMPLIFIED,
        ),
        (
            "gb18030.txt",
            include_bytes!("../tests/fixtures/encodings/gb18030.txt"),
            GB18030,
        ),
        (
            "big5.txt",
            include_bytes!("../tests/fixtures/encodings/big5.txt"),
            TRADITIONAL,
        ),
        (
            "windows-1251.txt",
            include_bytes!("../tests/fixtures/encodings/windows-1251.txt"),
            RUSSIAN,
        ),
        (
            "windows-1252.txt",
            include_bytes!("../tests/fixtures/encodings/windows-1252.txt"),
            WESTERN,
        ),
        (
            "utf-16le.txt",
            include_bytes!("../tests/fixtures/encodings/utf-16le.txt"),
            UTF16,
        ),
        (
            "utf-16be.txt",
            include_bytes!("../tests/fixtures/encodings/utf-16be.txt"),
            UTF16,
        ),
    ];

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
    fn test_encoding_fixture_matrix_is_included_as_valid_utf8_xml() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        let mut file_tree = FileTree::default();
        for (name, bytes, _) in ENCODING_FIXTURES {
            fs::write(temp_dir.path().join(name), bytes).unwrap();
            file_tree.file_paths.push(name.to_string());
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
        for (name, _, expected) in ENCODING_FIXTURES {
            let start = format!("<file path=\"{name}\"");
            let file_xml = xml.split(&start).nth(1).unwrap();
            assert!(file_xml
                .split("</file>")
                .next()
                .unwrap()
                .contains(expected));
        }
    }

    #[test]
    fn test_utf16_fixtures_are_excluded_when_conversion_is_disabled() {
        let temp_dir = tempdir().unwrap();
        let output_file = temp_dir.path().join("output.xml");
        let mut file_tree = FileTree::default();
        for (name, bytes) in [
            (
                "utf-16le.txt",
                include_bytes!("../tests/fixtures/encodings/utf-16le.txt")
                    .as_slice(),
            ),
            (
                "utf-16be.txt",
                include_bytes!("../tests/fixtures/encodings/utf-16be.txt")
                    .as_slice(),
            ),
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
        fs::write(
            temp_dir.path().join("legacy.txt"),
            include_bytes!("../tests/fixtures/encodings/windows-1252.txt"),
        )
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
        let mut malformed =
            include_bytes!("../tests/fixtures/encodings/utf-16le.txt")
                .to_vec();
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
        let mut xml = Vec::new();
        let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);

        write_repository_files_to_xml(
            &mut xml,
            &paths,
            temp_dir.path(),
            &params,
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();

        assert!(String::from_utf8(xml)
            .unwrap()
            .contains("\u{feff}malformed \u{fffd} text"));
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
                assert_eq!(output, [xml.as_slice(), b"\n"].concat());
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
        let mut timings = ProcessingTimings::default();
        write_stdout(&mut output, b"xml\n", false, 6, &mut timings).unwrap();
        assert_eq!(output, b"xml\n\n");
        assert!(timings.compression.is_zero());
        assert!(!timings.output_write_or_copy.is_zero());
    }
}
