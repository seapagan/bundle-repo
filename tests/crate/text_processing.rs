use super::*;
use crate::embedded::{TokenizerFamily, get_tokenizer_json};
use crate::test_fixtures::{
    BIG5_BYTES, ENCODING_FIXTURES, EUC_JP_BYTES, GB18030_BYTES, GBK_BYTES,
    ISO_2022_JP_BYTES, JAPANESE, SHIFT_JIS_BYTES, UTF16_TEXT, UTF16LE_BYTES,
};
use sha2::{Digest, Sha256};

const LATE_NUL_BYTES: [u8; BINARY_PROBE_SIZE + 1] = {
    let mut bytes = [b'x'; BINARY_PROBE_SIZE + 1];
    bytes[BINARY_PROBE_SIZE] = 0;
    bytes
};

struct CountingReader {
    prefix: &'static [u8],
    total_len: usize,
    position: usize,
}

impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = buffer.len().min(self.total_len - self.position);
        for (offset, byte) in buffer[..count].iter_mut().enumerate() {
            *byte = self
                .prefix
                .get(self.position + offset)
                .copied()
                .unwrap_or(b'x');
        }
        self.position += count;
        Ok(count)
    }
}

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
fn test_valid_utf8_bom_preserves_bom_and_allocation() {
    let mut bytes = b"\xef\xbb\xbfvalid UTF-8 text".to_vec();
    bytes.shrink_to_fit();
    let pointer = bytes.as_ptr();
    let capacity = bytes.capacity();

    let decoded = text(process(bytes, true));

    assert_eq!(decoded.text, "\u{feff}valid UTF-8 text");
    assert_eq!(decoded.text.as_ptr(), pointer);
    assert_eq!(decoded.text.capacity(), capacity);
    assert_eq!(decoded.conversion, None);
    assert!(!decoded.utf8_had_replacements);
}

#[test]
fn test_utf8_bom_payload_binary_evidence_is_excluded() {
    for (payload, expected) in [
        (b"text\0text".as_slice(), BinaryReason::NullByte),
        (b"\x01\x02\x03ab".as_slice(), BinaryReason::ControlDensity),
    ] {
        let mut bytes = b"\xef\xbb\xbf".to_vec();
        bytes.extend_from_slice(payload);
        assert_eq!(process(bytes, true), ProcessedFile::Binary(expected));
    }

    let mut bytes = b"\xef\xbb\xbf".to_vec();
    bytes.extend_from_slice(b"\x89PNG\r\n\x1a\nrest");
    assert!(matches!(
        process(bytes, true),
        ProcessedFile::Binary(BinaryReason::RecognizedMagic(_))
    ));
}

#[test]
fn test_malformed_utf8_bom_reports_only_enabled_replacements() {
    let bytes = b"\xef\xbb\xbfmalformed \xff text".to_vec();

    let decoded = text(process(bytes.clone(), true));
    assert_eq!(decoded.text, "\u{feff}malformed \u{fffd} text");
    assert_eq!(decoded.conversion, None);
    assert!(decoded.utf8_had_replacements);

    let disabled = text(process(bytes, false));
    assert_eq!(disabled.text, "\u{feff}malformed \u{fffd} text");
    assert_eq!(disabled.conversion, None);
    assert!(!disabled.utf8_had_replacements);
}

#[test]
fn test_encoding_fixture_matrix_decodes_exact_text_and_labels() {
    for fixture in &ENCODING_FIXTURES[..8] {
        let decoded = text(process(fixture.bytes.to_vec(), true));
        assert_eq!(
            decoded.text, fixture.expected,
            "wrong text for {}",
            fixture.encoding
        );
        assert_eq!(
            decoded.conversion,
            Some(ConversionReport {
                source_encoding: fixture.encoding,
                had_replacements: false,
            })
        );
    }
}

#[test]
fn test_utf16_fixture_matrix_and_disabled_timing() {
    for fixture in &ENCODING_FIXTURES[8..] {
        let decoded = text(process(fixture.bytes.to_vec(), true));
        assert_eq!(decoded.text, UTF16_TEXT);
        assert_eq!(
            decoded.conversion,
            Some(ConversionReport {
                source_encoding: fixture.encoding,
                had_replacements: false,
            })
        );

        let mut timings = ProcessingTimings::default();
        let disabled =
            classify_and_decode(fixture.bytes.to_vec(), false, &mut timings);
        assert_eq!(
            disabled,
            ProcessedFile::Binary(BinaryReason::Utf16ConversionDisabled(
                fixture.encoding
            ))
        );
        assert_eq!(timings.transcoded_files, 0);
    }
}

