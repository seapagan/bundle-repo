use crate::timings::ProcessingTimings;
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, ISO_2022_JP, UTF_8, UTF_16BE, UTF_16LE};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::time::Instant;

const BINARY_PROBE_SIZE: usize = 8 * 1024;
const DISALLOWED_CONTROL_DENSITY: f64 = 0.30;

struct ByteClassification {
    binary_reason: Option<BinaryReason>,
    iso_2022_jp_candidate: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ProcessedFile {
    Text(DecodedText),
    Binary(BinaryReason),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DecodedText {
    pub(crate) text: String,
    pub(crate) conversion: Option<ConversionReport>,
    pub(crate) utf8_had_replacements: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ConversionReport {
    pub(crate) source_encoding: &'static str,
    pub(crate) had_replacements: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum BinaryReason {
    RecognizedMagic(&'static str),
    NullByte,
    ControlDensity,
    Utf16ConversionDisabled(&'static str),
    ImplausibleDecodedData,
}

pub(crate) fn read_classify_and_decode(
    path: &Path,
    utf8: bool,
    timings: &mut ProcessingTimings,
) -> io::Result<ProcessedFile> {
    let open_start = Instant::now();
    let mut file = File::open(path)?;
    timings.file_classification_and_read += open_start.elapsed();
    read_classify_and_decode_from_reader(&mut file, utf8, timings)
}

fn read_classify_and_decode_from_reader<R: Read>(
    reader: &mut R,
    utf8: bool,
    timings: &mut ProcessingTimings,
) -> io::Result<ProcessedFile> {
    let mut bytes = Vec::with_capacity(BINARY_PROBE_SIZE);
    let read_start = Instant::now();
    reader
        .by_ref()
        .take(BINARY_PROBE_SIZE as u64)
        .read_to_end(&mut bytes)?;
    timings.file_classification_and_read += read_start.elapsed();

    let classification_start = Instant::now();
    let early_binary_reason = match Encoding::for_bom(&bytes) {
        None => bytes
            .contains(&0)
            .then_some(BinaryReason::NullByte)
            .or_else(|| magic_binary_reason(&bytes)),
        Some((encoding, bom_length)) if encoding == UTF_8 => {
            let payload = &bytes[bom_length..];
            payload
                .contains(&0)
                .then_some(BinaryReason::NullByte)
                .or_else(|| magic_binary_reason(payload))
        }
        Some((encoding, _))
            if !utf8 && (encoding == UTF_16LE || encoding == UTF_16BE) =>
        {
            Some(BinaryReason::Utf16ConversionDisabled(encoding.name()))
        }
        Some(_) => None,
    };
    if let Some(reason) = early_binary_reason {
        timings.file_classification_and_read += classification_start.elapsed();
        return Ok(ProcessedFile::Binary(reason));
    }
    timings.file_classification_and_read += classification_start.elapsed();

    let read_start = Instant::now();
    reader.read_to_end(&mut bytes)?;
    timings.file_classification_and_read += read_start.elapsed();
    Ok(classify_and_decode(bytes, utf8, timings))
}

fn classify_and_decode(
    bytes: Vec<u8>,
    utf8: bool,
    timings: &mut ProcessingTimings,
) -> ProcessedFile {
    let classification_start = Instant::now();
    if let Some((encoding, bom_length)) = Encoding::for_bom(&bytes) {
        if encoding == UTF_16LE || encoding == UTF_16BE {
            timings.file_classification_and_read +=
                classification_start.elapsed();
            return process_utf16(bytes, encoding, utf8, timings);
        }
        debug_assert_eq!(encoding, UTF_8);
        let payload = &bytes[bom_length..];
        if let Some(reason) = magic_binary_reason(payload) {
            timings.file_classification_and_read +=
                classification_start.elapsed();
            return ProcessedFile::Binary(reason);
        }
        if let Some(reason) = classify_bytes(payload).binary_reason {
            timings.file_classification_and_read +=
                classification_start.elapsed();
            return ProcessedFile::Binary(reason);
        }
        timings.file_classification_and_read += classification_start.elapsed();
        return process_bom_marked_utf8(bytes, utf8, timings);
    }

    if let Some(reason) = magic_binary_reason(&bytes) {
        timings.file_classification_and_read += classification_start.elapsed();
        return ProcessedFile::Binary(reason);
    }
    let classification = classify_bytes(&bytes);
    if let Some(reason) = classification.binary_reason {
        timings.file_classification_and_read += classification_start.elapsed();
        return ProcessedFile::Binary(reason);
    }
    timings.file_classification_and_read += classification_start.elapsed();

    let bytes = if utf8 && classification.iso_2022_jp_candidate {
        match try_transcode_iso_2022_jp(bytes, timings) {
            Ok(processed) => return processed,
            Err(bytes) => bytes,
        }
    } else {
        bytes
    };

    let validation_start = Instant::now();
    match String::from_utf8(bytes) {
        Ok(text) => {
            timings.utf8_validation_or_transcode += validation_start.elapsed();
            timings.valid_files += 1;
            timings.valid_bytes += text.len() as u64;
            ProcessedFile::Text(DecodedText {
                text,
                conversion: None,
                utf8_had_replacements: false,
            })
        }
        Err(error) => {
            let bytes = error.into_bytes();
            if utf8 {
                transcode_legacy(bytes, validation_start, timings)
            } else {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                timings.utf8_validation_or_transcode +=
                    validation_start.elapsed();
                ProcessedFile::Text(DecodedText {
                    text,
                    conversion: None,
                    utf8_had_replacements: false,
                })
            }
        }
    }
}

fn process_utf16(
    bytes: Vec<u8>,
    encoding: &'static Encoding,
    utf8: bool,
    timings: &mut ProcessingTimings,
) -> ProcessedFile {
    if !utf8 {
        return ProcessedFile::Binary(BinaryReason::Utf16ConversionDisabled(
            encoding.name(),
        ));
    }

    let validation_start = Instant::now();
    let (decoded, had_replacements) = encoding.decode_with_bom_removal(&bytes);
    let text = decoded.into_owned();
    let elapsed = validation_start.elapsed();
    timings.utf8_validation_or_transcode += elapsed;
    if text_binary_reason(&text).is_some() {
        return ProcessedFile::Binary(BinaryReason::ImplausibleDecodedData);
    }
    timings.transcoded_files += 1;
    timings.transcoded_bytes += bytes.len() as u64;
    ProcessedFile::Text(DecodedText {
        text,
        conversion: Some(ConversionReport {
            source_encoding: encoding.name(),
            had_replacements,
        }),
        utf8_had_replacements: false,
    })
}

fn process_bom_marked_utf8(
    bytes: Vec<u8>,
    utf8: bool,
    timings: &mut ProcessingTimings,
) -> ProcessedFile {
    let validation_start = Instant::now();
    match String::from_utf8(bytes) {
        Ok(text) => {
            timings.utf8_validation_or_transcode += validation_start.elapsed();
            timings.valid_files += 1;
            timings.valid_bytes += text.len() as u64;
            ProcessedFile::Text(DecodedText {
                text,
                conversion: None,
                utf8_had_replacements: false,
            })
        }
        Err(error) => {
            let bytes = error.into_bytes();
            let (text, utf8_had_replacements) = if utf8 {
                let (decoded, had_errors) =
                    UTF_8.decode_without_bom_handling(&bytes);
                (decoded.into_owned(), had_errors)
            } else {
                (String::from_utf8_lossy(&bytes).into_owned(), false)
            };
            timings.utf8_validation_or_transcode += validation_start.elapsed();
            ProcessedFile::Text(DecodedText {
                text,
                conversion: None,
                utf8_had_replacements,
            })
        }
    }
}

fn transcode_legacy(
    bytes: Vec<u8>,
    validation_start: Instant,
    timings: &mut ProcessingTimings,
) -> ProcessedFile {
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    detector.feed(&bytes, true);
    let encoding = detector.guess(None, Utf8Detection::Deny);
    let (decoded, had_replacements) =
        encoding.decode_without_bom_handling(&bytes);
    let text = decoded.into_owned();
    let elapsed = validation_start.elapsed();
    timings.utf8_validation_or_transcode += elapsed;
    if text_binary_reason(&text).is_some() {
        return ProcessedFile::Binary(BinaryReason::ImplausibleDecodedData);
    }
    timings.transcoded_files += 1;
    timings.transcoded_bytes += bytes.len() as u64;
    ProcessedFile::Text(DecodedText {
        text,
        conversion: Some(ConversionReport {
            source_encoding: encoding.name(),
            had_replacements,
        }),
        utf8_had_replacements: false,
    })
}

fn try_transcode_iso_2022_jp(
    bytes: Vec<u8>,
    timings: &mut ProcessingTimings,
) -> Result<ProcessedFile, Vec<u8>> {
    let detection_start = Instant::now();
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    detector.feed(&bytes, true);
    let encoding = detector.guess(None, Utf8Detection::Deny);
    if encoding != ISO_2022_JP {
        timings.utf8_validation_or_transcode += detection_start.elapsed();
        return Err(bytes);
    }

    let (decoded, had_replacements) =
        encoding.decode_without_bom_handling(&bytes);
    let text = decoded.into_owned();
    timings.utf8_validation_or_transcode += detection_start.elapsed();
    if text_binary_reason(&text).is_some() {
        return Ok(ProcessedFile::Binary(
            BinaryReason::ImplausibleDecodedData,
        ));
    }
    timings.transcoded_files += 1;
    timings.transcoded_bytes += bytes.len() as u64;
    Ok(ProcessedFile::Text(DecodedText {
        text,
        conversion: Some(ConversionReport {
            source_encoding: encoding.name(),
            had_replacements,
        }),
        utf8_had_replacements: false,
    }))
}

fn magic_binary_reason(bytes: &[u8]) -> Option<BinaryReason> {
    infer::get(bytes).and_then(|kind| {
        (!kind.mime_type().starts_with("text/"))
            .then(|| BinaryReason::RecognizedMagic(kind.mime_type()))
    })
}

fn classify_bytes(bytes: &[u8]) -> ByteClassification {
    let mut disallowed = 0;
    let mut iso_2022_jp_candidate = false;
    let mut previous = [0; 2];

    for (index, &byte) in bytes.iter().enumerate() {
        if byte == 0 {
            return ByteClassification {
                binary_reason: Some(BinaryReason::NullByte),
                iso_2022_jp_candidate,
            };
        }
        disallowed += usize::from(is_disallowed_byte(byte));
        if index >= 2
            && previous[0] == 0x1b
            && previous[1] == b'$'
            && matches!(byte, b'@' | b'B')
        {
            iso_2022_jp_candidate = true;
        }
        previous = [previous[1], byte];
    }

    ByteClassification {
        binary_reason: exceeds_control_density(disallowed, bytes.len())
            .then_some(BinaryReason::ControlDensity),
        iso_2022_jp_candidate,
    }
}

fn text_binary_reason(text: &str) -> Option<BinaryReason> {
    if text.contains('\0') {
        return Some(BinaryReason::NullByte);
    }
    let (disallowed, total) =
        text.chars().fold((0, 0), |(disallowed, total), character| {
            (
                disallowed + usize::from(is_disallowed_char(character)),
                total + 1,
            )
        });
    exceeds_control_density(disallowed, total)
        .then_some(BinaryReason::ControlDensity)
}

fn exceeds_control_density(disallowed: usize, total: usize) -> bool {
    total != 0
        && (disallowed as f64 / total as f64) > DISALLOWED_CONTROL_DENSITY
}

fn is_disallowed_byte(byte: u8) -> bool {
    matches!(byte, 0x01..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f)
}

fn is_disallowed_char(character: char) -> bool {
    matches!(character, '\u{0001}'..='\u{0008}' | '\u{000b}' | '\u{000c}' | '\u{000e}'..='\u{001f}' | '\u{007f}' | '\u{0080}'..='\u{009f}')
}

#[cfg(test)]
#[path = "../tests/crate/text_processing.rs"]
mod tests;
