use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const TEXT_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "kt", "swift",
    "c", "cpp", "cc", "cxx", "h", "hpp", "hxx", "cs", "rb", "php",
    "sh", "bash", "zsh", "fish", "pl", "pm", "r", "lua", "scala", "clj",
    "cljs", "edn", "ex", "exs", "erl", "hrl", "hs", "lhs", "ml", "mli",
    "fs", "fsx", "fsi", "v", "vh", "sv", "zig", "nim", "cr", "odin",
    "toml", "yaml", "yml", "json", "xml", "html", "css", "scss", "less",
    "md", "markdown", "rst", "txt", "cfg", "conf", "ini", "env",
    "dockerfile", "makefile", "cmake", "lock", "gradle", "sbt",
    "sql", "graphql", "proto", "nix", "dhall", "cue", "prisma",
];

#[derive(Serialize)]
pub struct GrepMatch {
    pub file: String,
    pub line_num: usize,
    pub line: String,
    pub context: Vec<String>,
}

#[derive(Serialize)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub modified: f64,
    pub extension: String,
}

fn is_text_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        TEXT_EXTENSIONS.contains(&ext.to_lowercase().as_str())
    } else {
        false
    }
}

fn compile_glob_pattern(pattern: &str) -> String {
    let mut re = String::from("^");
    let mut i = 0;
    let chars: Vec<char> = pattern.chars().collect();
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    if i + 2 < chars.len() && chars[i + 2] == '/' {
                        re.push_str("(?:.*/)?");
                        i += 3;
                        continue;
                    }
                    if i + 2 < chars.len() && chars[i + 2] == '.' {
                        re.push_str(".*");
                        i += 1;
                        continue;
                    }
                    if i + 2 >= chars.len() {
                        re.push_str(".*");
                        i += 2;
                        continue;
                    }
                    i += 1;
                    re.push_str(".*");
                } else {
                    re.push_str("[^/]*");
                    i += 1;
                }
            }
            '?' => {
                re.push_str("[^/]");
                i += 1;
            }
            '.' | '(' | ')' | '+' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                re.push('\\');
                re.push(chars[i]);
                i += 1;
            }
            '/' => {
                re.push('/');
                i += 1;
            }
            _ => {
                re.push(chars[i]);
                i += 1;
            }
        }
    }
    re.push('$');
    re
}

pub fn grep_repo(
    repo: &PathBuf,
    pattern: &str,
    path_filter: &str,
    max_results: usize,
    context_lines: usize,
    ignore_case: bool,
) -> (Vec<GrepMatch>, usize, bool) {
    let mut matches: Vec<GrepMatch> = Vec::with_capacity(max_results.min(1000));
    let mut total = 0usize;

    let re_str = if ignore_case {
        format!("(?i){}", pattern)
    } else {
        pattern.to_string()
    };
    let re = match Regex::new(&re_str) {
        Ok(r) => r,
        Err(_) => return (matches, 0, false),
    };

    let glob_re = Regex::new(&compile_glob_pattern(path_filter)).ok();

    let repo_str = repo.to_string_lossy().to_string();

    for entry in WalkDir::new(repo).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_text_file(path) {
            continue;
        }
        let rel = path
            .strip_prefix(&repo_str)
            .unwrap_or(path);
        let rel_str = rel.to_string_lossy();
        if let Some(ref gre) = glob_re {
            if !gre.is_match(&rel_str) {
                continue;
            }
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if re.is_match(line) {
                total += 1;
                if matches.len() < max_results {
                    let start = i.saturating_sub(context_lines);
                    let end = (i + context_lines + 1).min(lines.len());
                    let context: Vec<String> = lines[start..end]
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    matches.push(GrepMatch {
                        file: rel_str.to_string(),
                        line_num: i + 1,
                        line: line.to_string(),
                        context,
                    });
                }
            }
        }

        if matches.len() >= max_results && total >= max_results * 2 {
            break;
        }
    }

    let truncated = total > max_results;
    (matches, total, truncated)
}

