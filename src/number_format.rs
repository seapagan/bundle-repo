use icu_decimal::{
    DecimalFormatter, DecimalFormatterPreferences, input::Decimal,
};
use icu_locale_core::Locale;

const FALLBACK_LOCALE: &str = "en-US";
const UNITS: [(&str, u64); 7] = [
    ("bytes", 1),
    ("KiB", 1 << 10),
    ("MiB", 1 << 20),
    ("GiB", 1 << 30),
    ("TiB", 1 << 40),
    ("PiB", 1 << 50),
    ("EiB", 1 << 60),
];

#[derive(Clone, Copy)]
enum PlatformLocaleSource {
    #[cfg(any(test, all(unix, not(target_vendor = "apple"))))]
    GenericUnix,
    #[cfg(any(test, target_vendor = "apple"))]
    NativeUnix,
    #[cfg(any(test, windows))]
    Windows,
}

enum NormalizedLocale {
    C,
    Icu(Locale),
}

enum NumericConvention {
    C,
    Icu(DecimalFormatter),
    English,
}

pub(crate) struct NumberFormatter {
    convention: NumericConvention,
}

impl NumberFormatter {
    pub(crate) fn system() -> Self {
        let candidate = system_locale_candidate();
        Self::from_candidate(candidate.as_deref())
    }

    #[cfg(test)]
    pub(crate) fn from_locale(locale: &str) -> Self {
        Self::from_candidate(Some(locale))
    }

    pub(crate) fn format_count(&self, value: usize) -> String {
        self.format_integer(value as u64)
    }

    pub(crate) fn format_output_size(
        &self,
        bytes: u64,
        compressed: bool,
    ) -> String {
        let exact = self.format_integer(bytes);
        let human = self.format_human_size(bytes);
        let qualifier = if compressed { ", compressed" } else { "" };
        format!("{exact} ({human}{qualifier})")
    }

    fn from_candidate(candidate: Option<&str>) -> Self {
        Self::from_candidate_with(candidate, decimal_formatter)
    }

    fn from_candidate_with<F>(candidate: Option<&str>, mut factory: F) -> Self
    where
        F: FnMut(Locale) -> Option<DecimalFormatter>,
    {
        match candidate.and_then(normalize_locale) {
            Some(NormalizedLocale::C) => Self {
                convention: NumericConvention::C,
            },
            Some(NormalizedLocale::Icu(locale)) => factory(locale)
                .map(|formatter| Self {
                    convention: NumericConvention::Icu(formatter),
                })
                .unwrap_or_else(|| Self::english_fallback(&mut factory)),
            None => Self::english_fallback(&mut factory),
        }
    }

    fn english_fallback<F>(factory: &mut F) -> Self
    where
        F: FnMut(Locale) -> Option<DecimalFormatter>,
    {
        let formatter =
            FALLBACK_LOCALE.parse::<Locale>().ok().and_then(factory);
        formatter.map_or(
            Self {
                convention: NumericConvention::English,
            },
            |formatter| Self {
                convention: NumericConvention::Icu(formatter),
            },
        )
    }

    fn format_integer(&self, value: u64) -> String {
        match &self.convention {
            NumericConvention::C => value.to_string(),
            NumericConvention::English => group_english(value),
            NumericConvention::Icu(formatter) => {
                formatter.format_to_string(&Decimal::from(value))
            }
        }
    }

    fn format_human_size(&self, bytes: u64) -> String {
        if bytes < 1024 {
            let unit = if bytes == 1 { "byte" } else { "bytes" };
            return format!("{} {unit}", self.format_integer(bytes));
        }

        let mut unit_index = unit_index(bytes);
        let mut tenths = rounded_tenths(bytes, UNITS[unit_index].1);
        if tenths == 10_240 && unit_index + 1 < UNITS.len() {
            unit_index += 1;
            tenths = rounded_tenths(bytes, UNITS[unit_index].1);
        }

        format!("{} {}", self.format_tenths(tenths), UNITS[unit_index].0)
    }

