use super::*;

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
