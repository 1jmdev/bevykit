//! Translated text with interpolation, plural rules, fallbacks, and locale-aware formatting.
//!
//! Each language is a TOML file supplied by the game. Nested tables flatten into dotted keys,
//! and a table whose keys are plural categories (`zero`, `one`, `two`, `few`, `many`, `other`)
//! is a plural message selected by the `count` argument. The optional `[locale]` table
//! describes the language.
//!
//! Top-level messages must come before the first table header, as TOML assigns keys to the
//! most recent table.
//!
//! ```toml
//! welcome = "Welcome, {player}!"
//!
//! [locale]
//! name = "English"
//! direction = "ltr"
//! decimal_separator = "."
//! grouping_separator = ","
//!
//! [settings]
//! title = "Settings"
//!
//! [inventory.count]
//! one = "{count} item"
//! other = "{count} items"
//! ```
//!
//! Text is requested with [`tr!`](crate::tr), which produces a [`LocalizedText`]. Attached to
//! a UI text entity, it refreshes whenever the language changes or a file is reloaded.
//!
//! ```ignore
//! ui.label(tr!("inventory.count", count = inventory.len()));
//! let greeting = locale.translate(&tr!("welcome", player = name));
//! ```

use std::borrow::Cow;
use std::fmt::{self, Write as _};
use std::sync::Arc;
use std::time::Duration;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use intl_pluralrules::{PluralCategory, PluralRuleType, PluralRules};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use unic_langid::LanguageIdentifier;

/// A language tag such as `en`, `en-US`, or `cs`.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Reflect, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageId(pub Cow<'static, str>);

impl LanguageId {
    /// Creates a language identifier.
    pub fn new(tag: impl Into<Cow<'static, str>>) -> Self {
        Self(tag.into())
    }

    /// Returns the tag.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for LanguageId {
    fn default() -> Self {
        Self::new("en")
    }
}