#[test]
fn test_fixture_hashes_are_stable() {
    let cases = [
        (
            BIG5_BYTES,
            "193d7f0e99d3a5964ebf217e629efef1c707d2c83be8317d7ec4f81271b91602",
        ),
        (
            EUC_JP_BYTES,
            "91a37bc153ef380393e5c2cb8f52e793e593c5cd1e0d9b7de1cd20c151023d0f",
        ),
        (
            GB18030_BYTES,
            "9e595b6e63720df4393911617670f8c3136f82757fee328b7f550dc12ad95cd4",
        ),
        (
            GBK_BYTES,
            "7fd8f1bcec1064109b0511f69e94d0655a8894f8da29c60f45c03940936bc33e",
        ),
        (
            ISO_2022_JP_BYTES,
            "5e3a4177b42d3c7f2aaa7a5b48456d2bb0a16ca18acb7df2c902c45382b6888f",
        ),
        (
            SHIFT_JIS_BYTES,
            "3f5ea89b27d50f0978035ed513a81f359ba814ad39776fb402b762313d942dbf",
        ),
        (
            ENCODING_FIXTURES[9].bytes,
            "f6dbc0c548d420a3fa7ebdedfbe73c4275693e895ac5161c52d5126439dd79fd",
        ),
        (
            UTF16LE_BYTES,
            "89092b865c9a2447b8ea301b974459256ad938874750e44bcecea1ae6296576f",
        ),
        (
            ENCODING_FIXTURES[6].bytes,
            "bc18e2357afd2a20f107a1c5ac44e3dbc17c9d9a7710369b3e92a7d6bfb5bb95",
        ),
        (
            ENCODING_FIXTURES[7].bytes,
            "e4a57b8dc2af3b7865147af1ce6d9d0375d63ca9fa16200e52225b3eb116beb7",
        ),
    ];

    for (bytes, expected) in cases {
        let actual = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(actual, expected);
    }
}

#[test]
fn test_valid_utf8_is_not_reported_as_transcoded() {
    let mut timings = ProcessingTimings::default();
    let expected = "日本語 Русский العربية";
    let decoded = text(classify_and_decode(
        expected.as_bytes().to_vec(),
        true,
        &mut timings,
    ));

    assert_eq!(decoded.text.as_bytes(), expected.as_bytes());
    assert_eq!(decoded.conversion, None);
    assert_eq!(timings.transcoded_files, 0);
}

#[test]
fn test_iso_2022_jp_candidate_designators_are_recognized() {
    for designator in [b"\x1b$B".as_slice(), b"\x1b$@".as_slice()] {
        let classification = classify_bytes(designator);
        assert!(classification.iso_2022_jp_candidate);
    }

    for ordinary_escape in [
        b"\x1b(Bplain ASCII".as_slice(),
        b"\x1b(Iplain ASCII".as_slice(),
        b"\x1b(Jplain ASCII".as_slice(),
        b"\x1b[31mred\x1b[0m".as_slice(),
        b"text \x1bX text".as_slice(),
        b"truncated \x1b$(".as_slice(),
    ] {
        let classification = classify_bytes(ordinary_escape);
        assert!(!classification.iso_2022_jp_candidate);
    }
}

#[test]
fn test_ascii_designators_retain_owned_utf8_fast_path() {
    for expected in [
        "\u{1b}(Bplain ASCII",
        "日本語 before \u{1b}(I after",
        "Русский before \u{1b}(J after",
    ] {
        let mut bytes = expected.as_bytes().to_vec();
        bytes.shrink_to_fit();
        let pointer = bytes.as_ptr();
        let capacity = bytes.capacity();

        let decoded = text(process(bytes, true));

        assert_eq!(decoded.text.as_bytes(), expected.as_bytes());
        assert_eq!(decoded.text.as_ptr(), pointer);
        assert_eq!(decoded.text.capacity(), capacity);
        assert_eq!(decoded.conversion, None);
    }
}

