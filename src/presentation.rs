#[cfg(test)]
use colored_text::{ColorLevel, TerminalCapabilities};
use colored_text::{Colorize, RenderTarget, StyledText};

#[derive(Clone, Copy)]
enum SemanticStyle {
    Heading,
    Phase,
    Success,
    Warning,
    Error,
    Accent,
    SummaryValue,
}

pub(crate) struct Presentation {
    normal_target: RenderTarget,
    diagnostic_target: RenderTarget,
}

impl Presentation {
    pub(crate) const fn terminal() -> Self {
        Self {
            normal_target: RenderTarget::Stdout,
            diagnostic_target: RenderTarget::Stderr,
        }
    }

    #[cfg(test)]
    pub(crate) const fn plain() -> Self {
        let capabilities = TerminalCapabilities {
            is_terminal: false,
            color_level: ColorLevel::NoColor,
        };
        Self {
            normal_target: RenderTarget::Capabilities(capabilities),
            diagnostic_target: RenderTarget::Capabilities(capabilities),
        }
    }

    #[cfg(test)]
    pub(crate) const fn ansi16() -> Self {
        let capabilities = TerminalCapabilities {
            is_terminal: true,
            color_level: ColorLevel::Ansi16,
        };
        Self {
            normal_target: RenderTarget::Capabilities(capabilities),
            diagnostic_target: RenderTarget::Capabilities(capabilities),
        }
    }

    pub(crate) fn phase(&self, message: &str) -> String {
        format!("{} {message}", self.normal("->", SemanticStyle::Phase))
    }

    pub(crate) fn header(
        &self,
        version: &str,
        authors: &str,
        description: &str,
    ) -> String {
        let heading = self.normal(
            &format!("BundleRepo Version {version}"),
            SemanticStyle::Heading,
        );
        format!("\n{heading}, © 2024-2026 {authors}\n\n{description}\n\n")
    }

    pub(crate) fn phase_with_accent(
        &self,
        before: &str,
        accent: &str,
        after: &str,
    ) -> String {
        format!(
            "{} {before}{}{after}",
            self.normal("->", SemanticStyle::Phase),
            self.normal(accent, SemanticStyle::Accent),
        )
    }

    pub(crate) fn conversion(&self, path: &str, encoding: &str) -> String {
        format!(
            "{} Converted '{}' from {} to UTF-8",
            self.normal("->", SemanticStyle::Phase),
            self.normal(path, SemanticStyle::Accent),
            self.normal(encoding, SemanticStyle::Accent),
        )
    }

    pub(crate) fn conversion_warning(
        &self,
        path: &str,
        encoding: &str,
    ) -> String {
        format!(
            "{} '{}' decoded as {} with replacement characters; information was lost",
            self.diagnostic("warning:", SemanticStyle::Warning),
            self.diagnostic(path, SemanticStyle::Accent),
            self.diagnostic(encoding, SemanticStyle::Accent),
        )
    }

    pub(crate) fn malformed_utf8_warning(&self, path: &str) -> String {
        format!(
            "{} '{}' contained malformed UTF-8 and was decoded with replacement characters; information was lost",
            self.diagnostic("warning:", SemanticStyle::Warning),
            self.diagnostic(path, SemanticStyle::Accent),
        )
    }

    pub(crate) fn success(&self, remainder: &str) -> String {
        format!(
            "{} {}{remainder}",
            self.normal("->", SemanticStyle::Success),
            self.normal("Successfully", SemanticStyle::Success),
        )
    }

    pub(crate) fn success_with_accent(
        &self,
        before: &str,
        accent: &str,
        after: &str,
    ) -> String {
        format!(
            "{} {}{before}{}{after}",
            self.normal("->", SemanticStyle::Success),
            self.normal("Successfully", SemanticStyle::Success),
            self.normal(accent, SemanticStyle::Accent),
        )
    }

