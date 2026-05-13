pub mod audit;
pub mod config;
pub mod document;
pub mod i18n;
pub mod scanner;

pub use audit::{
    AuditReport, AuditResult, AuditSummary, FixSuggestion, KeyUsage, MissingTranslation,
    PlaceholderIssue, PlaceholderIssueType, UnusedKey,
};
pub use config::{I18nConfig, KeyStyle};
pub use i18n::key_finder::{FoundKey, KeyFinder};
pub use i18n::parser::TranslationParser;
pub use i18n::store::{TranslationEntry, TranslationLocation, TranslationStore};
pub use scanner::{CodeKeyOccurrence, CodeScanner, ScannedFile};
