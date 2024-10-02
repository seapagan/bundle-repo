use crate::filelist::{FileTree, FolderNode};
use std::fs::read_to_string;
use std::fs::File;
use std::io;
use xml::writer::Error as XmlError;
use xml::writer::{EmitterConfig, XmlEvent};

pub fn output_filelist_as_xml(file_tree: FileTree) -> Result<(), io::Error> {
    let file = File::create("filelist.xml")?;
    let mut writer = EmitterConfig::new()
        .perform_indent(true)
        .create_writer(file);

    // Write the repository_structure node
    writer
        .write(XmlEvent::start_element("repository_structure"))
        .map_err(map_xml_error)?;
    write_folder_to_xml(&mut writer, &file_tree.folder_node)?;
    writer
        .write(XmlEvent::end_element())
        .map_err(map_xml_error)?;

    // Write the repository_files node
    writer
        .write(XmlEvent::start_element("repository_files"))
        .map_err(map_xml_error)?;
    write_repository_files_to_xml(&mut writer, &file_tree.file_paths)?;
    writer
        .write(XmlEvent::end_element())
        .map_err(map_xml_error)?;

    Ok(())
}

fn write_repository_files_to_xml(
    writer: &mut xml::writer::EventWriter<File>,
    file_paths: &Vec<String>,
) -> Result<(), io::Error> {
    for file_path in file_paths {
        // Start the <file> node with the path attribute
        writer
            .write(XmlEvent::start_element("file").attr("path", file_path))
            .map_err(map_xml_error)?;

        // Read the contents of the file
        match read_to_string(&file_path) {
            Ok(contents) => {
                // Write the raw contents into the XML without escaping characters
                writer
                    .write(XmlEvent::characters(&contents))
                    .map_err(map_xml_error)?;
            }
            Err(err) => {
                eprintln!("Failed to read {}: {}", file_path, err);
                writer
                    .write(XmlEvent::characters(
                        "<!-- Failed to read file -->",
                    ))
                    .map_err(map_xml_error)?;
            }
        }

        // End the <file> node
        writer
            .write(XmlEvent::end_element())
            .map_err(map_xml_error)?;
    }

    Ok(())
}

fn write_folder_to_xml(
    writer: &mut xml::writer::EventWriter<File>,
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

fn map_xml_error(err: XmlError) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}
