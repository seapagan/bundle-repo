use super::*;

const STRUCTURE_START: &[u8] = b"<repository_structure>";
const STRUCTURE_END: &[u8] = b"</repository_structure>";
const BASELINE_XML_ENV: &str = "BUNDLEREPO_BASELINE_XML";
const OPTIMIZED_XML_ENV: &str = "BUNDLEREPO_OPTIMIZED_XML";

#[derive(Debug, Eq, PartialEq)]
enum ProtectedOutputDifference {
    DeterministicBytes,
    InvalidStructureShape,
    MalformedStructure,
    RepositoryStructure,
    StructureNesting,
    UnsupportedStructureEvent,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    text: Vec<String>,
    children: Vec<Element>,
}

struct StructureParts<'a> {
    prefix: &'a [u8],
    structure: &'a [u8],
    suffix: &'a [u8],
}

fn semantic_protected_output_equivalence(
    baseline: &[u8],
    optimized: &[u8],
) -> Result<(), ProtectedOutputDifference> {
    let baseline = split_structure(baseline)?;
    let optimized = split_structure(optimized)?;

    if baseline.prefix != optimized.prefix
        || baseline.suffix != optimized.suffix
    {
        return Err(ProtectedOutputDifference::DeterministicBytes);
    }

    let baseline_tree = canonical_structure(baseline.structure)?;
    let optimized_tree = canonical_structure(optimized.structure)?;
    if baseline_tree != optimized_tree {
        return Err(ProtectedOutputDifference::RepositoryStructure);
    }
    Ok(())
}

fn split_structure(
    xml: &[u8],
) -> Result<StructureParts<'_>, ProtectedOutputDifference> {
    let start = offset(xml, STRUCTURE_START)?;
    let end_start = start + offset(&xml[start..], STRUCTURE_END)?;
    let end = end_start + STRUCTURE_END.len();
    Ok(StructureParts {
        prefix: &xml[..start],
        structure: &xml[start..end],
        suffix: &xml[end..],
    })
}

fn offset(
    haystack: &[u8],
    needle: &[u8],
) -> Result<usize, ProtectedOutputDifference> {
    haystack
        .windows(needle.len())
        .position(|bytes| bytes == needle)
        .ok_or(ProtectedOutputDifference::MalformedStructure)
}

fn canonical_structure(
    xml: &[u8],
) -> Result<Element, ProtectedOutputDifference> {
    let mut root = parse_element(xml)?;
    normalize_folder_siblings(&mut root)
        .map_err(|_| ProtectedOutputDifference::InvalidStructureShape)?;
    Ok(root)
}

fn parse_element(xml: &[u8]) -> Result<Element, ProtectedOutputDifference> {
    let mut stack = Vec::new();
    let mut root = None;
    for event in ParserConfig::new().create_reader(xml) {
        let event = event
            .map_err(|_| ProtectedOutputDifference::MalformedStructure)?;
        match event {
            ReaderXmlEvent::StartElement {
                name, attributes, ..
            } => stack.push(Element {
                name: name.local_name,
                attributes: attributes
                    .into_iter()
                    .map(|attribute| {
                        (attribute.name.local_name, attribute.value)
                    })
                    .collect(),
                text: Vec::new(),
                children: Vec::new(),
            }),
            ReaderXmlEvent::EndElement { name } => {
                finish_element(&mut stack, &mut root, &name.local_name)?;
            }
            ReaderXmlEvent::Characters(text) | ReaderXmlEvent::CData(text) => {
                stack
                    .last_mut()
                    .ok_or(ProtectedOutputDifference::StructureNesting)?
                    .text
                    .push(text)
            }
            ReaderXmlEvent::Whitespace(_)
            | ReaderXmlEvent::StartDocument { .. }
            | ReaderXmlEvent::EndDocument => {}
            _ => {
                return Err(
                    ProtectedOutputDifference::UnsupportedStructureEvent,
                );
            }
        }
    }
    if !stack.is_empty() {
        return Err(ProtectedOutputDifference::StructureNesting);
    }
    root.ok_or(ProtectedOutputDifference::StructureNesting)
}

fn finish_element(
    stack: &mut Vec<Element>,
    root: &mut Option<Element>,
    end_name: &str,
) -> Result<(), ProtectedOutputDifference> {
    let element = stack
        .pop()
        .ok_or(ProtectedOutputDifference::StructureNesting)?;
    if element.name != end_name {
        return Err(ProtectedOutputDifference::StructureNesting);
    }
    if let Some(parent) = stack.last_mut() {
        parent.children.push(element);
    } else if root.replace(element).is_some() {
        return Err(ProtectedOutputDifference::StructureNesting);
    }
    Ok(())
}

