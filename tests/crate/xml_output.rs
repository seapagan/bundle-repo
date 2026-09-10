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
use xml::attribute::OwnedAttribute;
use xml::reader::{ParserConfig, XmlEvent as ReaderXmlEvent};

#[derive(Debug)]
struct ParsedFile {
    attributes: Vec<(String, String)>,
    text: String,
    comments: Vec<String>,
}

#[derive(Debug, Default, PartialEq)]
struct ParsedMetadata {
    summary: String,
    entries: Vec<(String, String)>,
}

fn parse_metadata(xml: &[u8]) -> Option<ParsedMetadata> {
    let mut parsed = None;
    let mut element = String::new();
    for event in parse_document(xml) {
        match event {
            ReaderXmlEvent::StartElement {
                name, attributes, ..
            } => {
                element = name.local_name;
                if element == "repository_metadata" {
                    parsed = Some(ParsedMetadata::default());
                } else if element == "entry"
                    && let Some(metadata) = &mut parsed
                {
                    metadata.entries.push((
                        reader_attribute(&attributes, "key"),
                        String::new(),
                    ));
                }
            }
            ReaderXmlEvent::Characters(text)
            | ReaderXmlEvent::Whitespace(text) => {
                if let Some(metadata) = &mut parsed {
                    match element.as_str() {
                        "summary" => metadata.summary.push_str(&text),
                        "entry" => metadata
                            .entries
                            .last_mut()
                            .unwrap()
                            .1
                            .push_str(&text),
                        _ => {}
                    }
                }
            }
            ReaderXmlEvent::EndElement { name } => {
                if name.local_name == "repository_metadata" {
                    break;
                }
                element.clear();
            }
            _ => {}
        }
    }
    parsed
}

fn parse_top_level_sections(xml: &[u8]) -> Vec<String> {
    let mut depth = 0;
    let mut sections = Vec::new();
    for event in parse_document(xml) {
        match event {
            ReaderXmlEvent::StartElement { name, .. } => {
                if depth == 1 {
                    sections.push(name.local_name);
                }
                depth += 1;
            }
            ReaderXmlEvent::EndElement { .. } => depth -= 1,
            _ => {}
        }
    }
    sections
}

fn parse_element_text(xml: &[u8], expected: &str) -> String {
    let mut active = false;
    let mut text = String::new();
    for event in parse_document(xml) {
        match event {
            ReaderXmlEvent::StartElement { name, .. } => {
                active = name.local_name == expected
            }
            ReaderXmlEvent::Characters(value)
            | ReaderXmlEvent::Whitespace(value)
                if active =>
            {
                text.push_str(&value)
            }
            ReaderXmlEvent::EndElement { name }
                if name.local_name == expected =>
            {
                break;
            }
            _ => {}
        }
    }
    text
}

fn metadata_source(
    identity: crate::configuration::ConfigSourceIdentity,
    entries: &[(&str, &str)],
) -> crate::configuration::MetadataSource {
    use crate::configuration::{
        CandidateValue, MetadataCandidate, MetadataSource,
    };
    MetadataSource {
        identity,
        entries: entries
            .iter()
            .map(|(key, value)| MetadataCandidate {
                key: (*key).to_string(),
                key_line: 1,
                value_line: 1,
                value: CandidateValue::String((*value).to_string()),
            })
            .collect(),
    }
}

fn metadata_params(entries: &[(&str, &str)]) -> Params {
    let source = metadata_source(
        crate::configuration::ConfigSourceIdentity::Global,
        entries,
    );
    Params {
        metadata: crate::configuration::validate_and_merge_metadata(
            &[source],
            &SecretScanner::from_bundled().unwrap(),
        )
        .unwrap(),
        ..Params::default()
    }
}

fn serialize_metadata_fixture(
    flags: &Params,
    skipped: &[SkippedRepositoryItem],
) -> Vec<u8> {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join("test.txt"), "ordinary text").unwrap();
    let mut tree = FileTree::default();
    tree.file_paths.push("test.txt".to_string());
    tree.folder_node.files.push("test.txt".to_string());
    serialize_repository_xml(
        flags,
        &tree,
        skipped,
        temp_dir.path(),
        None,
        &mut ProgressReporter::new(Vec::new(), Vec::new(), true),
        &mut ProcessingTimings::default(),
    )
    .unwrap()
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
    let mut parsed = None;
    for event in repository_file_events(parse_document(xml)) {
        if collect_file_event(event, expected_path, &mut parsed) {
            break;
        }
    }
    parsed.unwrap_or_else(|| {
        panic!("missing file element for {expected_path:?}")
    })
}

