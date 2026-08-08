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

#[path = "text_processing/decoding.rs"]
mod decoding;
#[path = "text_processing/edge_cases.rs"]
mod edge_cases;
#[path = "text_processing/probe_classification.rs"]
mod probe_classification;
