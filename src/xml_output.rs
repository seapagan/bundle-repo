use crate::filelist::FolderNode;
use std::fs::File;
use std::io;
use xml::writer::Error as XmlError;
use xml::writer::{EmitterConfig, XmlEvent};

pub fn output_filelist_as_xml(
    root_folder: FolderNode,
) -> Result<(), io::Error> {
    let file = File::create("filelist.xml")?;
    let mut writer = EmitterConfig::new()
        .perform_indent(true)
        .create_writer(file);

    writer
        .write(XmlEvent::start_element("repository_structure"))
        .map_err(map_xml_error)?;
    write_folder_to_xml(&mut writer, &root_folder)?;
    writer
        .write(XmlEvent::end_element())
        .map_err(map_xml_error)?;

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