pub fn glob_repo(
    repo: &PathBuf,
    pattern: &str,
    max_results: usize,
) -> (Vec<FileInfo>, usize, bool) {
    let mut files: Vec<FileInfo> = Vec::with_capacity(max_results.min(1000));
    let mut total = 0usize;

    let glob_re = compile_glob_pattern(pattern);
    let re = match Regex::new(&glob_re) {
        Ok(r) => r,
        Err(_) => return (files, 0, false),
    };

    let repo_str = repo.to_string_lossy().to_string();

    for entry in WalkDir::new(repo)
        .sort_by_file_name()
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(&repo_str).unwrap_or(path);
        let rel_str = rel.to_string_lossy();

        if !re.is_match(&rel_str) {
            continue;
        }

        total += 1;
        if files.len() < max_results {
            let metadata = match path.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            files.push(FileInfo {
                path: rel_str.to_string(),
                size: metadata.len(),
                modified,
                extension,
            });
        }
    }

    let truncated = total > max_results;
    (files, total, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn make_test_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().to_path_buf();

        let sub = repo.join("src");
        fs::create_dir_all(&sub).unwrap();

        let mut f1 = std::fs::File::create(repo.join("src/main.rs")).unwrap();
        writeln!(f1, "fn main() {{ println!(\"hello\"); }}").unwrap();
        writeln!(f1, "fn helper() {{ let x = 42; }}").unwrap();

        let mut f2 = std::fs::File::create(repo.join("src/lib.rs")).unwrap();
        writeln!(f2, "pub fn lib_func() {{ return 42; }}").unwrap();
        writeln!(f2, "// helper comment").unwrap();

        let mut f3 = std::fs::File::create(repo.join("Cargo.toml")).unwrap();
        writeln!(f3, "[package]\nname = \"test\"\nversion = \"0.1.0\"").unwrap();

        let mut f4 = std::fs::File::create(repo.join("src/test.py")).unwrap();
        writeln!(f4, "print('hello')").unwrap();
        writeln!(f4, "def helper(): return 42").unwrap();

        (dir, repo)
    }

    #[test]
    fn test_grep_basic() {
        let (_dir, repo) = make_test_repo();
        let (matches, total, truncated) = grep_repo(
            &repo, "hello", "**/*", 500, 0, true,
        );
        assert!(total >= 2, "should find hello in main.rs and test.py, got {}", total);
        assert!(!truncated);
        let files: Vec<&str> = matches.iter().map(|m| m.file.as_str()).collect();
        assert!(files.contains(&"src/main.rs"), "should contain src/main.rs: {:?}", files);
        assert!(files.contains(&"src/test.py"), "should contain src/test.py: {:?}", files);
    }

    #[test]
    fn test_grep_path_filter() {
        let (_dir, repo) = make_test_repo();
        let (matches, total, _) = grep_repo(
            &repo, "hello", "**/*.rs", 500, 0, true,
        );
        assert!(total >= 1);
        for m in &matches {
            assert!(m.file.ends_with(".rs"), "file {} should end with .rs", m.file);
        }
    }

    #[test]
    fn test_grep_case_insensitive() {
        let (_dir, repo) = make_test_repo();
        let (_matches, total, _) = grep_repo(
            &repo, "hello", "**/*", 500, 0, true,
        );
        assert!(total >= 2, "should find hello in main.rs and test.py, got {}", total);
    }

    #[test]
    fn test_grep_case_sensitive() {
        let (_dir, repo) = make_test_repo();
        let (_matches, total, _) = grep_repo(
            &repo, "HELLO", "**/*", 500, 0, false,
        );
        assert_eq!(total, 0, "case-sensitive HELLO should not match");
    }

    #[test]
    fn test_grep_max_results() {
        let (_dir, repo) = make_test_repo();
        let (_matches, total, truncated) = grep_repo(
            &repo, ".", "**/*", 1, 0, true,
        );
        assert!(total > 1, "should have more than 1 total match");
        assert!(truncated, "should be truncated at 1 result");
    }

    #[test]
    fn test_grep_context_lines() {
        let (_dir, repo) = make_test_repo();
        let (matches, _, _) = grep_repo(
            &repo, "helper", "**/*.rs", 500, 2, true,
        );
        for m in &matches {
            assert!(!m.context.is_empty(), "should have context lines");
            assert!(m.context.len() <= 5, "context should be at most 2*2+1=5 lines");
        }
    }

    #[test]
    fn test_glob_basic() {
        let (_dir, repo) = make_test_repo();
        let (files, total, _) = glob_repo(&repo, "**/*.rs", 500);
        assert!(total >= 2, "should find at least 2 .rs files, got {}", total);
        for f in &files {
            assert!(f.path.ends_with(".rs"), "{} should end with .rs", f.path);
        }
    }

    #[test]
    fn test_glob_max_results() {
        let (_dir, repo) = make_test_repo();
        let (files, total, truncated) = glob_repo(&repo, "**/*", 1);
        assert!(total >= 3, "should have at least 3 files total");
        assert!(truncated, "should be truncated at 1 result");
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_glob_file_info() {
        let (_dir, repo) = make_test_repo();
        let (files, _, _) = glob_repo(&repo, "**/*.toml", 500);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "Cargo.toml");
        assert!(files[0].size > 0, "file should have size > 0");
        assert_eq!(files[0].extension, "toml");
    }
}