impl From<&'static str> for LanguageId {
    fn from(tag: &'static str) -> Self {
        Self::new(tag)
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A key referring to translated text, for use in content definitions and other data.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Reflect, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalizedKey(pub Cow<'static, str>);

impl LocalizedKey {
    /// Creates a key.
    pub fn new(key: impl Into<Cow<'static, str>>) -> Self {
        Self(key.into())
    }

    /// Returns a [`LocalizedText`] for this key without arguments.
    pub fn text(&self) -> LocalizedText {
        LocalizedText::new(self.0.clone())
    }
}

/// The value of an interpolation argument.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub enum TextArg {
    /// Plain text, inserted as-is.
    Text(String),
    /// A number, formatted for the current locale. Also selects plural forms.
    Number {
        /// The value.
        value: f64,
        /// Fixed number of decimals, or `None` to show up to two when needed.
        decimals: Option<u8>,
    },
}

impl TextArg {
    /// A number shown with a fixed number of decimals.
    pub fn fixed(value: f64, decimals: u8) -> Self {
        Self::Number {
            value,
            decimals: Some(decimals),
        }
    }
}

macro_rules! number_arg {
    ($($ty:ty),*) => {
        $(
            impl From<$ty> for TextArg {
                fn from(value: $ty) -> Self {
                    Self::Number {
                        value: value as f64,
                        decimals: None,
                    }
                }
            }
        )*
    };
}

number_arg!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

impl From<String> for TextArg {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for TextArg {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<&String> for TextArg {
    fn from(value: &String) -> Self {
        Self::Text(value.clone())
    }
}

impl From<Cow<'static, str>> for TextArg {
    fn from(value: Cow<'static, str>) -> Self {
        Self::Text(value.into_owned())
    }
}

/// A translation request: a key and its arguments.
///
/// As a component on an entity with UI text, it keeps the text translated.
#[derive(Component, Clone, Debug, PartialEq, Reflect)]
#[reflect(Component)]
pub struct LocalizedText {
    /// The message key.
    pub key: Cow<'static, str>,
    /// Interpolation arguments.
    pub args: Vec<(Cow<'static, str>, TextArg)>,
}

impl LocalizedText {
    /// Creates a request without arguments.
    pub fn new(key: impl Into<Cow<'static, str>>) -> Self {
        Self {
            key: key.into(),
            args: Vec::new(),
        }
    }

    /// Adds an argument.
    pub fn arg(mut self, name: impl Into<Cow<'static, str>>, value: impl Into<TextArg>) -> Self {
        self.set_arg(name, value);
        self
    }

    /// Sets or replaces an argument.
    pub fn set_arg(&mut self, name: impl Into<Cow<'static, str>>, value: impl Into<TextArg>) {
        let name = name.into();
        let value = value.into();
        match self.args.iter_mut().find(|(existing, _)| *existing == name) {
            Some((_, existing)) => *existing = value,
            None => self.args.push((name, value)),
        }
    }

    fn get(&self, name: &str) -> Option<&TextArg> {
        self.args
            .iter()
            .find(|(existing, _)| existing == name)
            .map(|(_, value)| value)
    }
}

/// Builds a [`LocalizedText`] from a key and named arguments.
///
/// ```
/// use bevykit_data::tr;
///
/// let text = tr!("inventory.count", count = 3);
/// assert_eq!(text.key, "inventory.count");
/// ```
#[macro_export]
macro_rules! tr {
    ($key:expr $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::localization::LocalizedText::new($key)
            $(.arg(stringify!($name), $value))*
    };
}

/// Writing direction of a language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextDirection {
    /// Left to right.
    #[default]
    Ltr,
    /// Right to left.
    Rtl,
}

/// Information about a language from the `[locale]` table.
#[derive(Clone, Debug, Reflect, Serialize, Deserialize)]
#[serde(default)]
pub struct LanguageInfo {
    /// The language's own name for itself.
    pub name: String,
    /// Writing direction.
    pub direction: TextDirection,
    /// Decimal separator for numbers.
    pub decimal_separator: String,
    /// Thousands grouping separator for numbers.
    pub grouping_separator: String,
}

impl Default for LanguageInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            direction: TextDirection::Ltr,
            decimal_separator: ".".to_string(),
            grouping_separator: ",".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Segment {
    Text(String),
    Arg(String),
}

#[derive(Clone, Debug, PartialEq)]
enum MessageForms {
    Single(Vec<Segment>),
    Plural(Vec<(PluralForm, Vec<Segment>)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PluralForm {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralForm {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "zero" => Self::Zero,
            "one" => Self::One,
            "two" => Self::Two,
            "few" => Self::Few,
            "many" => Self::Many,
            "other" => Self::Other,
            _ => return None,
        })
    }

    fn from_category(category: PluralCategory) -> Self {
        match category {
            PluralCategory::ZERO => Self::Zero,
            PluralCategory::ONE => Self::One,
            PluralCategory::TWO => Self::Two,
            PluralCategory::FEW => Self::Few,
            PluralCategory::MANY => Self::Many,
            PluralCategory::OTHER => Self::Other,
        }
    }
}

/// A parsed translation file.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct TranslationFile {
    info: LanguageInfo,
    messages: HashMap<String, MessageForms>,
}

impl TranslationFile {
    /// Parses a translation file from TOML.
    pub fn parse(source: &str) -> Result<Self, TranslationError> {
        let table: toml::Table = source.parse()?;
        let mut info = LanguageInfo::default();
        let mut messages = HashMap::default();
        for (key, value) in table {
            if key == "locale" {
                info = value.try_into()?;
                continue;
            }
            flatten(&key, value, &mut messages)?;
        }
        Ok(Self { info, messages })
    }

    /// Returns the language information.
    pub fn info(&self) -> &LanguageInfo {
        &self.info
    }

    /// Returns the number of messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Returns `true` if the file has no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

fn flatten(
    prefix: &str,
    value: toml::Value,
    messages: &mut HashMap<String, MessageForms>,
) -> Result<(), TranslationError> {
    match value {
        toml::Value::String(text) => {
            messages.insert(prefix.to_string(), MessageForms::Single(parse_template(&text)));
        }
        toml::Value::Table(table) => {
            let is_plural = !table.is_empty()
                && table.keys().all(|key| PluralForm::parse(key).is_some())
                && table.values().all(toml::Value::is_str);
            if is_plural {
                if !table.contains_key("other") {
                    return Err(TranslationError::MissingOther(prefix.to_string()));
                }
                let forms = table
                    .into_iter()
                    .filter_map(|(form, text)| {
                        Some((PluralForm::parse(&form)?, parse_template(text.as_str()?)))
                    })
                    .collect();
                messages.insert(prefix.to_string(), MessageForms::Plural(forms));
            } else {
                for (key, value) in table {
                    flatten(&format!("{prefix}.{key}"), value, messages)?;
                }
            }
        }
        _ => return Err(TranslationError::InvalidValue(prefix.to_string())),
    }
    Ok(())
}

fn parse_template(source: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut text = String::new();
    let mut characters = source.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '{' if characters.peek() == Some(&'{') => {
                characters.next();
                text.push('{');
            }
            '}' if characters.peek() == Some(&'}') => {
                characters.next();
                text.push('}');
            }
            '{' => {
                let name: String = characters.by_ref().take_while(|c| *c != '}').collect();
                if !text.is_empty() {
                    segments.push(Segment::Text(std::mem::take(&mut text)));
                }
                segments.push(Segment::Arg(name.trim().to_string()));
            }
            other => text.push(other),
        }
    }
    if !text.is_empty() {
        segments.push(Segment::Text(text));
    }
    segments
}

