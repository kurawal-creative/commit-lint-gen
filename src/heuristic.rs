use std::collections::HashMap;

use crate::git::staged_numstat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileChange {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
    pub status: Status,
}

/// Generate a conventional commit from staged changes without AI.
pub fn generate() -> String {
    let out = staged_numstat();
    let changes = parse_numstat(&out);
    build(&changes)
}

pub fn build(changes: &[FileChange]) -> String {
    if changes.is_empty() {
        return "chore: update files".into();
    }
    let ty = infer_type(changes);
    let scope = infer_scope(changes);
    let description = generate_description(&ty, changes, scope.as_deref());
    format_message(&ty, scope.as_deref(), &description)
}

pub fn format_message(ty: &str, scope: Option<&str>, description: &str) -> String {
    match scope {
        Some(scope) if !scope.is_empty() => format!("{}({}): {}", ty, scope, description),
        _ => format!("{}: {}", ty, description),
    }
}

/// Git rename paths appear as `old.ts => new.ts` or `src/{old => new}/file.ts`.
fn extract_renamed_path(raw: &str) -> (String, bool) {
    if !raw.contains("=>") {
        return (raw.to_string(), false);
    }
    if let (Some(open), Some(close)) = (raw.find('{'), raw.rfind('}'))
        && open < close
    {
        let prefix = &raw[..open];
        let inner = &raw[open + 1..close];
        let suffix = &raw[close + 1..];
        let new_segment = inner.rsplit("=>").next().unwrap_or("").trim();
        let path = format!("{}{}{}", prefix, new_segment, suffix).replace("//", "/");
        return (path, true);
    }
    let new_path = raw.rsplit("=>").next().unwrap_or(raw).trim();
    (new_path.to_string(), true)
}

