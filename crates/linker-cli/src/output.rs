use chrono::{DateTime, Utc};
use linker_core::state::Item;
use unicode_width::UnicodeWidthStr;

/// Keep user-controlled names, paths and errors on one terminal line. In
/// particular, do not emit escape sequences embedded in filesystem names.
pub fn label(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            '|' => out.push_str("\\|"),
            c if c.is_control() => out.extend(c.escape_unicode()),
            c => out.push(c),
        }
    }
    out
}

pub fn table<const N: usize>(headers: [&str; N], rows: Vec<[String; N]>) -> String {
    let rows: Vec<_> = std::iter::once(headers.map(str::to_owned))
        .chain(rows)
        .map(|row| row.map(|value| label(&value)))
        .collect();
    let widths: [usize; N] =
        std::array::from_fn(|col| rows.iter().map(|row| row[col].width()).max().unwrap_or(0));
    let border = format!("+{}+\n", widths.map(|w| "-".repeat(w + 2)).join("+"));
    let mut output = border.clone();
    for (index, row) in rows.iter().enumerate() {
        output.push('|');
        for (col, value) in row.iter().enumerate() {
            output.push(' ');
            output.push_str(value);
            output.push_str(&" ".repeat(widths[col] - value.width() + 1));
            output.push('|');
        }
        output.push('\n');
        if index == 0 {
            output.push_str(&border);
        }
    }
    output.push_str(&border);
    output
}

pub fn items(items: &[Item]) -> String {
    if items.is_empty() {
        return "no items\n".into();
    }
    table(
        [
            "NAME",
            "TYPE",
            "STATUS",
            "SOURCE",
            "TARGET",
            "LAST SYNC (UTC)",
            "LAST ERROR",
        ],
        items
            .iter()
            .map(|item| {
                let time = item.last_sync_at.map_or_else(
                    || "-".into(),
                    |seconds| {
                        DateTime::<Utc>::from_timestamp(seconds, 0)
                            .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| format!("invalid ({seconds})"))
                    },
                );
                [
                    item.name.clone(),
                    item.item_type.clone(),
                    item.status.clone(),
                    item.local_path.clone(),
                    item.cloud_path.clone(),
                    time,
                    item.last_error
                        .clone()
                        .filter(|error| !error.is_empty())
                        .unwrap_or_else(|| "-".into()),
                ]
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> Item {
        Item {
            id: "id".into(),
            name: "中文笔记".into(),
            item_type: "directory".into(),
            local_path: "/Users/example/完整 路径".into(),
            cloud_path: "/Volumes/target/完整 路径".into(),
            status: "active".into(),
            last_sync_at: Some(0),
            last_error: None,
        }
    }

    #[test]
    fn unicode_cells_align_without_truncating_paths() {
        let output = items(&[item()]);
        assert!(output.contains("中文笔记"));
        assert!(output.contains("/Volumes/target/完整 路径"));
        assert!(output.contains("1970-01-01 00:00:00"));
        let width = output.lines().next().unwrap().width();
        assert!(output.lines().all(|line| line.width() == width));
    }

    #[test]
    fn controls_are_escaped_and_errors_do_not_break_rows() {
        let mut entry = item();
        entry.name = "e\u{301}|name".into();
        entry.last_error = Some("failed\nnext\tline\r\u{1b}[31m\\path".into());
        let output = items(&[entry]);
        assert_eq!(output.lines().count(), 5);
        assert!(!output.contains('\u{1b}'));
        assert!(output.contains("failed\\nnext\\tline\\r\\u{1b}[31m\\\\path"));
        assert!(output.contains("e\u{301}\\|name"));
        let width = output.lines().next().unwrap().width();
        assert!(output.lines().all(|line| line.width() == width));
    }

    #[test]
    fn missing_and_invalid_times_are_explicit() {
        let mut entry = item();
        entry.last_sync_at = None;
        assert!(items(&[entry.clone()]).contains("| -"));
        entry.last_sync_at = Some(i64::MAX);
        assert!(items(&[entry]).contains("invalid (9223372036854775807)"));
        assert_eq!(items(&[]), "no items\n");
    }
}