fn template_args(forms: &MessageForms) -> HashSet<&str> {
    let segments: Vec<&Segment> = match forms {
        MessageForms::Single(segments) => segments.iter().collect(),
        MessageForms::Plural(forms) => forms
            .iter()
            .flat_map(|(_, segments)| segments.iter())
            .collect(),
    };
    segments
        .into_iter()
        .filter_map(|segment| match segment {
            Segment::Arg(name) => Some(name.as_str()),
            Segment::Text(_) => None,
        })
        .collect()
}

/// A failure parsing a translation file.
#[derive(Error, Debug)]
pub enum TranslationError {
    /// The file could not be read.
    #[error("could not read translation file: {0}")]
    Io(#[from] std::io::Error),
    /// The file is not valid UTF-8.
    #[error("translation file is not valid UTF-8: {0}")]
    Encoding(#[from] std::str::Utf8Error),
    /// The file is not valid TOML.
    #[error("translation file is not valid TOML: {0}")]
    Syntax(#[from] toml::de::Error),
    /// A message is neither text nor a table.
    #[error("message `{0}` must be text or a table")]
    InvalidValue(String),
    /// A plural message lacks the required `other` form.
    #[error("plural message `{0}` has no `other` form")]
    MissingOther(String),
}

/// Loads [`TranslationFile`]s from `.toml` files.
#[derive(Default, TypePath)]
pub struct TranslationLoader;

impl AssetLoader for TranslationLoader {
    type Asset = TranslationFile;
    type Settings = ();
    type Error = TranslationError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<TranslationFile, TranslationError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        TranslationFile::parse(std::str::from_utf8(&bytes)?)
    }

    fn extensions(&self) -> &[&str] {
        &["toml"]
    }
}

struct Catalog {
    file: TranslationFile,
    plurals: Option<PluralRules>,
}

impl Catalog {
    fn new(language: &LanguageId, file: TranslationFile) -> Self {
        Self {
            plurals: plural_rules(language),
            file,
        }
    }
}

fn plural_rules(language: &LanguageId) -> Option<PluralRules> {
    let identifier: LanguageIdentifier = language.as_str().parse().ok()?;
    PluralRules::create(identifier.clone(), PluralRuleType::CARDINAL)
        .ok()
        .or_else(|| {
            let base: LanguageIdentifier = identifier.language.as_str().parse().ok()?;
            PluralRules::create(base, PluralRuleType::CARDINAL).ok()
        })
}

/// A problem found by [`Locale::validate`].
#[derive(Clone, Debug, PartialEq)]
pub enum LocalizationIssue {
    /// A key present in the primary language is missing from another.
    MissingKey {
        /// The language lacking the key.
        language: LanguageId,
        /// The key.
        key: String,
    },
    /// A translation uses different arguments than the primary language.
    ArgumentMismatch {
        /// The language with the mismatch.
        language: LanguageId,
        /// The key.
        key: String,
        /// Arguments in the primary language.
        expected: Vec<String>,
        /// Arguments in this language.
        found: Vec<String>,
    },
}

impl fmt::Display for LocalizationIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKey { language, key } => write!(formatter, "[{language}] missing `{key}`"),
            Self::ArgumentMismatch {
                language,
                key,
                expected,
                found,
            } => write!(
                formatter,
                "[{language}] `{key}` uses arguments {found:?}, expected {expected:?}"
            ),
        }
    }
}

