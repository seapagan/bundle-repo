use crate::filelist::{FileTree, FolderNode};
use std::fs::{read_to_string, File};
use std::io::{self, Write};
use xml::writer::Error as XmlError;
use xml::writer::{EmitterConfig, EventWriter, XmlEvent};

// Function to output the repository structure and files list to XML
pub fn output_repo_as_xml(file_tree: FileTree) -> Result<(), io::Error> {
    let mut file = File::create("packed-repo.xml")?;

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
    let mut file = File::options().append(true).open("packed-repo.xml")?;

    // Add two <CR> between the repository_structure and repository_files nodes
    file.write_all(b"\n\n")?; // Two carriage returns for clarity

    // Write repository_files node manually using file I/O to avoid escaping
    file.write_all(b"<repository_files>\n")?; // Start repository_files node
    file.write_all(b"<summary>This node contains a list of files with their full paths and raw contents.</summary>\n")?; // Add <summary>
    write_repository_files_to_xml(&mut file, &file_tree.file_paths)?;
    file.write_all(b"</repository_files>\n")?; // End repository_files node

    // Close the root <repository> node
    file.write_all(b"</repository>\n")?;

    Ok(())
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
) -> Result<(), io::Error> {
    for file_path in file_paths {
        // Write the <file> node with the path attribute
        file.write_all(format!(r#"<file path="{}">"#, file_path).as_bytes())?;
        file.write_all(b"\n")?; // Proper newline after the opening <file> tag

        // Read the contents of the file
        match read_to_string(&file_path) {
            Ok(contents) => {
                // Write raw file contents without escaping
                file.write_all(contents.as_bytes())?;
            }
            Err(err) => {
                eprintln!("Failed to read {}: {}", file_path, err);
                file.write_all(b"<!-- Failed to read file -->")?;
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