    pub(crate) fn clone_success(
        &self,
        repository_url: &str,
        branch: Option<&str>,
    ) -> String {
        let branch = branch.map_or_else(String::new, |branch| {
            format!(
                " (branch: {})",
                self.normal(branch, SemanticStyle::Accent)
            )
        });
        self.success_with_accent(
            " cloned repository '",
            repository_url,
            &format!("'{branch}"),
        )
    }

    pub(crate) fn repository_found(&self, path: &str, branch: &str) -> String {
        format!(
            "{} Found a git repository in the current directory: '{}' (branch: {})",
            self.normal("->", SemanticStyle::Phase),
            self.normal(path, SemanticStyle::Accent),
            self.normal(branch, SemanticStyle::Accent),
        )
    }

    pub(crate) fn warning_with_accent(
        &self,
        prefix: &str,
        before: &str,
        accent: &str,
        after: &str,
    ) -> String {
        format!(
            "{}{before}{}{after}",
            self.diagnostic(prefix, SemanticStyle::Warning),
            self.diagnostic(accent, SemanticStyle::Accent),
        )
    }

    pub(crate) fn error_with_accent(
        &self,
        prefix: &str,
        before: &str,
        accent: &str,
        after: &str,
    ) -> String {
        format!(
            "{}{before}{}{after}",
            self.diagnostic(prefix, SemanticStyle::Error),
            self.diagnostic(accent, SemanticStyle::Accent),
        )
    }

    pub(crate) fn summary(&self, table: &str, values: &[String]) -> String {
        let table = table
            .lines()
            .enumerate()
            .map(|(index, row)| {
                let Some(value) = values.get(index) else {
                    return row.to_string();
                };
                let content_end = row.trim_end().len();
                let (content, padding) = row.split_at(content_end);
                content.strip_suffix(value).map_or_else(
                    || row.to_string(),
                    |label| {
                        format!(
                            "{label}{}{padding}",
                            self.normal(value, SemanticStyle::SummaryValue)
                        )
                    },
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n{}\n{table}\n\n",
            self.normal("Summary:", SemanticStyle::Heading)
        )
    }

    pub(crate) fn warning(&self, message: &str) -> String {
        self.diagnostic_prefix(message, &["warning:", "Warning:"])
    }

    pub(crate) fn error(&self, message: &str) -> String {
        self.diagnostic_prefix(message, &["Error:", "ERROR:", "X"])
    }

    pub(crate) fn diagnostic_message(&self, message: &str) -> String {
        if message.starts_with("warning:") || message.starts_with("Warning:") {
            self.warning(message)
        } else {
            self.error(message)
        }
    }

    fn normal(&self, text: &str, style: SemanticStyle) -> String {
        Self::styled(text, style).render(self.normal_target)
    }

    fn diagnostic(&self, text: &str, style: SemanticStyle) -> String {
        Self::styled(text, style).render(self.diagnostic_target)
    }

    fn diagnostic_prefix(&self, message: &str, prefixes: &[&str]) -> String {
        prefixes
            .iter()
            .find_map(|prefix| {
                message.strip_prefix(prefix).map(|remainder| {
                    let style = if prefix.starts_with('w')
                        || prefix.starts_with('W')
                    {
                        SemanticStyle::Warning
                    } else {
                        SemanticStyle::Error
                    };
                    format!("{}{remainder}", self.diagnostic(prefix, style))
                })
            })
            .unwrap_or_else(|| message.to_string())
    }

    fn styled(text: &str, style: SemanticStyle) -> StyledText {
        match style {
            SemanticStyle::Heading => text.cyan().bold(),
            SemanticStyle::Phase | SemanticStyle::Accent => text.cyan(),
            SemanticStyle::SummaryValue => text.cyan().bold(),
            SemanticStyle::Success => text.green().bold(),
            SemanticStyle::Warning => text.yellow().bold(),
            SemanticStyle::Error => text.red().bold(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/crate/presentation.rs"]
mod tests;