/// The active language and every loaded translation.
#[derive(Resource)]
pub struct Locale {
    language: LanguageId,
    primary: LanguageId,
    fallbacks: Vec<LanguageId>,
    sources: Vec<(LanguageId, String)>,
    handles: HashMap<LanguageId, Handle<TranslationFile>>,
    catalogs: HashMap<LanguageId, Arc<Catalog>>,
    revision: u64,
}

impl Locale {
    /// Returns the active language.
    pub fn language(&self) -> &LanguageId {
        &self.language
    }

    /// Changes the active language. Unknown languages fall back through the fallback chain.
    pub fn set(&mut self, language: impl Into<LanguageId>) {
        let language = language.into();
        if language != self.language {
            self.language = language;
            self.revision += 1;
        }
    }

    /// Returns the languages for which a translation file is registered.
    pub fn available(&self) -> impl Iterator<Item = &LanguageId> {
        self.sources.iter().map(|(language, _)| language)
    }

    /// Returns information about a loaded language.
    pub fn info(&self, language: &LanguageId) -> Option<&LanguageInfo> {
        self.catalogs.get(language).map(|catalog| catalog.file.info())
    }

    /// Returns the writing direction of the active language.
    pub fn direction(&self) -> TextDirection {
        self.active_catalog()
            .map(|catalog| catalog.file.info.direction)
            .unwrap_or_default()
    }

    /// Returns a counter that increases whenever translated text may have changed.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns `true` once every registered language has loaded.
    pub fn is_ready(&self) -> bool {
        self.sources
            .iter()
            .all(|(language, _)| self.catalogs.contains_key(language))
    }

    /// Returns `true` if the key exists in the active language or a fallback.
    pub fn contains(&self, key: &str) -> bool {
        self.chain().any(|catalog| catalog.file.messages.contains_key(key))
    }

    /// Translates a request. Missing keys produce the key itself, so they are easy to spot.
    pub fn translate(&self, text: &LocalizedText) -> String {
        for catalog in self.chain() {
            if let Some(forms) = catalog.file.messages.get(text.key.as_ref()) {
                return self.render(catalog, forms, text);
            }
        }
        text.key.to_string()
    }

    /// Translates a key without arguments.
    pub fn get(&self, key: &str) -> String {
        self.translate(&LocalizedText::new(key.to_string()))
    }

    /// Formats a number with the active language's separators.
    pub fn format_number(&self, value: f64, decimals: Option<u8>) -> String {
        let info = self
            .active_catalog()
            .map(|catalog| catalog.file.info.clone())
            .unwrap_or_default();
        format_number(value, decimals, &info)
    }