fn normalize_folder_siblings(
    element: &mut Element,
) -> Result<(), ProtectedOutputDifference> {
    for child in &mut element.children {
        normalize_folder_siblings(child)?;
    }
    match element.name.as_str() {
        "repository_structure" => normalize_structure_root(element),
        "folder" => sort_trailing_folders(&mut element.children),
        "file" | "summary" => Ok(()),
        _ => Err(ProtectedOutputDifference::MalformedStructure),
    }
}

fn normalize_structure_root(
    element: &mut Element,
) -> Result<(), ProtectedOutputDifference> {
    if element.attributes.is_empty()
        && element.text.is_empty()
        && matches!(element.children.first(), Some(child) if child.name == "summary")
    {
        sort_trailing_folders(&mut element.children[1..])
    } else {
        Err(ProtectedOutputDifference::MalformedStructure)
    }
}

fn sort_trailing_folders(
    children: &mut [Element],
) -> Result<(), ProtectedOutputDifference> {
    let first_folder = children
        .iter()
        .position(|child| child.name == "folder")
        .unwrap_or(children.len());
    if children[..first_folder]
        .iter()
        .any(|child| child.name != "file")
        || children[first_folder..]
            .iter()
            .any(|child| child.name != "folder")
    {
        return Err(ProtectedOutputDifference::MalformedStructure);
    }
    children[first_folder..].sort();
    Ok(())
}

fn fixture(structure: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><repository>\
         <file_summary><summary>fixed</summary></file_summary>\
         <repository_structure><summary>tree</summary>{structure}\
         </repository_structure>\
         <repository_skipped><skipped kind=\"file\" reason=\"secret-in-path\" \
         path=\"[REDACTED_PATH]\" secret-type=\"token\" />\
         </repository_skipped>\
         <repository_files><file path=\"canonical/one.rs\" size=\"17\">\
         protected-one</file></repository_files></repository>"
    )
    .into_bytes()
}

const ORDER_ONE: &str = "<folder name=\"alpha\"><file path=\"a.rs\" />\
    <file path=\"b.rs\" /></folder><folder name=\"beta\" />";
const ORDER_TWO: &str = "<folder name=\"beta\" /><folder name=\"alpha\">\
    <file path=\"a.rs\" /><file path=\"b.rs\" /></folder>";

#[test]
fn test_semantic_equivalence_ignores_only_folder_sibling_order() {
    let baseline = fixture(ORDER_ONE);
    let reordered = fixture(ORDER_TWO);

    assert_ne!(baseline, reordered);
    assert_eq!(
        semantic_protected_output_equivalence(&baseline, &reordered),
        Ok(())
    );
}

#[test]
fn test_semantic_equivalence_rejects_deterministic_output_changes() {
    let baseline = fixture(ORDER_ONE);
    for changed in [
        String::from_utf8(fixture(ORDER_ONE))
            .unwrap()
            .replace("protected-one", "protected-two"),
        String::from_utf8(fixture(ORDER_ONE))
            .unwrap()
            .replace("canonical/one.rs", "canonical/two.rs"),
        String::from_utf8(fixture(ORDER_ONE))
            .unwrap()
            .replace("secret-type=\"token\"", "secret-type=\"key\""),
        String::from_utf8(fixture(ORDER_ONE))
            .unwrap()
            .replace("<summary>fixed</summary>", "<summary>changed</summary>"),
    ] {
        assert_eq!(
            semantic_protected_output_equivalence(
                &baseline,
                changed.as_bytes()
            ),
            Err(ProtectedOutputDifference::DeterministicBytes)
        );
    }
}

#[test]
fn test_semantic_equivalence_rejects_structure_semantic_changes() {
    let baseline = fixture(ORDER_ONE);
    let changed_folder = fixture(&ORDER_ONE.replace("beta", "gamma"));
    let changed_file = fixture(&ORDER_ONE.replace("b.rs", "c.rs"));
    let changed_file_order = fixture(&ORDER_ONE.replace(
        "<file path=\"a.rs\" /><file path=\"b.rs\" />",
        "<file path=\"b.rs\" /><file path=\"a.rs\" />",
    ));

    for changed in [changed_folder, changed_file, changed_file_order] {
        assert_eq!(
            semantic_protected_output_equivalence(&baseline, &changed),
            Err(ProtectedOutputDifference::RepositoryStructure)
        );
    }
}

#[test]
#[ignore = "manual cross-process baseline comparison"]
fn test_external_protected_outputs_are_semantically_equivalent() {
    let baseline_path = std::env::var_os(BASELINE_XML_ENV)
        .expect("baseline XML environment variable is required");
    let optimized_path = std::env::var_os(OPTIMIZED_XML_ENV)
        .expect("optimized XML environment variable is required");
    let baseline =
        fs::read(baseline_path).expect("failed to read baseline XML");
    let optimized =
        fs::read(optimized_path).expect("failed to read optimized XML");

    assert_eq!(
        semantic_protected_output_equivalence(&baseline, &optimized),
        Ok(())
    );
}