    fn format_tenths(&self, tenths: u64) -> String {
        match &self.convention {
            NumericConvention::C => {
                format!("{}.{:01}", tenths / 10, tenths % 10)
            }
            NumericConvention::English => {
                format!("{}.{:01}", group_english(tenths / 10), tenths % 10)
            }
            NumericConvention::Icu(formatter) => {
                let mut decimal = Decimal::from(tenths);
                decimal.multiply_pow10(-1);
                formatter.format_to_string(&decimal)
            }
        }
    }
}

fn decimal_formatter(locale: Locale) -> Option<DecimalFormatter> {
    let mut preferences = DecimalFormatterPreferences::from(locale);
    let latn = "und-u-nu-latn".parse::<Locale>().ok()?;
    preferences.numbering_system =
        DecimalFormatterPreferences::from(latn).numbering_system;
    DecimalFormatter::try_new(preferences, Default::default()).ok()
}

fn normalize_locale(candidate: &str) -> Option<NormalizedLocale> {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return None;
    }

    let suffix = candidate.find(['.', '@']).unwrap_or(candidate.len());
    let base = &candidate[..suffix];
    if base.eq_ignore_ascii_case("c") || base.eq_ignore_ascii_case("posix") {
        return Some(NormalizedLocale::C);
    }

    base.replace('_', "-")
        .parse::<Locale>()
        .ok()
        .map(NormalizedLocale::Icu)
}

fn resolve_locale_candidate<G, N>(
    source: PlatformLocaleSource,
    mut environment: G,
    _native: N,
) -> Option<String>
where
    G: FnMut(&str) -> Option<String>,
    N: FnOnce() -> Option<String>,
{
    #[cfg(any(test, windows))]
    if matches!(source, PlatformLocaleSource::Windows) {
        return _native().and_then(non_empty);
    }

    for key in ["LC_ALL", "LC_NUMERIC", "LANG"] {
        if let Some(candidate) = environment(key).and_then(non_empty) {
            return Some(candidate);
        }
    }

    match source {
        #[cfg(any(test, target_vendor = "apple"))]
        PlatformLocaleSource::NativeUnix => _native().and_then(non_empty),
        #[cfg(any(test, all(unix, not(target_vendor = "apple"))))]
        PlatformLocaleSource::GenericUnix => None,
        #[cfg(any(test, windows))]
        PlatformLocaleSource::Windows => None,
    }
}

fn non_empty(candidate: String) -> Option<String> {
    let candidate = candidate.trim();
    (!candidate.is_empty()).then(|| candidate.to_string())
}

#[cfg(target_vendor = "apple")]
fn system_locale_candidate() -> Option<String> {
    resolve_locale_candidate(
        PlatformLocaleSource::NativeUnix,
        environment_locale,
        sys_locale::get_locale,
    )
}

#[cfg(all(unix, not(target_vendor = "apple")))]
fn system_locale_candidate() -> Option<String> {
    resolve_locale_candidate(
        PlatformLocaleSource::GenericUnix,
        environment_locale,
        || None,
    )
}

#[cfg(windows)]
fn system_locale_candidate() -> Option<String> {
    resolve_locale_candidate(
        PlatformLocaleSource::Windows,
        |_| None,
        sys_locale::get_locale,
    )
}

#[cfg(not(any(unix, windows)))]
fn system_locale_candidate() -> Option<String> {
    None
}

#[cfg(unix)]
fn environment_locale(key: &str) -> Option<String> {
    std::env::var_os(key).map(|value| value.to_string_lossy().into_owned())
}

fn group_english(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(character);
    }
    grouped
}

fn unit_index(bytes: u64) -> usize {
    UNITS
        .iter()
        .rposition(|(_, factor)| bytes >= *factor)
        .unwrap_or(0)
}

fn rounded_tenths(bytes: u64, factor: u64) -> u64 {
    let quotient = bytes / factor;
    let remainder = bytes % factor;
    quotient * 10 + (remainder * 10 + factor / 2) / factor
}

#[cfg(test)]
#[path = "../tests/crate/number_format.rs"]
mod tests;