    /// Formats a duration as a clock: `1:05:09` or `5:09`.
    pub fn format_duration(&self, duration: Duration) -> String {
        let total = duration.as_secs();
        let (hours, minutes, seconds) = (total / 3600, (total / 60) % 60, total % 60);
        if hours > 0 {
            format!("{hours}:{minutes:02}:{seconds:02}")
        } else {
            format!("{minutes}:{seconds:02}")
        }
    }

    /// Formats a duration using the largest two units, such as `2h 5m`.
    ///
    /// Each unit uses the message `time.days`, `time.hours`, `time.minutes`, or `time.seconds`
    /// with a `count` argument when the language defines it, and a short English suffix
    /// otherwise.
    pub fn format_duration_long(&self, duration: Duration) -> String {
        let total = duration.as_secs();
        let units = [
            ("time.days", "d", total / 86_400),
            ("time.hours", "h", (total / 3600) % 24),
            ("time.minutes", "m", (total / 60) % 60),
            ("time.seconds", "s", total % 60),
        ];
        let first = units.iter().position(|(_, _, value)| *value > 0).unwrap_or(3);
        let mut output = String::new();
        for (key, suffix, value) in units.iter().skip(first).take(2) {
            if !output.is_empty() {
                output.push(' ');
            }
            if self.contains(key) {
                output.push_str(&self.translate(&LocalizedText::new(*key).arg("count", *value)));
            } else {
                let _ = write!(output, "{value}{suffix}");
            }
        }
        output
    }

    /// Compares every language with the primary language and reports missing keys and
    /// argument mismatches.
    pub fn validate(&self) -> Vec<LocalizationIssue> {
        let Some(primary) = self.catalogs.get(&self.primary) else {
            return Vec::new();
        };
        let mut issues = Vec::new();
        let mut keys: Vec<&String> = primary.file.messages.keys().collect();
        keys.sort();
        for (language, _) in &self.sources {
            if *language == self.primary {
                continue;
            }
            let Some(catalog) = self.catalogs.get(language) else {
                continue;
            };
            for key in &keys {
                let Some(forms) = catalog.file.messages.get(*key) else {
                    issues.push(LocalizationIssue::MissingKey {
                        language: language.clone(),
                        key: (*key).clone(),
                    });
                    continue;
                };
                let expected = template_args(&primary.file.messages[*key]);
                let found = template_args(forms);
                if expected != found {
                    let mut expected: Vec<String> = expected.into_iter().map(String::from).collect();
                    let mut found: Vec<String> = found.into_iter().map(String::from).collect();
                    expected.sort();
                    found.sort();
                    issues.push(LocalizationIssue::ArgumentMismatch {
                        language: language.clone(),
                        key: (*key).clone(),
                        expected,
                        found,
                    });
                }
            }
        }
        issues
    }

    /// Installs a translation directly, without the asset system. Useful in tests and tools.
    pub fn insert(&mut self, language: impl Into<LanguageId>, file: TranslationFile) {
        let language = language.into();
        if !self.sources.iter().any(|(existing, _)| *existing == language) {
            self.sources.push((language.clone(), String::new()));
        }
        self.catalogs
            .insert(language.clone(), Arc::new(Catalog::new(&language, file)));
        self.revision += 1;
    }

    fn active_catalog(&self) -> Option<&Catalog> {
        self.chain().next()
    }

    fn chain(&self) -> impl Iterator<Item = &Catalog> {
        let base = self
            .language
            .as_str()
            .split(['-', '_'])
            .next()
            .map(|base| LanguageId::new(base.to_string()));
        std::iter::once(self.language.clone())
            .chain(base)
            .chain(self.fallbacks.iter().cloned())
            .chain(std::iter::once(self.primary.clone()))
            .filter_map(|language| self.catalogs.get(&language).map(Arc::as_ref))
    }

