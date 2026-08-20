use std::fs;
use std::path::Path;

use crate::{LinkerError, Result};

#[derive(Debug, Clone)]
pub struct Rule {
    pub pattern: String,
}

pub fn initial_rules(ignore_file: Option<&Path>, extra_excludes: &[String]) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();

    if let Some(ignore_file) = ignore_file {
        let contents = fs::read_to_string(ignore_file)?;
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            push_unique_rule(&mut rules, trimmed.to_string());
        }
    }

    for pattern in extra_excludes {
        push_unique_rule(&mut rules, normalize_rule_pattern(pattern)?);
    }

    Ok(rules)
}

fn push_unique_rule(rules: &mut Vec<Rule>, pattern: String) {
    if rules.iter().any(|rule| rule.pattern == pattern) {
        return;
    }
    rules.push(Rule { pattern });
}

pub fn normalize_rule_pattern(pattern: &str) -> Result<String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(LinkerError::InvalidRulePattern(
            "pattern must not be empty".to_string(),
        ));
    }
    Ok(pattern.to_string())
}

pub fn write_rule_snapshot(path: &Path, rules: &[Rule]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut text = rules
        .iter()
        .filter(|rule| !rule.pattern.is_empty())
        .map(|rule| rule.pattern.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    fs::write(path, text)?;
    Ok(())
}
