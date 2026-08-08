use super::*;

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