    fn render(&self, catalog: &Catalog, forms: &MessageForms, text: &LocalizedText) -> String {
        let segments = match forms {
            MessageForms::Single(segments) => segments,
            MessageForms::Plural(forms) => {
                let count = match text.get("count") {
                    Some(TextArg::Number { value, .. }) => *value,
                    _ => 0.0,
                };
                let form = if count == 0.0 && forms.iter().any(|(form, _)| *form == PluralForm::Zero)
                {
                    PluralForm::Zero
                } else {
                    catalog
                        .plurals
                        .as_ref()
                        .and_then(|rules| select_plural(rules, count))
                        .map(PluralForm::from_category)
                        .unwrap_or(if count == 1.0 {
                            PluralForm::One
                        } else {
                            PluralForm::Other
                        })
                };
                forms
                    .iter()
                    .find(|(candidate, _)| *candidate == form)
                    .or_else(|| forms.iter().find(|(candidate, _)| *candidate == PluralForm::Other))
                    .map(|(_, segments)| segments)
                    .expect("plural messages always have an `other` form")
            }
        };

        let mut output = String::new();
        for segment in segments {
            match segment {
                Segment::Text(literal) => output.push_str(literal),
                Segment::Arg(name) => match text.get(name) {
                    Some(TextArg::Text(value)) => output.push_str(value),
                    Some(TextArg::Number { value, decimals }) => {
                        output.push_str(&format_number(*value, *decimals, &catalog.file.info));
                    }
                    None => {
                        let _ = write!(output, "{{{name}}}");
                    }
                },
            }
        }
        output
    }
}

fn select_plural(rules: &PluralRules, count: f64) -> Option<PluralCategory> {
    if count.fract() == 0.0 && count.abs() < i64::MAX as f64 {
        rules.select(count as i64).ok()
    } else {
        rules.select(count).ok()
    }
}

fn format_number(value: f64, decimals: Option<u8>, info: &LanguageInfo) -> String {
    let fixed = decimals.is_some();
    let decimals = decimals.unwrap_or(if value.fract() == 0.0 { 0 } else { 2 });
    let formatted = format!("{:.*}", decimals as usize, value.abs());
    let (integer, fraction) = match formatted.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (formatted.as_str(), None),
    };
    let mut output = String::new();
    if value < 0.0 && formatted.chars().any(|c| c.is_ascii_digit() && c != '0') {
        output.push('-');
    }
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index) % 3 == 0 {
            output.push_str(&info.grouping_separator);
        }
        output.push(digit);
    }
    if let Some(fraction) = fraction {
        // Without fixed decimals, trailing zeros carry no information.
        let fraction = if !fixed {
            fraction.trim_end_matches('0')
        } else {
            fraction
        };
        if !fraction.is_empty() {
            output.push_str(&info.decimal_separator);
            output.push_str(fraction);
        }
    }
    output
}

/// Loads translation files and keeps [`LocalizedText`] current.
pub struct KitLocalizationPlugin {
    primary: LanguageId,
    fallbacks: Vec<LanguageId>,
    sources: Vec<(LanguageId, String)>,
}

impl KitLocalizationPlugin {
    /// Creates the plugin. `primary` is the language that is complete and used as the final
    /// fallback and as the reference for validation.
    pub fn new(primary: impl Into<LanguageId>) -> Self {
        Self {
            primary: primary.into(),
            fallbacks: Vec::new(),
            sources: Vec::new(),
        }
    }

    /// Registers a language and the asset path of its translation file.
    pub fn language(mut self, language: impl Into<LanguageId>, path: impl Into<String>) -> Self {
        self.sources.push((language.into(), path.into()));
        self
    }

    /// Adds a fallback consulted before the primary language.
    pub fn fallback(mut self, language: impl Into<LanguageId>) -> Self {
        self.fallbacks.push(language.into());
        self
    }
}

impl Plugin for KitLocalizationPlugin {
    fn build(&self, app: &mut App) {
        bevykit_core::ensure_core(app);
        app.init_asset::<TranslationFile>()
            .init_asset_loader::<TranslationLoader>()
            .register_type::<LocalizedText>()
            .insert_resource(Locale {
                language: self.primary.clone(),
                primary: self.primary.clone(),
                fallbacks: self.fallbacks.clone(),
                sources: self.sources.clone(),
                handles: HashMap::default(),
                catalogs: HashMap::default(),
                revision: 0,
            })
            .add_systems(PreStartup, load_translations)
            .add_systems(PreUpdate, sync_catalogs);
    }
}

