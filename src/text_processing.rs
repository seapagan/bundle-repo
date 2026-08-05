use crate::timings::ProcessingTimings;
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, ISO_2022_JP, UTF_16BE, UTF_16LE, UTF_8};
use std::fs;
use std::io;
use std::path::Path;
use std::time::Instant;

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
    let read_start = Instant::now();
    let bytes = fs::read(path)?;
    timings.file_classification_and_read += read_start.elapsed();
    Ok(classify_and_decode(bytes, utf8, timings))
}

fn classify_and_decode(
    bytes: Vec<u8>,
    utf8: bool,
    timings: &mut ProcessingTimings,
) -> ProcessedFile {
    let classification_start = Instant::now();
    if let Some((encoding, _)) = Encoding::for_bom(&bytes) {
        timings.file_classification_and_read += classification_start.elapsed();
        if encoding == UTF_16LE || encoding == UTF_16BE {
            return process_utf16(bytes, encoding, utf8, timings);
        }
        debug_assert_eq!(encoding, UTF_8);
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
    timings.transcode_time += elapsed;
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
            })
        }
        Err(error) => {
            let bytes = error.into_bytes();
            let text = if utf8 {
                UTF_8.decode_without_bom_handling(&bytes).0.into_owned()
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            };
            timings.utf8_validation_or_transcode += validation_start.elapsed();
            ProcessedFile::Text(DecodedText {
                text,
                conversion: None,
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
    timings.transcode_time += elapsed;
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

    let decode_start = Instant::now();
    let (decoded, had_replacements) =
        encoding.decode_without_bom_handling(&bytes);
    let text = decoded.into_owned();
    let decode_elapsed = decode_start.elapsed();
    timings.utf8_validation_or_transcode += detection_start.elapsed();
    timings.transcode_time += decode_elapsed;
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
            && ((previous[1] == b'$' && matches!(byte, b'@' | b'B'))
                || (previous[1] == b'(' && matches!(byte, b'B' | b'I' | b'J')))
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
    exceeds_control_density(
        text.chars()
            .filter(|character| is_disallowed_char(*character))
            .count(),
        text.chars().count(),
    )
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
    matches!(character, '\u{0001}'..='\u{0008}' | '\u{000b}' | '\u{000c}' | '\u{000e}'..='\u{001f}' | '\u{007f}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded::{get_tokenizer_json, TokenizerFamily};
    use sha2::{Digest, Sha256};

    const JAPANESE: &str = "これは文字コード検出のための日本語の文章です。複数の文を含めて、短い入力による誤判定を避けます。古い文書も正しく読み取ります。\n";
    const SIMPLIFIED_CHINESE: &str = "这是用于字符编码检测的中文文本。它包含多个自然句子，以避免短输入造成误判。旧文件也应该被正确读取。\n";
    const GB18030_CHINESE: &str = "这是用于字符编码检测的中文文本。它包含多个自然句子，以避免短输入造成误判。扩展字符𠀀用于验证四字节编码。\n";
    const TRADITIONAL_CHINESE: &str = "這是用於字元編碼偵測的中文文字。它包含多個自然句子，以避免短輸入造成誤判。舊檔案也應該被正確讀取。\n";
    const RUSSIAN: &str = "Это русский текст для проверки определения кодировки. Он содержит несколько естественных предложений. Старые файлы должны читаться правильно.\n";
    const WESTERN: &str = "Voici un texte français pour vérifier la détection d’encodage. Il contient plusieurs phrases naturelles. Les fichiers anciens doivent être lus correctement.\n";
    const UTF16_TEXT: &str = "UTF-16 text with 日本語, русский текст, and العربية. This fixture contains multiple natural sentences. It verifies byte-order-mark handling.\n";

    fn process(bytes: Vec<u8>, utf8: bool) -> ProcessedFile {
        classify_and_decode(bytes, utf8, &mut ProcessingTimings::default())
    }

    fn text(result: ProcessedFile) -> DecodedText {
        match result {
            ProcessedFile::Text(text) => text,
            ProcessedFile::Binary(reason) => {
                panic!("expected text, got {reason:?}")
            }
        }
    }

    #[test]
    fn test_valid_utf8_reuses_original_allocation() {
        let mut bytes = "valid multilingual 日本語 Русский العربية"
            .repeat(1024)
            .into_bytes();
        bytes.shrink_to_fit();
        let pointer = bytes.as_ptr();
        let capacity = bytes.capacity();

        let decoded = text(process(bytes, true));

        assert_eq!(decoded.text.as_ptr(), pointer);
        assert_eq!(decoded.text.capacity(), capacity);
        assert_eq!(decoded.conversion, None);
    }

    #[test]
    fn test_embedded_tokenizer_reuses_original_allocation() {
        let embedded = get_tokenizer_json(TokenizerFamily::Glm5_2).unwrap();
        let mut bytes = embedded.data.as_ref().to_vec();
        bytes.shrink_to_fit();
        let pointer = bytes.as_ptr();
        let capacity = bytes.capacity();

        let decoded = text(process(bytes, true));

        assert_eq!(decoded.text.as_ptr(), pointer);
        assert_eq!(decoded.text.capacity(), capacity);
        assert_eq!(decoded.conversion, None);
    }

    #[test]
    fn test_multilingual_utf8_bytes_are_unchanged() {
        for expected in [
            "ASCII text",
            "日本語の文章です。",
            "这是一段中文。",
            "Это русский текст.",
            "هذا نص عربي.",
        ] {
            let decoded = text(process(expected.as_bytes().to_vec(), true));
            assert_eq!(decoded.text.as_bytes(), expected.as_bytes());
            assert_eq!(decoded.conversion, None);
        }
    }

    #[test]
    fn test_utf16_bom_is_decoded_only_when_enabled() {
        let bytes = vec![0xff, 0xfe, b'H', 0, b'i', 0];
        let decoded = text(process(bytes.clone(), true));
        assert_eq!(decoded.text, "Hi");
        assert_eq!(
            decoded.conversion,
            Some(ConversionReport {
                source_encoding: "UTF-16LE",
                had_replacements: false,
            })
        );
        assert_eq!(
            process(bytes, false),
            ProcessedFile::Binary(BinaryReason::Utf16ConversionDisabled(
                "UTF-16LE"
            ))
        );
    }

    #[test]
    fn test_encoding_fixture_matrix_decodes_exact_text_and_labels() {
        let cases = [
            (
                include_bytes!("../tests/fixtures/encodings/shift-jis.txt")
                    .as_slice(),
                JAPANESE,
                "Shift_JIS",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/euc-jp.txt")
                    .as_slice(),
                JAPANESE,
                "EUC-JP",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/iso-2022-jp.txt")
                    .as_slice(),
                JAPANESE,
                "ISO-2022-JP",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/gbk.txt")
                    .as_slice(),
                SIMPLIFIED_CHINESE,
                "GBK",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/gb18030.txt")
                    .as_slice(),
                GB18030_CHINESE,
                "GBK",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/big5.txt")
                    .as_slice(),
                TRADITIONAL_CHINESE,
                "Big5",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/windows-1251.txt")
                    .as_slice(),
                RUSSIAN,
                "windows-1251",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/windows-1252.txt")
                    .as_slice(),
                WESTERN,
                "windows-1252",
            ),
        ];

        for (bytes, expected, encoding) in cases {
            let decoded = text(process(bytes.to_vec(), true));
            assert_eq!(decoded.text, expected, "wrong text for {encoding}");
            assert_eq!(
                decoded.conversion,
                Some(ConversionReport {
                    source_encoding: encoding,
                    had_replacements: false,
                })
            );
        }
    }

    #[test]
    fn test_utf16_fixture_matrix_and_disabled_timing() {
        for (bytes, encoding) in [
            (
                include_bytes!("../tests/fixtures/encodings/utf-16le.txt")
                    .as_slice(),
                "UTF-16LE",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/utf-16be.txt")
                    .as_slice(),
                "UTF-16BE",
            ),
        ] {
            let decoded = text(process(bytes.to_vec(), true));
            assert_eq!(decoded.text, UTF16_TEXT);
            assert_eq!(
                decoded.conversion,
                Some(ConversionReport {
                    source_encoding: encoding,
                    had_replacements: false,
                })
            );

            let mut timings = ProcessingTimings::default();
            let disabled =
                classify_and_decode(bytes.to_vec(), false, &mut timings);
            assert_eq!(
                disabled,
                ProcessedFile::Binary(BinaryReason::Utf16ConversionDisabled(
                    encoding
                ))
            );
            assert!(timings.transcode_time.is_zero());
            assert_eq!(timings.transcoded_files, 0);
        }
    }

    #[test]
    fn test_fixture_hashes_are_stable() {
        let cases = [
            (include_bytes!("../tests/fixtures/encodings/big5.txt").as_slice(), "193d7f0e99d3a5964ebf217e629efef1c707d2c83be8317d7ec4f81271b91602"),
            (include_bytes!("../tests/fixtures/encodings/euc-jp.txt").as_slice(), "91a37bc153ef380393e5c2cb8f52e793e593c5cd1e0d9b7de1cd20c151023d0f"),
            (include_bytes!("../tests/fixtures/encodings/gb18030.txt").as_slice(), "9e595b6e63720df4393911617670f8c3136f82757fee328b7f550dc12ad95cd4"),
            (include_bytes!("../tests/fixtures/encodings/gbk.txt").as_slice(), "7fd8f1bcec1064109b0511f69e94d0655a8894f8da29c60f45c03940936bc33e"),
            (include_bytes!("../tests/fixtures/encodings/iso-2022-jp.txt").as_slice(), "5e3a4177b42d3c7f2aaa7a5b48456d2bb0a16ca18acb7df2c902c45382b6888f"),
            (include_bytes!("../tests/fixtures/encodings/shift-jis.txt").as_slice(), "3f5ea89b27d50f0978035ed513a81f359ba814ad39776fb402b762313d942dbf"),
            (include_bytes!("../tests/fixtures/encodings/utf-16be.txt").as_slice(), "f6dbc0c548d420a3fa7ebdedfbe73c4275693e895ac5161c52d5126439dd79fd"),
            (include_bytes!("../tests/fixtures/encodings/utf-16le.txt").as_slice(), "89092b865c9a2447b8ea301b974459256ad938874750e44bcecea1ae6296576f"),
            (include_bytes!("../tests/fixtures/encodings/windows-1251.txt").as_slice(), "bc18e2357afd2a20f107a1c5ac44e3dbc17c9d9a7710369b3e92a7d6bfb5bb95"),
            (include_bytes!("../tests/fixtures/encodings/windows-1252.txt").as_slice(), "e4a57b8dc2af3b7865147af1ce6d9d0375d63ca9fa16200e52225b3eb116beb7"),
        ];

        for (bytes, expected) in cases {
            assert_eq!(format!("{:x}", Sha256::digest(bytes)), expected);
        }
    }

    #[test]
    fn test_valid_utf8_has_zero_transcode_time() {
        let mut timings = ProcessingTimings::default();
        let result = classify_and_decode(
            "日本語 Русский العربية".as_bytes().to_vec(),
            true,
            &mut timings,
        );

        assert!(matches!(result, ProcessedFile::Text(_)));
        assert!(timings.transcode_time.is_zero());
        assert_eq!(timings.transcoded_files, 0);
    }

    #[test]
    fn test_iso_2022_jp_candidate_designators_are_recognized() {
        for designator in [
            b"\x1b$B".as_slice(),
            b"\x1b$@".as_slice(),
            b"\x1b(B".as_slice(),
            b"\x1b(J".as_slice(),
            b"\x1b(I".as_slice(),
        ] {
            let classification = classify_bytes(designator);
            assert!(classification.iso_2022_jp_candidate);
        }

        for ordinary_escape in [
            b"\x1b[31mred\x1b[0m".as_slice(),
            b"text \x1bX text".as_slice(),
            b"truncated \x1b$(".as_slice(),
        ] {
            let classification = classify_bytes(ordinary_escape);
            assert!(!classification.iso_2022_jp_candidate);
        }
    }

    #[test]
    fn test_ansi_escape_text_retains_owned_utf8_fast_path() {
        let mut bytes = b"plain \x1b[31mred\x1b[0m text".to_vec();
        bytes.shrink_to_fit();
        let expected = bytes.clone();
        let pointer = bytes.as_ptr();
        let capacity = bytes.capacity();
        let mut timings = ProcessingTimings::default();

        let decoded = text(classify_and_decode(bytes, true, &mut timings));

        assert_eq!(decoded.text.as_bytes(), expected);
        assert_eq!(decoded.text.as_ptr(), pointer);
        assert_eq!(decoded.text.capacity(), capacity);
        assert_eq!(decoded.conversion, None);
        assert!(timings.transcode_time.is_zero());
    }

    #[test]
    fn test_non_iso_detector_result_retains_owned_utf8_fast_path() {
        let expected = "café before \u{1b}(B after";
        let mut bytes = expected.as_bytes().to_vec();
        bytes.shrink_to_fit();
        let pointer = bytes.as_ptr();
        let capacity = bytes.capacity();

        let decoded = text(process(bytes, true));

        assert_eq!(decoded.text, expected);
        assert_eq!(decoded.text.as_ptr(), pointer);
        assert_eq!(decoded.text.capacity(), capacity);
        assert_eq!(decoded.conversion, None);
    }

    #[test]
    fn test_iso_2022_jp_is_not_detected_when_conversion_is_disabled() {
        let bytes =
            include_bytes!("../tests/fixtures/encodings/iso-2022-jp.txt");
        let mut timings = ProcessingTimings::default();

        let decoded =
            text(classify_and_decode(bytes.to_vec(), false, &mut timings));

        assert_eq!(decoded.text.as_bytes(), bytes);
        assert_eq!(decoded.conversion, None);
        assert!(timings.transcode_time.is_zero());
        assert_eq!(timings.transcoded_files, 0);
    }

    #[test]
    fn test_binary_evidence_excludes_unknown_and_magic_files() {
        assert_eq!(
            process(vec![b'a', 0, b'b'], true),
            ProcessedFile::Binary(BinaryReason::NullByte)
        );
        assert!(matches!(
            process(b"\x89PNG\r\n\x1a\nrest".to_vec(), true),
            ProcessedFile::Binary(BinaryReason::RecognizedMagic(_))
        ));
        assert_eq!(
            process(vec![1, 2, 3, b'a', b'b'], true),
            ProcessedFile::Binary(BinaryReason::ControlDensity)
        );

        for bytes in [
            b"%PDF-1.7\n".as_slice(),
            b"PK\x03\x04archive".as_slice(),
            b"MZ\x90\0executable".as_slice(),
        ] {
            assert!(matches!(
                process(bytes.to_vec(), true),
                ProcessedFile::Binary(BinaryReason::RecognizedMagic(_))
                    | ProcessedFile::Binary(BinaryReason::NullByte)
            ));
        }
    }

    #[test]
    fn test_high_bit_bytes_alone_are_not_binary() {
        assert!(matches!(
            process(vec![0x80, 0x81, 0x82, 0x83], false),
            ProcessedFile::Text(_)
        ));
    }

    #[test]
    fn test_truncated_multibyte_fixtures_report_selected_decoder_results() {
        let cases = [
            (
                include_bytes!("../tests/fixtures/encodings/shift-jis.txt")
                    .as_slice(),
                "Shift_JIS",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/euc-jp.txt")
                    .as_slice(),
                "EUC-JP",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/gbk.txt")
                    .as_slice(),
                "GBK",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/gb18030.txt")
                    .as_slice(),
                "GBK",
            ),
            (
                include_bytes!("../tests/fixtures/encodings/big5.txt")
                    .as_slice(),
                "Big5",
            ),
        ];

        for (fixture, source_encoding) in cases {
            let mut malformed = fixture.to_vec();
            assert_eq!(malformed.pop(), Some(b'\n'));
            malformed.pop();

            let decoded = text(process(malformed, true));
            let report = decoded.conversion.unwrap();
            assert_eq!(
                report.source_encoding, "windows-1252",
                "source fixture was {source_encoding}"
            );
            assert!(!report.had_replacements, "{source_encoding}");
            assert!(!decoded.text.contains('\u{fffd}'), "{source_encoding}");
        }
    }

    #[test]
    fn test_truncated_iso_2022_jp_uses_normal_utf8_path_when_not_selected() {
        let mut malformed =
            include_bytes!("../tests/fixtures/encodings/iso-2022-jp.txt")
                .to_vec();
        assert_eq!(malformed.pop(), Some(b'\n'));
        malformed.pop();
        let expected = malformed.clone();

        let decoded = text(process(malformed, true));

        assert_eq!(decoded.text.as_bytes(), expected);
        assert_eq!(decoded.conversion, None);
    }

    #[test]
    fn test_truncated_utf16_reports_replacement_and_disabled_stays_binary() {
        let mut malformed =
            include_bytes!("../tests/fixtures/encodings/utf-16le.txt")
                .to_vec();
        malformed.pop();

        let decoded = text(process(malformed.clone(), true));
        assert!(decoded.text.ends_with('\u{fffd}'));
        assert_eq!(
            decoded.conversion,
            Some(ConversionReport {
                source_encoding: "UTF-16LE",
                had_replacements: true,
            })
        );
        assert_eq!(
            process(malformed, false),
            ProcessedFile::Binary(BinaryReason::Utf16ConversionDisabled(
                "UTF-16LE"
            ))
        );
    }

    #[test]
    fn test_empty_bom_only_and_short_ambiguous_inputs() {
        assert_eq!(text(process(Vec::new(), true)).text, "");
        assert_eq!(
            text(process(b"\xef\xbb\xbf".to_vec(), true)).text,
            "\u{feff}"
        );
        assert_eq!(text(process(b"\xff\xfe".to_vec(), true)).text, "");

        let decoded = text(process(b"\x93Hi\x94".to_vec(), true));
        assert_eq!(decoded.text, "“Hi”");
        assert_eq!(
            decoded.conversion,
            Some(ConversionReport {
                source_encoding: "windows-1252",
                had_replacements: false,
            })
        );

        assert_eq!(
            process(vec![1, 2, 3, 0x93, b'A'], true),
            ProcessedFile::Binary(BinaryReason::ControlDensity)
        );
    }

    #[test]
    fn test_post_decode_plausibility_uses_same_control_policy() {
        assert_eq!(text_binary_reason("line\nwith\ttabs\r"), None);
        assert_eq!(
            text_binary_reason("\u{0001}\u{0002}\u{0003}ab"),
            Some(BinaryReason::ControlDensity)
        );
        assert_eq!(
            text_binary_reason("text\0text"),
            Some(BinaryReason::NullByte)
        );
    }
}