/// Parse `git diff --cached --numstat` (`add\tdel\tpath`).
pub fn parse_numstat(out: &str) -> Vec<FileChange> {
    let mut changes = Vec::new();
    for line in out.lines() {
        let mut parts = line.split('\t');
        let (Some(add), Some(del), Some(raw_path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if raw_path.trim().is_empty() {
            continue;
        }
        let (path, is_rename) = extract_renamed_path(raw_path.trim());
        let additions = add.parse::<usize>().unwrap_or(0);
        let deletions = del.parse::<usize>().unwrap_or(0);
        let binary = add == "-" || del == "-";

        let status = if is_rename {
            Status::Renamed
        } else if binary {
            Status::Modified
        } else if additions > 0 && deletions == 0 {
            Status::Added
        } else if additions == 0 && deletions > 0 {
            Status::Deleted
        } else {
            Status::Modified
        };

        changes.push(FileChange {
            path,
            additions,
            deletions,
            status,
        });
    }
    changes
}

fn is_test(path: &str) -> bool {
    path.starts_with("test/")
        || path.starts_with("tests/")
        || path.contains(".test.")
        || path.contains(".spec.")
        || path.contains("__tests__")
}

fn is_docs(path: &str) -> bool {
    path.starts_with("doc/")
        || path.starts_with("docs/")
        || path.ends_with(".md")
        || path.contains("README")
}

fn is_chore(path: &str) -> bool {
    path.contains("package.json")
        || path.contains("package-lock.json")
        || path.contains("pnpm-lock.yaml")
        || path.contains("yarn.lock")
        || path.contains("eslint")
        || path.contains("prettier")
        || path.contains("tsconfig")
        || path.contains(".config.")
}

fn is_style(path: &str) -> bool {
    [".css", ".scss", ".sass", ".less", ".styl"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

fn is_ci(path: &str) -> bool {
    path.starts_with(".github/")
        || path.contains(".gitlab-ci")
        || path.contains(".circleci")
        || path.contains("Dockerfile")
        || path.contains("docker-compose")
}

fn top_vote(votes: HashMap<&'static str, usize>) -> Option<&'static str> {
    let mut best: Option<(&str, usize)> = None;
    for (key, value) in votes {
        if best.is_none_or(|(_, best_value)| value > best_value) {
            best = Some((key, value));
        }
    }
    best.map(|(key, _)| key)
}

pub fn infer_type(changes: &[FileChange]) -> String {
    let mut votes: HashMap<&'static str, usize> = HashMap::new();
    for change in changes {
        let path = &change.path;
        if is_test(path) {
            *votes.entry("test").or_default() += 1;
        }
        if is_docs(path) {
            *votes.entry("docs").or_default() += 1;
        }
        if is_chore(path) {
            *votes.entry("chore").or_default() += 1;
        }
        if is_style(path) {
            *votes.entry("style").or_default() += 1;
        }
        if is_ci(path) {
            *votes.entry("ci").or_default() += 1;
        }
        match change.status {
            Status::Added => *votes.entry("feat").or_default() += 1,
            Status::Deleted | Status::Renamed => *votes.entry("refactor").or_default() += 1,
            Status::Modified => {}
        }
    }

    if votes.is_empty() {
        let additions: usize = changes.iter().map(|c| c.additions).sum();
        let deletions: usize = changes.iter().map(|c| c.deletions).sum();
        return if additions > deletions * 2 {
            "feat"
        } else if deletions > additions {
            "refactor"
        } else {
            "fix"
        }
        .to_string();
    }

    top_vote(votes).unwrap_or("chore").to_string()
}

fn scope_for(path: &str) -> Option<&'static str> {
    let path = path.strip_prefix("src/").unwrap_or(path);
    let first = path.split('/').next().unwrap_or("");
    let known = match first {
        "components" | "component" => "components",
        "utils" | "util" => "utils",
        "api" => "api",
        "hooks" | "hook" => "hooks",
        "services" | "service" => "services",
        "models" | "model" => "models",
        "types" | "type" => "types",
        "lib" => "lib",
        "tests" | "test" | "__tests__" => "tests",
        "docs" | "doc" => "docs",
        _ => return None,
    };
    Some(known)
}

pub fn infer_scope(changes: &[FileChange]) -> Option<String> {
    let mut votes: HashMap<&'static str, usize> = HashMap::new();
    for change in changes {
        if let Some(scope) = scope_for(&change.path) {
            *votes.entry(scope).or_default() += 1;
        }
    }
    if votes.is_empty() {
        return None;
    }
    let total: usize = votes.values().sum();
    let best = top_vote(votes.clone())?;
    let count = votes[best];
    if count as f64 / total as f64 > 0.6 {
        Some(best.to_string())
    } else {
        None
    }
}

pub fn generate_description(ty: &str, changes: &[FileChange], scope: Option<&str>) -> String {
    let added = changes.iter().filter(|c| c.status == Status::Added).count();
    let deleted = changes
        .iter()
        .filter(|c| c.status == Status::Deleted)
        .count();
    let modified = changes
        .iter()
        .filter(|c| c.status == Status::Modified)
        .count();
    let renamed = changes
        .iter()
        .filter(|c| c.status == Status::Renamed)
        .count();

    match ty {
        "feat" => match (scope, added) {
            (Some(scope), _) => format!("add new {} functionality", scope),
            (None, added) if added > 0 => "add new features".into(),
            _ => "implement new functionality".into(),
        },
        "fix" => match scope {
            Some(scope) => format!("resolve issues in {}", scope),
            None => "resolve bugs and issues".into(),
        },
        "docs" => "update documentation".into(),
        "test" => {
            if added > 0 {
                "add test coverage".into()
            } else {
                "update tests".into()
            }
        }
        "refactor" => {
            if renamed > 0 && renamed >= deleted && renamed >= modified {
                match scope {
                    Some(scope) => format!("rename and reorganize {} files", scope),
                    None => "rename and reorganize files".into(),
                }
            } else if deleted > modified {
                "remove deprecated code".into()
            } else {
                match scope {
                    Some(scope) => format!("refactor {} implementation", scope),
                    None => "refactor code structure".into(),
                }
            }
        }
        "style" => "update styles and formatting".into(),
        "chore" => {
            if changes.iter().any(|c| c.path.contains("package")) {
                "update dependencies".into()
            } else {
                "update build configuration".into()
            }
        }
        "ci" => "update CI/CD pipeline".into(),
        _ => match scope {
            Some(scope) => format!("update {}", scope),
            None => "update codebase".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, status: Status) -> FileChange {
        FileChange {
            path: path.into(),
            additions: 1,
            deletions: 0,
            status,
        }
    }

    #[test]
    fn parses_numstat_and_statuses() {
        let out = "10\t0\tsrc/api/new.rs\n0\t5\tsrc/old.rs\n3\t2\tsrc/mod.rs\n";
        let changes = parse_numstat(out);
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].status, Status::Added);
        assert_eq!(changes[1].status, Status::Deleted);
        assert_eq!(changes[2].status, Status::Modified);
    }

    #[test]
    fn parses_binary_and_renames() {
        let out = "-\t-\tassets/logo.png\n4\t1\tsrc/{old => new}/file.rs\n2\t0\told.ts => new.ts\n";
        let changes = parse_numstat(out);
        assert_eq!(changes[0].status, Status::Modified);
        assert_eq!(changes[1].path, "src/new/file.rs");
        assert_eq!(changes[1].status, Status::Renamed);
        assert_eq!(changes[2].path, "new.ts");
    }

    #[test]
    fn infers_types_from_paths() {
        assert_eq!(
            infer_type(&[change("src/a.test.ts", Status::Modified)]),
            "test"
        );
        assert_eq!(infer_type(&[change("README.md", Status::Modified)]), "docs");
        assert_eq!(
            infer_type(&[change(".github/workflows/ci.yml", Status::Modified)]),
            "ci"
        );
        assert_eq!(
            infer_type(&[change("src/theme.css", Status::Modified)]),
            "style"
        );
        assert_eq!(infer_type(&[change("src/new.rs", Status::Added)]), "feat");
        assert_eq!(
            infer_type(&[change("src/old.rs", Status::Deleted)]),
            "refactor"
        );
    }

    #[test]
    fn infers_scope_when_dominant() {
        let changes = vec![
            change("src/api/a.rs", Status::Modified),
            change("src/api/b.rs", Status::Modified),
        ];
        assert_eq!(infer_scope(&changes).as_deref(), Some("api"));
        let mixed = vec![
            change("src/api/a.rs", Status::Modified),
            change("src/utils/b.rs", Status::Modified),
        ];
        assert_eq!(infer_scope(&mixed), None);
    }

    #[test]
    fn description_matches_type_and_scope() {
        let changes = vec![change("src/api/a.rs", Status::Modified)];
        assert_eq!(
            generate_description("fix", &changes, Some("api")),
            "resolve issues in api"
        );
        assert_eq!(
            generate_description("docs", &changes, None),
            "update documentation"
        );
        assert_eq!(
            generate_description("chore", &[change("package.json", Status::Modified)], None),
            "update dependencies"
        );
    }

    #[test]
    fn formats_message_with_and_without_scope() {
        assert_eq!(
            format_message("feat", Some("api"), "add x"),
            "feat(api): add x"
        );
        assert_eq!(format_message("feat", None, "add x"), "feat: add x");
    }

    #[test]
    fn empty_diff_falls_back_to_chore() {
        assert_eq!(build(&[]), "chore: update files");
    }
}
