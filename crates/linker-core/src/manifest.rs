use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub source_path: String,
    pub target_path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Manifest {
    pub fn new(
        id: String,
        name: String,
        item_type: String,
        source_path: String,
        target_path: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: 2,
            id: id.clone(),
            name,
            item_type,
            source_path,
            target_path,
            created_at: now,
            updated_at: now,
        }
    }
}

pub fn write_manifest(path: &Path, manifest: &Manifest) -> Result<()> {
    write_json_atomic(path, &serde_json::to_value(manifest)?)
}

pub(crate) fn write_json_atomic(path: &Path, value: &serde_json::Value) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        writeln!(file, "{}", serde_json::to_string_pretty(value)?)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    Ok(())
}
