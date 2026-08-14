use crate::{presentation::Presentation, text_processing::ConversionReport};
use std::io::{self, Write};

pub(crate) struct ProgressReporter<N, D> {
    normal: N,
    diagnostic: D,
    quiet: bool,
    presentation: Presentation,
}

impl<N: Write, D: Write> ProgressReporter<N, D> {
    #[cfg(test)]
    pub(crate) fn new(normal: N, diagnostic: D, quiet: bool) -> Self {
        Self {
            normal,
            diagnostic,
            quiet,
            presentation: Presentation::plain(),
        }
    }

    pub(crate) fn phase(&mut self, message: &str) -> io::Result<()> {
        self.normal_line(&self.presentation.phase(message))
    }

    pub(crate) fn header(
        &mut self,
        version: &str,
        authors: &str,
        description: &str,
    ) -> io::Result<()> {
        self.normal_text(&self.presentation.header(
            version,
            authors,
            description,
        ))
    }

    pub(crate) fn phase_with_accent(
        &mut self,
        before: &str,
        accent: &str,
        after: &str,
    ) -> io::Result<()> {
        self.normal_line(
            &self.presentation.phase_with_accent(before, accent, after),
        )
    }

    pub(crate) fn success(&mut self, remainder: &str) -> io::Result<()> {
        self.normal_line(&self.presentation.success(remainder))
    }

    pub(crate) fn success_with_accent(
        &mut self,
        before: &str,
        accent: &str,
        after: &str,
    ) -> io::Result<()> {
        self.normal_line(
            &self.presentation.success_with_accent(before, accent, after),
        )
    }

    pub(crate) fn clone_success(
        &mut self,
        repository_url: &str,
        branch: Option<&str>,
    ) -> io::Result<()> {
        self.normal_line(
            &self.presentation.clone_success(repository_url, branch),
        )
    }

    pub(crate) fn repository_found(
        &mut self,
        path: &str,
        branch: &str,
    ) -> io::Result<()> {
        self.normal_line(&self.presentation.repository_found(path, branch))
    }

    pub(crate) fn summary(
        &mut self,
        table: &str,
        values: &[String],
    ) -> io::Result<()> {
        self.normal_text(&self.presentation.summary(table, values))
    }

    pub(crate) fn conversion(
        &mut self,
        path: &str,
        report: &ConversionReport,
    ) -> io::Result<()> {
        self.normal_line(
            &self.presentation.conversion(path, report.source_encoding),
        )?;
        if report.had_replacements {
            self.warning_rendered(
                &self
                    .presentation
                    .conversion_warning(path, report.source_encoding),
            )?;
        }
        Ok(())
    }

    pub(crate) fn malformed_utf8_replacement(
        &mut self,
        path: &str,
    ) -> io::Result<()> {
        self.warning_rendered(&self.presentation.malformed_utf8_warning(path))
    }

    pub(crate) fn normal_line(&mut self, message: &str) -> io::Result<()> {
        if !self.quiet {
            writeln!(self.normal, "{message}")?;
        }
        Ok(())
    }

    pub(crate) fn normal_text(&mut self, message: &str) -> io::Result<()> {
        if !self.quiet {
            write!(self.normal, "{message}")?;
        }
        Ok(())
    }

    pub(crate) fn warning_with_accent(
        &mut self,
        prefix: &str,
        before: &str,
        accent: &str,
        after: &str,
    ) -> io::Result<()> {
        self.warning_rendered(
            &self
                .presentation
                .warning_with_accent(prefix, before, accent, after),
        )
    }

    fn warning_rendered(&mut self, message: &str) -> io::Result<()> {
        if !self.quiet {
            writeln!(self.diagnostic, "{message}")?;
        }
        Ok(())
    }

    pub(crate) fn error(&mut self, message: &str) -> io::Result<()> {
        writeln!(self.diagnostic, "{}", self.presentation.error(message))
    }

    pub(crate) fn always_visible_diagnostic(
        &mut self,
        message: &str,
    ) -> io::Result<()> {
        writeln!(
            self.diagnostic,
            "{}",
            self.presentation.diagnostic_message(message)
        )
    }

    pub(crate) fn always_visible_error_with_accent(
        &mut self,
        prefix: &str,
        before: &str,
        accent: &str,
        after: &str,
    ) -> io::Result<()> {
        writeln!(
            self.diagnostic,
            "{}",
            self.presentation
                .error_with_accent(prefix, before, accent, after)
        )
    }

    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (N, D) {
        (self.normal, self.diagnostic)
    }
}

impl ProgressReporter<std::io::Stdout, std::io::Stderr> {
    pub(crate) fn terminal(quiet: bool) -> Self {
        Self {
            normal: std::io::stdout(),
            diagnostic: std::io::stderr(),
            quiet,
            presentation: Presentation::terminal(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion_uses_separate_normal_and_diagnostic_sinks() {
        let mut reporter =
            ProgressReporter::new(Vec::new(), Vec::new(), false);
        reporter.phase("Reading files and generating XML").unwrap();
        reporter
            .conversion(
                "legacy.txt",
                &ConversionReport {
                    source_encoding: "windows-1252",
                    had_replacements: true,
                },
            )
            .unwrap();

        let (normal, diagnostic) = reporter.into_parts();
        assert_eq!(
            String::from_utf8(normal).unwrap(),
            "-> Reading files and generating XML\n\
             -> Converted 'legacy.txt' from windows-1252 to UTF-8\n"
        );
        assert_eq!(
            String::from_utf8(diagnostic).unwrap(),
            "warning: 'legacy.txt' decoded as windows-1252 with replacement characters; information was lost\n"
        );
    }

    #[test]
    fn test_quiet_mode_suppresses_ordinary_output_but_not_errors() {
        let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
        reporter.phase("Hidden phase").unwrap();
        reporter
            .conversion(
                "legacy.txt",
                &ConversionReport {
                    source_encoding: "windows-1252",
                    had_replacements: true,
                },
            )
            .unwrap();
        reporter.error("genuine error").unwrap();
        reporter
            .always_visible_diagnostic("Warning: visible diagnostic")
            .unwrap();
        reporter
            .malformed_utf8_replacement("malformed.txt")
            .unwrap();

        let (normal, diagnostic) = reporter.into_parts();
        assert!(normal.is_empty());
        assert_eq!(
            diagnostic,
            b"genuine error\nWarning: visible diagnostic\n"
        );
    }
}