fn repository_file_events(
    events: Vec<ReaderXmlEvent>,
) -> impl Iterator<Item = ReaderXmlEvent> {
    events
        .into_iter()
        .skip_while(|event| {
            !matches!(
                event,
                ReaderXmlEvent::StartElement { name, .. }
                    if name.local_name == "repository_files"
            )
        })
        .skip(1)
        .take_while(|event| {
            !matches!(
                event,
                ReaderXmlEvent::EndElement { name }
                    if name.local_name == "repository_files"
            )
        })
}

fn collect_file_event(
    event: ReaderXmlEvent,
    expected_path: &str,
    parsed: &mut Option<ParsedFile>,
) -> bool {
    match event {
        ReaderXmlEvent::StartElement {
            name, attributes, ..
        } if is_expected_file(
            &name.local_name,
            &attributes,
            expected_path,
        ) =>
        {
            *parsed = Some(ParsedFile {
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
        ReaderXmlEvent::Characters(text) | ReaderXmlEvent::CData(text)
            if parsed.is_some() =>
        {
            parsed.as_mut().unwrap().text.push_str(&text);
        }
        ReaderXmlEvent::Comment(comment) if parsed.is_some() => {
            parsed.as_mut().unwrap().comments.push(comment);
        }
        ReaderXmlEvent::EndElement { name }
            if parsed.is_some() && name.local_name == "file" =>
        {
            return true;
        }
        _ => {}
    }
    false
}

fn is_expected_file(
    element_name: &str,
    attributes: &[OwnedAttribute],
    expected_path: &str,
) -> bool {
    element_name == "file"
        && attributes.iter().any(|attribute| {
            attribute.name.local_name == "path"
                && attribute.value == expected_path
        })
}

fn attribute<'a>(file: &'a ParsedFile, name: &str) -> &'a str {
    file.attributes
        .iter()
        .find(|(attribute, _)| attribute == name)
        .map(|(_, value)| value.as_str())
        .unwrap()
}

fn reader_attribute(attributes: &[OwnedAttribute], name: &str) -> String {
    attributes
        .iter()
        .find(|attribute| attribute.name.local_name == name)
        .unwrap()
        .value
        .clone()
}

fn parse_skipped(xml: &[u8]) -> Vec<Vec<(String, String)>> {
    parse_document(xml)
        .into_iter()
        .skip_while(|event| {
            !matches!(
                event,
                ReaderXmlEvent::StartElement { name, .. }
                    if name.local_name == "repository_skipped"
            )
        })
        .skip(1)
        .take_while(|event| {
            !matches!(
                event,
                ReaderXmlEvent::EndElement { name }
                    if name.local_name == "repository_skipped"
            )
        })
        .filter_map(|event| match event {
            ReaderXmlEvent::StartElement {
                name, attributes, ..
            } if name.local_name == "skipped" => Some(
                attributes
                    .into_iter()
                    .map(|attribute| {
                        (attribute.name.local_name, attribute.value)
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

fn parse_structure_files(xml: &[u8]) -> Vec<(Vec<String>, String)> {
    let events = parse_document(xml);
    let mut folders = Vec::new();
    let mut files = Vec::new();
    let structure = events
        .into_iter()
        .skip_while(|event| {
            !matches!(
                event,
                ReaderXmlEvent::StartElement { name, .. }
                    if name.local_name == "repository_structure"
            )
        })
        .skip(1)
        .take_while(|event| {
            !matches!(
                event,
                ReaderXmlEvent::EndElement { name }
                    if name.local_name == "repository_structure"
            )
        });
    for event in structure {
        match event {
            ReaderXmlEvent::StartElement {
                name, attributes, ..
            } if name.local_name == "folder" => {
                folders.push(reader_attribute(&attributes, "name"));
            }
            ReaderXmlEvent::StartElement {
                name, attributes, ..
            } if name.local_name == "file" => {
                files.push((
                    folders.clone(),
                    reader_attribute(&attributes, "path"),
                ));
            }
            ReaderXmlEvent::EndElement { name }
                if name.local_name == "folder" =>
            {
                folders.pop();
            }
            _ => {}
        }
    }
    files
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
        &[],
        temp_dir.path(),
        None,
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
    writer
        .write(XmlEvent::start_element("repository_files"))
        .unwrap();
    write_text_file_entry(&mut writer, path, content.len() as u64, content)
        .unwrap();
    writer.write(XmlEvent::end_element()).unwrap();
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
    writer
        .write(XmlEvent::start_element("repository_files"))
        .unwrap();
    write_read_error_file_entry(&mut writer, path, diagnostic).unwrap();
    writer.write(XmlEvent::end_element()).unwrap();
    writer.write(XmlEvent::end_element()).unwrap();
    writer.into_inner().into_inner()
}

#[path = "xml_output/destinations.rs"]
mod destinations;
#[path = "xml_output/repository_output.rs"]
mod repository_output;
#[path = "xml_output/semantic_equivalence.rs"]
mod semantic_equivalence;
#[path = "xml_output/serialization.rs"]
mod serialization;
