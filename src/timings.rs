use std::io::{self, Write};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct ProcessingTimings {
    pub(crate) tokenizer_load: Duration,
    pub(crate) file_classification_and_read: Duration,
    pub(crate) utf8_validation_or_transcode: Duration,
    pub(crate) xml_generation: Duration,
    pub(crate) token_count: Duration,
    pub(crate) compression: Duration,
    pub(crate) output_write_or_copy: Duration,
    pub(crate) valid_files: usize,
    pub(crate) valid_bytes: u64,
    pub(crate) transcoded_files: usize,
    pub(crate) transcoded_bytes: u64,
}

impl ProcessingTimings {
    pub(crate) fn enabled_from_env() -> bool {
        std::env::var_os("BUNDLEREPO_PHASE_TIMINGS")
            .is_some_and(|value| value == "1")
    }

    pub(crate) fn write_records<W: Write>(
        &self,
        sink: &mut W,
    ) -> io::Result<()> {
        self.write_duration(sink, "tokenizer_load", self.tokenizer_load)?;
        self.write_duration(
            sink,
            "file_classification_and_read",
            self.file_classification_and_read,
        )?;
        writeln!(
            sink,
            "BUNDLEREPO_TIMING phase=utf8_validation_or_transcode nanos={} valid_files={} valid_bytes={} transcoded_files={} transcoded_bytes={}",
            self.utf8_validation_or_transcode.as_nanos(),
            self.valid_files,
            self.valid_bytes,
            self.transcoded_files,
            self.transcoded_bytes,
        )?;
        self.write_duration(sink, "xml_generation", self.xml_generation)?;
        self.write_duration(sink, "token_count", self.token_count)?;
        self.write_duration(sink, "compression", self.compression)?;
        self.write_duration(
            sink,
            "output_write_or_copy",
            self.output_write_or_copy,
        )
    }

    fn write_duration<W: Write>(
        &self,
        sink: &mut W,
        phase: &str,
        duration: Duration,
    ) -> io::Result<()> {
        writeln!(
            sink,
            "BUNDLEREPO_TIMING phase={phase} nanos={}",
            duration.as_nanos()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_record_names_and_order() {
        let timings = ProcessingTimings::default();
        let mut output = Vec::new();

        timings.write_records(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        let phases = output
            .lines()
            .map(|line| {
                line.split_whitespace()
                    .nth(1)
                    .unwrap()
                    .strip_prefix("phase=")
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            phases,
            [
                "tokenizer_load",
                "file_classification_and_read",
                "utf8_validation_or_transcode",
                "xml_generation",
                "token_count",
                "compression",
                "output_write_or_copy",
            ]
        );
    }
}
