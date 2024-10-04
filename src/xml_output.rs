use crate::filelist::{FileTree, FolderNode};
use std::fs::{metadata, read_to_string, File};
use std::io::{self, Write};
use std::path::Path;
use tiktoken_rs::CoreBPE;
use xml::writer::Error as XmlError;
use xml::writer::{EmitterConfig, EventWriter, XmlEvent};

// Function to output the repository structure and files list to XML
// Function to output the repository structure and files list to XML
pub fn output_repo_as_xml(
    output_file: &str,
    file_tree: FileTree,
    base_path: &Path,
    tokenizer: &CoreBPE,
) -> Result<(usize, u64, usize), io::Error> {
    let mut file = File::create(output_file)?;

    // Manually add the XML declaration at the very top of the file
    file.write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n")?;

    // Start the root <repository> node
    file.write_all(b"<repository>\n")?;

    append_file_summary(&mut file)?;
    {
        let mut writer = EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(false) // Disable automatic XML declaration
            .create_writer(file); // Use file directly, not &mut file

        // Ensure repository_structure is written first
        writer
            .write(XmlEvent::start_element("repository_structure"))
            .map_err(map_xml_error)?;
        writer
            .write(XmlEvent::start_element("summary"))
            .map_err(map_xml_error)?;
        writer.write(XmlEvent::characters("This node contains the hierarchical structure of the repository's files and folders.")).map_err(map_xml_error)?;
        writer
            .write(XmlEvent::end_element())
            .map_err(map_xml_error)?; // Close <summary>

        write_folder_to_xml(&mut writer, &file_tree.folder_node)?;
        writer
            .write(XmlEvent::end_element())
            .map_err(map_xml_error)?;
    } // End the writer block here

    // Reopen the file for manual writing without the EventWriter
    let mut file = File::options().append(true).open(output_file)?;

    // Add two <CR> between the repository_structure and repository_files nodes
    file.write_all(b"\n\n")?; // Two carriage returns for clarity

    // Pass the base_path to ensure correct file paths are used
    file.write_all(b"<repository_files>\n")?; // Start repository_files node
    file.write_all(b"<summary>This node contains a list of files with their full paths and raw contents.</summary>\n")?; // Add <summary>
    write_repository_files_to_xml(
        &mut file,
        &file_tree.file_paths,
        base_path,
    )?;
    file.write_all(b"</repository_files>\n")?; // End repository_files node

    // Close the root <repository> node
    file.write_all(b"</repository>\n")?;

    // Number of files processed
    let number_of_files = file_tree.file_paths.len();

    // Total size of the output file
    let total_size = file.metadata()?.len(); // Total size of the written XML file

    // Now let's calculate the token count of the generated XML
    let xml_content = std::fs::read_to_string(output_file)?; // Read the XML file
    let token_count = tokenizer.encode_ordinary(&xml_content).len(); // Count the tokens

    Ok((number_of_files, total_size, token_count))
}

// Function to write folder structure to XML using EventWriter
fn write_folder_to_xml(
    writer: &mut EventWriter<File>,
    folder_node: &FolderNode,
) -> Result<(), io::Error> {
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

// Function to write the repository files with contents to XML without escaping
fn write_repository_files_to_xml(
    file: &mut File,
    file_paths: &Vec<String>,
    base_path: &Path,
) -> Result<(), io::Error> {
    for file_path in file_paths {
        let full_path = base_path.join(file_path);

        // Calculate file size
        let file_size = metadata(&full_path)?.len();

        // Try to read the file contents using the full path
        match read_to_string(&full_path) {
            Ok(contents) => {
                // Calculate number of lines
                let line_count = contents.lines().count();

                // Write the <file> node with size and line attributes
                file.write_all(
                    format!(
                        r#"<file path="{}" size="{}" lines="{}">"#,
                        file_path, file_size, line_count
                    )
                    .as_bytes(),
                )?;
                file.write_all(b"\n")?; // Proper newline after the opening <file> tag

                // Write raw file contents without escaping
                file.write_all(contents.as_bytes())?;
            }
            Err(err) => {
                // Check for specific phrase in the error message to determine if it's a UTF-8 error
                let error_message = err.to_string();
                if error_message.contains("stream did not contain valid UTF-8")
                {
                    // Handle UTF-8 decoding error: Assume it's a binary file
                    file.write_all(
                        format!(
                            r#"<file path="{}" size="{}" lines="0">"#,
                            file_path, file_size
                        )
                        .as_bytes(),
                    )?;
                    file.write_all(
                        b"\n<!-- This file is a binary file and not included -->\n",
                    )?;
                } else {
                    // For other types of errors, write a general failure message
                    file.write_all(
                        format!(
                            r#"<file path="{}" size="0" lines="0">"#,
                            file_path
                        )
                        .as_bytes(),
                    )?;
                    file.write_all(b"\n<!-- Failed to read file -->\n")?;
                }
            }
        }

        // End the <file> node, add a new line
        file.write_all(b"</file>\n\n")?;
    }

    Ok(())
}

// Map XML writing errors to IO errors
fn map_xml_error(err: XmlError) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}

fn append_file_summary(file: &mut File) -> Result<(), io::Error> {
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
    - Code comments have been removed for brevity.
  </notes>

  <additional_info>
    For more information about bundlerepo, visit: https://github.com/seapagan/bundle-repo
  </additional_info>
</file_summary>

"#;

    // Insert the file_summary after the opening <repository> tag
    file.write_all(file_summary.as_bytes())?;

    Ok(())
}
