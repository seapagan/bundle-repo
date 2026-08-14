#[cfg(test)]
use colored_text::{ColorLevel, TerminalCapabilities};
use colored_text::{Colorize, RenderTarget, StyledText};

#[derive(Clone, Copy)]
enum SemanticStyle {
    Phase,
    Warning,
    Error,
    Accent,
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

    pub(crate) fn phase(&self, message: &str) -> String {
        format!("{} {message}", self.normal("->", SemanticStyle::Phase))
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

    pub(crate) fn warning(&self, message: &str) -> String {
        self.diagnostic_prefix(message, &["warning:", "Warning:"])
    }

    pub(crate) fn error(&self, message: &str) -> String {
        self.diagnostic_prefix(message, &["Error:", "ERROR:", "X"])
    }

    #[cfg(test)]
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
            SemanticStyle::Phase | SemanticStyle::Accent => text.cyan(),
            SemanticStyle::Warning => text.yellow().bold(),
            SemanticStyle::Error => text.red().bold(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/crate/presentation.rs"]
mod tests;
