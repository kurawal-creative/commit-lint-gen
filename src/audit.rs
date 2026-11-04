use crate::git;
use crate::linter::{self, ValidationError};
use crate::types::Config;

#[derive(Debug, Clone)]
pub struct CommitAnalysis {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
    pub errors: Vec<ValidationError>,
}

impl CommitAnalysis {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn analyze(limit: usize, config: &Config) -> Result<Vec<CommitAnalysis>, String> {
    let raw = git::log(limit)?;
    Ok(analyze_log(&raw, config))
}

/// Parse output of `git log` with `%h%x1f%s%x1f%an%x1f%ad%x1e`.
pub fn analyze_log(raw: &str, config: &Config) -> Vec<CommitAnalysis> {
    raw.split('\x1e')
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let mut fields = record.trim().split('\x1f');
            let hash = fields.next()?.trim().to_string();
            let message = fields.next()?.trim().to_string();
            let author = fields.next()?.trim().to_string();
            let date = fields.next().unwrap_or("").trim().to_string();
            Some(CommitAnalysis {
                errors: linter::validate(&message, config),
                hash,
                message,
                author,
                date,
            })
        })
        .collect()
}

pub fn format_report(analyses: &[CommitAnalysis]) -> String {
    let invalid: Vec<&CommitAnalysis> = analyses.iter().filter(|a| !a.is_valid()).collect();
    let valid = analyses.len() - invalid.len();

    let mut report = String::new();
    report.push_str("\nCommit History Analysis\n");
    report.push_str(&format!("   Total commits analyzed: {}\n", analyses.len()));
    report.push_str(&format!("   Valid: {}\n", valid));
    report.push_str(&format!("   Invalid: {}\n\n", invalid.len()));

    if invalid.is_empty() {
        report.push_str("All commits follow conventional commit format!\n");
        return report;
    }

    report.push_str("Non-conventional commits:\n\n");
    for commit in invalid {
        report.push_str(&format!(
            "  {} - {} ({})\n",
            commit.hash, commit.author, commit.date
        ));
        report.push_str(&format!("  Message: \"{}\"\n", commit.message));
        report.push_str("  Issues:\n");
        for error in &commit.errors {
            report.push_str(&format!("    • {}\n", error.message));
        }
        report.push('\n');
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "abc1234\x1ffeat: add endpoint\x1fOzan\x1f2026-01-01\x1edef5678\x1fupdate stuff\x1fDodo\x1f2026-01-02\x1e";

    #[test]
    fn parses_git_log_records() {
        let cfg = Config::default();
        let analyses = analyze_log(LOG, &cfg);
        assert_eq!(analyses.len(), 2);
        assert_eq!(analyses[0].hash, "abc1234");
        assert_eq!(analyses[0].message, "feat: add endpoint");
        assert_eq!(analyses[0].author, "Ozan");
        assert_eq!(analyses[0].date, "2026-01-01");
        assert!(analyses[0].is_valid());
        assert!(!analyses[1].is_valid());
    }

    #[test]
    fn ignores_blank_records() {
        assert!(analyze_log("\x1e \x1e", &Config::default()).is_empty());
    }

    #[test]
    fn report_counts_valid_invalid() {
        let analyses = analyze_log(LOG, &Config::default());
        let report = format_report(&analyses);
        assert!(report.contains("Total commits analyzed: 2"));
        assert!(report.contains("Valid: 1"));
        assert!(report.contains("Invalid: 1"));
        assert!(report.contains("def5678"));
        assert!(report.contains("format"));
    }

    #[test]
    fn report_all_valid() {
        let analyses = analyze_log(
            "abc\x1ffeat: add endpoint\x1fOzan\x1f2026-01-01\x1e",
            &Config::default(),
        );
        assert!(format_report(&analyses).contains("All commits follow"));
    }
}
