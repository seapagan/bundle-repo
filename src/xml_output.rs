use crate::filelist::{FileTree, FolderNode};
use std::fs::{read_to_string, File};
use std::io::{self, Write};
use xml::writer::Error as XmlError;
use xml::writer::{EmitterConfig, EventWriter, XmlEvent};

// Function to output the repository structure and files list to XML
pub fn output_filelist_as_xml(file_tree: FileTree) -> Result<(), io::Error> {
    let file = File::create("filelist.xml")?;
    {
        let mut writer = EmitterConfig::new()
            .perform_indent(true)
            .create_writer(file); // Use file directly, not &mut file

        // Ensure repository_structure is written first
        writer
            .write(XmlEvent::start_element("repository_structure"))
            .map_err(map_xml_error)?;
        write_folder_to_xml(&mut writer, &file_tree.folder_node)?;
        writer
            .write(XmlEvent::end_element())
            .map_err(map_xml_error)?;
    } // End the writer block here

    // Reopen the file for manual writing without the EventWriter
    let mut file = File::options().append(true).open("filelist.xml")?;

    // Add two <CR> between the repository_structure and repository_files nodes
    file.write_all(b"\n\n")?; // Two carriage returns for clarity

    // Write repository_files node manually using file I/O to avoid escaping
    file.write_all(b"<repository_files>\n")?; // Start repository_files node
    write_repository_files_to_xml(&mut file, &file_tree.file_paths)?;
    file.write_all(b"</repository_files>\n")?; // End repository_files node

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
