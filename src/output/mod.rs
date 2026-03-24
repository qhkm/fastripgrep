pub mod color;

use crate::search::verify::Match;

pub fn format_match(m: &Match, use_color: bool) -> String {
    if use_color {
        let before = &m.line_content[..m.match_start];
        let matched = &m.line_content[m.match_start..m.match_end];
        let after = &m.line_content[m.match_end..];
        format!(
            "{}{}{}:{}{}{}:{}{}{}{}{}{}",
            color::MAGENTA,
            m.file_path,
            color::RESET,
            color::GREEN,
            m.line_number,
            color::RESET,
            before,
            color::RED,
            color::BOLD,
            matched,
            color::RESET,
            after
        )
    } else {
        format!("{}:{}:{}", m.file_path, m.line_number, m.line_content)
    }
}

pub fn format_context_line(
    line_num: usize,
    content: &str,
    file_path: &str,
    use_color: bool,
) -> String {
    if use_color {
        format!(
            "{}{}{}-{}{}{}-{}",
            color::MAGENTA,
            file_path,
            color::RESET,
            color::GREEN,
            line_num,
            color::RESET,
            content
        )
    } else {
        format!("{}-{}-{}", file_path, line_num, content)
    }
}

pub fn format_match_json(m: &Match) -> String {
    serde_json::json!({
        "file": m.file_path,
        "line": m.line_number,
        "content": m.line_content,
        "match_start": m.match_start,
        "match_end": m.match_end,
    })
    .to_string()
}

pub fn format_count(file_path: &str, count: usize, use_color: bool) -> String {
    if use_color {
        format!("{}{}{}:{}", color::MAGENTA, file_path, color::RESET, count)
    } else {
        format!("{}:{}", file_path, count)
    }
}

pub fn unique_files(matches: &[Match]) -> Vec<String> {
    let mut files: Vec<String> = matches.iter().map(|m| m.file_path.clone()).collect();
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_match_plain() {
        let m = Match {
            file_path: "src/main.rs".to_string(),
            line_number: 42,
            line_content: "fn hello() {}".to_string(),
            match_start: 3,
            match_end: 8,
            context_before: vec![],
            context_after: vec![],
        };
        let output = format_match(&m, false);
        assert_eq!(output, "src/main.rs:42:fn hello() {}");
    }

    #[test]
    fn test_format_match_colored() {
        let m = Match {
            file_path: "src/main.rs".to_string(),
            line_number: 1,
            line_content: "fn hello() {}".to_string(),
            match_start: 3,
            match_end: 8,
            context_before: vec![],
            context_after: vec![],
        };
        let output = format_match(&m, true);
        assert!(output.contains("\x1b[35m")); // MAGENTA for file
        assert!(output.contains("\x1b[31m")); // RED for match
        assert!(output.contains("hello")); // matched text
    }

    #[test]
    fn test_format_context_line_plain() {
        let output = format_context_line(5, "some content", "file.rs", false);
        assert_eq!(output, "file.rs-5-some content");
    }

    #[test]
    fn test_format_count_plain() {
        let output = format_count("file.rs", 3, false);
        assert_eq!(output, "file.rs:3");
    }

    #[test]
    fn test_format_match_json() {
        let m = Match {
            file_path: "src/main.rs".to_string(),
            line_number: 42,
            line_content: "fn hello() {}".to_string(),
            match_start: 3,
            match_end: 8,
            context_before: vec![],
            context_after: vec![],
        };
        let json = format_match_json(&m);
        assert!(json.contains("\"file\":\"src/main.rs\""));
        assert!(json.contains("\"line\":42"));
    }

    #[test]
    fn test_unique_files() {
        let matches = vec![
            Match {
                file_path: "a.rs".into(),
                line_number: 1,
                line_content: "x".into(),
                match_start: 0,
                match_end: 1,
                context_before: vec![],
                context_after: vec![],
            },
            Match {
                file_path: "a.rs".into(),
                line_number: 2,
                line_content: "y".into(),
                match_start: 0,
                match_end: 1,
                context_before: vec![],
                context_after: vec![],
            },
            Match {
                file_path: "b.rs".into(),
                line_number: 1,
                line_content: "z".into(),
                match_start: 0,
                match_end: 1,
                context_before: vec![],
                context_after: vec![],
            },
        ];
        assert_eq!(unique_files(&matches), vec!["a.rs", "b.rs"]);
    }
}
