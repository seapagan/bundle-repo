use crate::cli::Flags;
use crate::filelist::{FileTree, FolderNode};
use arboard::Clipboard;
use std::fs::{metadata, File};
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::path::Path;
use tiktoken_rs::CoreBPE;
use xml::writer::{EmitterConfig, EventWriter, XmlEvent};

/// Function to output the repository structure and files list to XML
pub fn output_repo_as_xml(
    flags: &Flags,
    file_tree: FileTree,
    base_path: &Path,
    tokenizer: &CoreBPE,
) -> Result<(usize, u64, usize), std::io::Error> {
    // Use an in-memory buffer instead of a physical file
    let mut buffer = Cursor::new(Vec::new());

    // Generate the XML content in memory
    buffer.write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n")?;
    buffer.write_all(b"<repository>\n")?;
    append_file_summary(&mut buffer)?;

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
    )?;
    buffer.write_all(b"</repository_files>\n")?;
    buffer.write_all(b"</repository>\n")?;

    // Output handling based on CLI flag
    if flags.stdout {
        // Print XML directly to stdout
        println!(
            "{}",
            String::from_utf8(buffer.into_inner()).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?
        );

        Ok((file_tree.file_paths.len(), 0, 0)) // Summary metrics not needed for stdout
    } else {
        // Extract XML content from buffer
        let xml_content =
            String::from_utf8(buffer.into_inner()).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?;

        if flags.clipboard {
            // Copy XML to clipboard
            let mut clipboard = Clipboard::new().map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, e)
            })?;
            clipboard.set_text(xml_content.clone()).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, e)
            })?;
        } else {
            // Write the XML to the specified output file
            let mut file = File::create(&flags.output_file)?;
            file.write_all(xml_content.as_bytes())?;
        }

        // Number of files processed
        let number_of_files = file_tree.file_paths.len();

        // Total size of the XML content
        let total_size = xml_content.len() as u64;

        // Calculate token count of the generated XML
        let token_count = tokenizer.encode_ordinary(&xml_content).len();

        Ok((number_of_files, total_size, token_count))
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

/// Function to write the repository files with contents to XML without escaping
fn write_repository_files_to_xml<W: Write>(
    writer: &mut W,
    file_paths: &Vec<String>,
    base_path: &Path,
    flags: &Flags, // Add the Flags reference here to access the lnumbers flag
) -> Result<(), std::io::Error> {
    for file_path in file_paths {
        let full_path = base_path.join(file_path);

        // Calculate file size
        let file_size = metadata(&full_path)?.len();

        // Check if file is binary using infer
        if is_binary_file(&full_path)? {
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
        match std::fs::read_to_string(&full_path) {
            Ok(mut contents) => {
                // Apply line numbering if the lnumbers flag is set
                if flags.lnumbers {
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
    std::io::Error::new(std::io::ErrorKind::Other, err)
}

/// Function to append the file summary section to the head of the XML output
///
/// This section provides information about the content and usage of the XML
/// file. It is designed to be read by AI systems for analysis, code review, or
/// other automated processes.
fn append_file_summary<W: Write>(
    writer: &mut W,
) -> Result<(), std::io::Error> {
    let file_summary = r#"
<file_summary>
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
    The LLM is instructed to focus solely on the repository's contents, including
    the code, file structure, and purpose of the files.
    Do not comment on the XML format, structure, or encoding of THIS FILE. Focus
    your analysis on the functionality, structure, and organization of the
    repository contents.
    Each <file> should be interpreted based on its file extension. For example:
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

    // Insert the file_summary after the opening <repository> tag
    writer.write_all(file_summary.as_bytes())?;

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