#[test]
fn test_iso_2022_jp_fixture_still_decodes_after_narrow_probe() {
    let bytes = ISO_2022_JP_BYTES;

    let decoded = text(process(bytes.to_vec(), true));

    assert_eq!(decoded.text, JAPANESE);
    assert_eq!(
        decoded.conversion,
        Some(ConversionReport {
            source_encoding: "ISO-2022-JP",
            had_replacements: false,
        })
    );
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
    assert_eq!(timings.transcoded_files, 0);
}

#[test]
fn test_iso_2022_jp_is_not_detected_when_conversion_is_disabled() {
    let bytes = ISO_2022_JP_BYTES;
    let mut timings = ProcessingTimings::default();

    let decoded =
        text(classify_and_decode(bytes.to_vec(), false, &mut timings));

    assert_eq!(decoded.text.as_bytes(), bytes);
    assert_eq!(decoded.conversion, None);
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
fn test_recognized_large_binary_reads_only_probe() {
    let mut reader = CountingReader {
        prefix: b"\x89PNG\r\n\x1a\n",
        total_len: BINARY_PROBE_SIZE * 4,
        position: 0,
    };

    let processed = read_classify_and_decode_from_reader(
        &mut reader,
        true,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert!(matches!(
        processed,
        ProcessedFile::Binary(BinaryReason::RecognizedMagic("image/png"))
    ));
    assert_eq!(reader.position, BINARY_PROBE_SIZE);
}

#[test]
fn test_unrecognized_large_binary_with_probe_nul_reads_only_probe() {
    let mut reader = CountingReader {
        prefix: b"unrecognized\0binary",
        total_len: BINARY_PROBE_SIZE * 4,
        position: 0,
    };

    let processed = read_classify_and_decode_from_reader(
        &mut reader,
        true,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert_eq!(processed, ProcessedFile::Binary(BinaryReason::NullByte));
    assert_eq!(reader.position, BINARY_PROBE_SIZE);
}

#[test]
fn test_utf8_bom_recognized_large_binary_reads_only_probe() {
    let mut reader = CountingReader {
        prefix: b"\xef\xbb\xbf\x89PNG\r\n\x1a\n",
        total_len: BINARY_PROBE_SIZE * 4,
        position: 0,
    };

    let processed = read_classify_and_decode_from_reader(
        &mut reader,
        true,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert!(matches!(
        processed,
        ProcessedFile::Binary(BinaryReason::RecognizedMagic("image/png"))
    ));
    assert_eq!(reader.position, BINARY_PROBE_SIZE);
}

#[test]
fn test_utf8_bom_large_binary_with_payload_nul_reads_only_probe() {
    let mut reader = CountingReader {
        prefix: b"\xef\xbb\xbfunrecognized\0binary",
        total_len: BINARY_PROBE_SIZE * 4,
        position: 0,
    };

    let processed = read_classify_and_decode_from_reader(
        &mut reader,
        true,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert_eq!(processed, ProcessedFile::Binary(BinaryReason::NullByte));
    assert_eq!(reader.position, BINARY_PROBE_SIZE);
}

#[test]
fn test_nul_after_probe_is_fully_read_before_classification() {
    let total_len = BINARY_PROBE_SIZE * 4;
    let mut reader = CountingReader {
        prefix: &LATE_NUL_BYTES,
        total_len,
        position: 0,
    };

    let processed = read_classify_and_decode_from_reader(
        &mut reader,
        true,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert_eq!(processed, ProcessedFile::Binary(BinaryReason::NullByte));
    assert_eq!(reader.position, total_len);
}

#[test]
fn test_utf16_bom_reads_only_probe_when_conversion_is_disabled() {
    for (prefix, encoding_name) in [
        (b"\xff\xfe".as_slice(), "UTF-16LE"),
        (b"\xfe\xff".as_slice(), "UTF-16BE"),
    ] {
        let mut reader = CountingReader {
            prefix,
            total_len: BINARY_PROBE_SIZE * 4,
            position: 0,
        };

        let processed = read_classify_and_decode_from_reader(
            &mut reader,
            false,
            &mut ProcessingTimings::default(),
        )
        .unwrap();

        assert_eq!(
            processed,
            ProcessedFile::Binary(BinaryReason::Utf16ConversionDisabled(
                encoding_name,
            ))
        );
        assert_eq!(reader.position, BINARY_PROBE_SIZE);
    }
}

#[test]
fn test_utf16_bom_is_fully_read_and_decoded_when_enabled() {
    let total_len = BINARY_PROBE_SIZE * 4;
    for (prefix, encoding_name) in [
        (b"\xff\xfe".as_slice(), "UTF-16LE"),
        (b"\xfe\xff".as_slice(), "UTF-16BE"),
    ] {
        let mut reader = CountingReader {
            prefix,
            total_len,
            position: 0,
        };

        let decoded = text(
            read_classify_and_decode_from_reader(
                &mut reader,
                true,
                &mut ProcessingTimings::default(),
            )
            .unwrap(),
        );

        assert_eq!(reader.position, total_len);
        assert_eq!(decoded.text.chars().count(), (total_len - 2) / 2);
        assert!(
            decoded
                .text
                .chars()
                .all(|character| character == '\u{7878}')
        );
        assert_eq!(
            decoded.conversion,
            Some(ConversionReport {
                source_encoding: encoding_name,
                had_replacements: false,
            })
        );
    }
}

#[test]
fn test_utf16_probe_nul_is_not_short_circuited_when_enabled() {
    let total_len = BINARY_PROBE_SIZE * 4;
    let mut reader = CountingReader {
        prefix: b"\xff\xfeA\0",
        total_len,
        position: 0,
    };

    let processed = read_classify_and_decode_from_reader(
        &mut reader,
        true,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert!(matches!(processed, ProcessedFile::Text(_)));
    assert_eq!(reader.position, total_len);
}

#[test]
fn test_utf8_bom_text_is_fully_read_and_preserved() {
    let total_len = BINARY_PROBE_SIZE * 4;
    let mut reader = CountingReader {
        prefix: b"\xef\xbb\xbfhello",
        total_len,
        position: 0,
    };

    let decoded = text(
        read_classify_and_decode_from_reader(
            &mut reader,
            true,
            &mut ProcessingTimings::default(),
        )
        .unwrap(),
    );
    let expected = format!("\u{feff}hello{}", "x".repeat(total_len - 8));

    assert_eq!(reader.position, total_len);
    assert_eq!(decoded.text, expected);
    assert_eq!(decoded.conversion, None);
    assert!(!decoded.utf8_had_replacements);
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
        (SHIFT_JIS_BYTES, "Shift_JIS"),
        (EUC_JP_BYTES, "EUC-JP"),
        (GBK_BYTES, "GBK"),
        (GB18030_BYTES, "GBK"),
        (BIG5_BYTES, "Big5"),
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
    let mut malformed = ISO_2022_JP_BYTES.to_vec();
    assert_eq!(malformed.pop(), Some(b'\n'));
    malformed.pop();
    malformed.shrink_to_fit();
    let expected = malformed.clone();
    let pointer = malformed.as_ptr();
    let capacity = malformed.capacity();

    let decoded = text(process(malformed, true));

    assert_eq!(decoded.text.as_bytes(), expected);
    assert_eq!(decoded.text.as_ptr(), pointer);
    assert_eq!(decoded.text.capacity(), capacity);
    assert_eq!(decoded.conversion, None);
}

#[test]
fn test_truncated_utf16_reports_replacement_and_disabled_stays_binary() {
    let mut malformed = UTF16LE_BYTES.to_vec();
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
fn test_post_decode_plausibility_rejects_control_density() {
    assert_eq!(text_binary_reason("line\nwith\ttabs\r"), None);
    assert_eq!(
        text_binary_reason("\u{0001}\u{0002}\u{0003}ab"),
        Some(BinaryReason::ControlDensity)
    );
    assert_eq!(
        text_binary_reason("text\0text"),
        Some(BinaryReason::NullByte)
    );
    assert_eq!(
        text_binary_reason("\u{0081}\u{008d}\u{008f}ab"),
        Some(BinaryReason::ControlDensity)
    );
    assert_eq!(
        process(vec![0x81, b' ', 0x8d, b' ', 0x8f, b' '], true),
        ProcessedFile::Binary(BinaryReason::ImplausibleDecodedData)
    );
}

#[test]
fn test_windows_1252_punctuation_remains_plausible_text() {
    let decoded = text(process(
        b"\x93Quoted prose\x94 with an \x96 dash.".to_vec(),
        true,
    ));

    assert_eq!(decoded.text, "“Quoted prose” with an – dash.");
    assert_eq!(
        decoded.conversion,
        Some(ConversionReport {
            source_encoding: "windows-1252",
            had_replacements: false,
        })
    );
}
