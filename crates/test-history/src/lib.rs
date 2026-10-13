//! Harness evidence models and parsers, based on the local run's log producers.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod parse;
pub mod queries;
#[cfg(feature = "store")]
pub mod store;

/// Version one of the evidence schema. SQL migrations are append-only.
pub const SCHEMA: &str = include_str!("schema.sql");
/// Child tables replaced atomically when a source is ingested again.
pub const TABLES: &[&str] = &[
    "tests",
    "failures",
    "compile",
    "resources",
    "play_checks",
    "agents",
    "captures",
    "merges",
    "metrics",
    "evidence_lines",
];
/// Default external log root, never copied into the repository.
pub const LOG_ROOT: &str = "/run/media/pengolodh/linux-data-games/tmp/soe-run";

/// Named fields in an evidence row. Unknown run facts remain absent.
pub type Row = BTreeMap<String, Value>;

/// One atomic step result, also the JSON format accepted by `record`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Run identity and facts. `id` and `kind` are required by the store.
    pub run: Row,
    /// Child table names mapped to rows.
    #[serde(default)]
    pub rows: BTreeMap<String, Vec<Row>>,
}

impl Evidence {
    /// Create a step with a caller-supplied idempotency key.
    pub fn new(id: &str, kind: &str) -> Self {
        Self {
            run: row(&[("id", Value::from(id)), ("kind", Value::from(kind))]),
            rows: BTreeMap::new(),
        }
    }

    /// Add one child row. IDs and run links are assigned when written.
    pub fn push(&mut self, table: &str, value: Row) {
        self.rows.entry(table.to_owned()).or_default().push(value);
    }

    /// Get the step id, rejecting missing or non-text identities.
    pub fn id(&self) -> Result<&str> {
        self.run
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Input("record requires a nonempty text run.id".to_owned()))
            .and_then(|id| {
                if id.is_empty() {
                    Err(Error::Input("empty run.id".to_owned()))
                } else {
                    Ok(id)
                }
            })
    }

    /// Retain image references and metadata while omitting inline image bytes.
    pub fn omit_image_bytes(&mut self) -> Result<()> {
        let urls = regex::Regex::new(r#"(?i)data:image/[^\s\"'<>]+"#)?;
        for value in self.run.values_mut().chain(
            self.rows
                .values_mut()
                .flat_map(|rows| rows.iter_mut().flat_map(|row| row.values_mut())),
        ) {
            omit_images(value, &urls)?;
        }
        Ok(())
    }
}

fn omit_images(value: &mut Value, urls: &regex::Regex) -> Result<()> {
    match value {
        Value::Object(fields) => {
            let image = fields.get("type").and_then(Value::as_str) == Some("image")
                || ["mimeType", "mime_type"].into_iter().any(|key| {
                    fields
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|mime| mime.starts_with("image/"))
                });
            if image {
                for key in ["data", "base64", "image_data"] {
                    if fields.remove(key).is_some() {
                        fields.insert("image_bytes_omitted".to_owned(), Value::Bool(true));
                    }
                }
            }
            for value in fields.values_mut() {
                omit_images(value, urls)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                omit_images(value, urls)?;
            }
        }
        Value::String(text) => {
            if let std::borrow::Cow::Owned(redacted) =
                urls.replace_all(text, "<image bytes omitted>")
            {
                *text = redacted;
            }
            if text.contains("\"data\"") && (text.contains("image/") || text.contains("\"image\""))
            {
                if let Ok(mut parsed) = serde_json::from_str::<Value>(text) {
                    if !parsed.is_string() {
                        omit_images(&mut parsed, urls)?;
                        *text = serde_json::to_string(&parsed)?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Build a row from named JSON values.
pub fn row(values: &[(&str, Value)]) -> Row {
    values
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

/// Errors returned by parsers and storage without erasing their cause.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// External source or database file could not be accessed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Structured evidence was malformed.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// Invalid command, schema version, row, or query argument.
    #[error("{0}")]
    Input(String),
    /// Invalid parser expression.
    #[error("{0}")]
    Regex(#[from] regex::Error),
    /// Turso rejected an operation.
    #[cfg(feature = "store")]
    #[error("{0}")]
    Database(#[from] turso::Error),
}

/// A crate operation's result.
pub type Result<T> = std::result::Result<T, Error>;

/// Preserve caller metadata next to a log so rebuild has the same facts.
pub fn write_manifest(path: &Path, evidence: &Evidence) -> Result<()> {
    let manifest = path.with_extension(format!(
        "{}.history.json",
        path.extension().and_then(|x| x.to_str()).unwrap_or("log")
    ));
    let mut facts = evidence.clone();
    facts.rows.clear();
    std::fs::write(manifest, serde_json::to_vec_pretty(&facts)?)?;
    Ok(())
}
