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
    pub rule_path: String,
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
        rule_path: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: 1,
            id: id.clone(),
            name,
            item_type,
            source_path,
            target_path,
            rule_path,
            created_at: now,
            updated_at: now,
        }
    }
}

pub fn write_manifest(path: &Path, manifest: &Manifest) -> Result<()> {
    let json = serde_json::to_string_pretty(manifest)?;
    fs::write(path, format!("{json}\n"))?;
    Ok(())
}