fn load_translations(mut locale: ResMut<Locale>, assets: Res<AssetServer>) {
    let sources = locale.sources.clone();
    for (language, path) in sources {
        if !path.is_empty() {
            locale.handles.insert(language, assets.load(path));
        }
    }
}

fn sync_catalogs(
    mut locale: ResMut<Locale>,
    mut events: MessageReader<AssetEvent<TranslationFile>>,
    files: Res<Assets<TranslationFile>>,
) {
    let mut changed = false;
    for event in events.read() {
        let (AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id }) = event
        else {
            continue;
        };
        let language = locale
            .handles
            .iter()
            .find(|(_, handle)| handle.id() == *id)
            .map(|(language, _)| language.clone());
        if let (Some(language), Some(file)) = (language, files.get(*id)) {
            let catalog = Arc::new(Catalog::new(&language, file.clone()));
            locale.catalogs.insert(language, catalog);
            changed = true;
        }
    }
    if changed {
        locale.revision += 1;
        if locale.is_ready() {
            for issue in locale.validate() {
                warn!("Localization: {issue}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locale() -> Locale {
        let mut locale = Locale {
            language: "en".into(),
            primary: "en".into(),
            fallbacks: Vec::new(),
            sources: Vec::new(),
            handles: HashMap::default(),
            catalogs: HashMap::default(),
            revision: 0,
        };
        let english = r#"
            welcome = "Welcome, {player}!"
            braces = "{{literal}}"
            [inventory.count]
            one = "{count} item"
            other = "{count} items"
        "#;
        let czech = r#"
            welcome = "Vítej, {name}!"
            [locale]
            decimal_separator = ","
            grouping_separator = " "
            [inventory.count]
            one = "{count} předmět"
            few = "{count} předměty"
            other = "{count} předmětů"
        "#;
        locale.insert("en", TranslationFile::parse(english).unwrap());
        locale.insert("cs", TranslationFile::parse(czech).unwrap());
        locale
    }

    #[test]
    fn interpolates_and_selects_plurals() {
        let mut locale = locale();
        assert_eq!(locale.translate(&tr!("welcome", player = "Ada")), "Welcome, Ada!");
        assert_eq!(locale.translate(&tr!("inventory.count", count = 1)), "1 item");
        assert_eq!(locale.translate(&tr!("inventory.count", count = 1500)), "1,500 items");
        assert_eq!(locale.get("braces"), "{literal}");

        locale.set("cs");
        assert_eq!(locale.translate(&tr!("inventory.count", count = 3)), "3 předměty");
        assert_eq!(locale.translate(&tr!("inventory.count", count = 1500)), "1 500 předmětů");
        assert_eq!(locale.get("braces"), "{literal}");
        assert_eq!(locale.get("missing.key"), "missing.key");
    }

    #[test]
    fn validation_reports_missing_keys_and_arguments() {
        let issues = locale().validate();
        assert!(issues.contains(&LocalizationIssue::MissingKey {
            language: "cs".into(),
            key: "braces".into(),
        }));
        assert!(issues.iter().any(|issue| matches!(
            issue,
            LocalizationIssue::ArgumentMismatch { key, .. } if key == "welcome"
        )));
    }

    #[test]
    fn formats_numbers_and_durations() {
        let info = LanguageInfo::default();
        assert_eq!(format_number(1234567.891, None, &info), "1,234,567.89");
        assert_eq!(format_number(-0.5, Some(2), &info), "-0.50");
        assert_eq!(format_number(12.0, None, &info), "12");
        let locale = locale();
        assert_eq!(locale.format_duration(Duration::from_secs(3725)), "1:02:05");
        assert_eq!(locale.format_duration_long(Duration::from_secs(3725)), "1h 2m");
    }
}
