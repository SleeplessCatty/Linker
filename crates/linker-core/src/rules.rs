//! Linker's deliberately small .gitignore language. No Git configuration is read.
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RuleWarning {
    pub file: PathBuf,
    pub line: usize,
    pub reason: String,
    pub pattern: String,
}

impl std::fmt::Display for RuleWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: skipped {:?}: {}",
            self.file.display(),
            self.line,
            self.pattern,
            self.reason
        )
    }
}

#[derive(Debug, Clone)]
struct Rule {
    base: PathBuf,
    parts: Vec<String>,
    anchored: bool,
    directory_only: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Rules {
    entries: Vec<Rule>,
}

impl Rules {
    pub fn add(&mut self, base: &Path, file: &Path, contents: &str) -> Vec<RuleWarning> {
        let mut warnings = Vec::new();
        for (index, line) in contents.lines().enumerate() {
            let pattern = line.trim();
            if pattern.is_empty() || pattern.starts_with('#') {
                continue;
            }
            let reason = if pattern.starts_with('!') {
                Some("negation (!) is not supported")
            } else if pattern.contains("**") {
                Some("recursive wildcards (**) are not supported")
            } else if pattern.contains(['?', '[', ']', '\\']) {
                Some("?, character classes and backslash escapes are not supported")
            } else {
                None
            };
            let directory_only = pattern.ends_with('/');
            let body = pattern.strip_prefix('/').unwrap_or(pattern);
            let body = if directory_only {
                body.strip_suffix('/').unwrap_or(body)
            } else {
                body
            };
            let parts: Vec<_> = body.split('/').map(str::to_owned).collect();
            let reason = reason.or_else(|| {
                parts
                    .iter()
                    .any(|p| p.is_empty() || p == "." || p == "..")
                    .then_some("empty paths and . or .. path segments are not supported")
            });
            if let Some(reason) = reason {
                warnings.push(RuleWarning {
                    file: file.to_path_buf(),
                    line: index + 1,
                    reason: reason.to_owned(),
                    pattern: pattern.to_owned(),
                });
            } else {
                self.entries.push(Rule {
                    base: base.to_path_buf(),
                    parts,
                    anchored: pattern.starts_with('/') || body.contains('/'),
                    directory_only,
                });
            }
        }
        warnings
    }

    /// Association-relative path; ancestors count as directories. A control file
    /// is exempt only when none of its parent directories is ignored.
    pub fn ignored(&self, relative: &Path, is_dir: bool) -> bool {
        self.entries.iter().any(|rule| {
            let Ok(tail) = relative.strip_prefix(&rule.base) else {
                return false;
            };
            let parts: Vec<_> = tail
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect();
            for end in 1..=parts.len() {
                let directory = end < parts.len() || is_dir;
                if !directory && parts[end - 1] == ".gitignore" {
                    continue;
                }
                if rule.directory_only && !directory {
                    continue;
                }
                let matched = if rule.anchored {
                    end == rule.parts.len()
                        && rule
                            .parts
                            .iter()
                            .zip(&parts[..end])
                            .all(|(p, s)| star_match(p, s))
                } else {
                    star_match(&rule.parts[0], &parts[end - 1])
                };
                if matched {
                    return true;
                }
            }
            false
        })
    }
}

// Single-star matching over Unicode characters. Callers supply one path segment.
fn star_match(pattern: &str, name: &str) -> bool {
    let p: Vec<_> = pattern.chars().collect();
    let s: Vec<_> = name.chars().collect();
    let (mut i, mut j, mut star, mut retry) = (0, 0, None, 0);
    while j < s.len() {
        if i < p.len() && p[i] == '*' {
            star = Some(i);
            i += 1;
            retry = j;
        } else if i < p.len() && p[i] == s[j] {
            i += 1;
            j += 1;
        } else if let Some(at) = star {
            retry += 1;
            j = retry;
            i = at + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_language_table() {
        for (pattern, path, directory, expected) in [
            (".env", "nested/.env", false, true),
            (".env", ".env.local", false, false),
            ("cache/", "nested/cache/a", false, true),
            ("cache/", "cache", false, false),
            ("cache/", "cache", true, true),
            ("/build", "build/a", false, true),
            ("/build", "x/build/a", false, false),
            ("src/cache/", "src/cache/a", false, true),
            ("src/cache/", "x/src/cache/a", false, false),
            ("*.log", "x/.hidden.log", false, true),
            ("temp*", "temp", false, true),
            ("labs/*/.env", "labs/one/.env", false, true),
            ("labs/*/.env", "labs/one/two/.env", false, false),
            ("*.log", "X.LOG", false, false),
            ("  my file  ", "my file", false, true),
            ("笔*", "笔记", false, true),
            ("*", ".gitignore", false, false),
            ("*", "sub/.gitignore", false, true),
        ] {
            let mut rules = Rules::default();
            assert!(rules
                .add(Path::new(""), Path::new(".gitignore"), pattern)
                .is_empty());
            assert_eq!(
                rules.ignored(Path::new(path), directory),
                expected,
                "{pattern:?}: {path}"
            );
        }
    }

    #[test]
    fn nested_rules_only_add_exclusions_and_invalid_lines_warn() {
        let mut rules = Rules::default();
        let warnings = rules.add(
            Path::new(""),
            Path::new(".gitignore"),
            "# comment\n\n*.log\n!keep.log\n**/x\nx?\n*.py[cod]\nx\\y\n../secret\n./a\n/\na//b\n",
        );
        assert_eq!(warnings.len(), 9);
        assert_eq!(warnings[0].line, 4);
        assert!(rules.ignored(Path::new("keep.log"), false));
        rules.add(
            Path::new("sub"),
            Path::new("sub/.gitignore"),
            "/cache/\n.env",
        );
        assert!(rules.ignored(Path::new("sub/cache/a"), false));
        assert!(!rules.ignored(Path::new("other/cache/a"), false));
        assert!(!rules.ignored(Path::new("sub/deep/cache/a"), false));
        assert!(rules.ignored(Path::new("sub/deep/.env"), false));
    }
}
