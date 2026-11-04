use crate::types::Config;

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub rule: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCommit {
    pub ty: String,
    pub scope: Option<String>,
    pub description: String,
    pub raw: String,
}

impl ValidationError {
    fn new(rule: &str, message: String) -> Self {
        Self {
            rule: rule.into(),
            message,
        }
    }
}

/// Parse `type(scope): description` or `type: description`.
pub fn parse(message: &str) -> Option<ParsedCommit> {
    let trimmed = message.trim();
    let colon = trimmed.find(':')?;
    let head = &trimmed[..colon];
    let description = trimmed[colon + 1..].trim();
    if description.is_empty() {
        return None;
    }

    let (ty, scope) = if let Some(rest) = head.strip_suffix(')') {
        let open = rest.find('(')?;
        let scope = &rest[open + 1..];
        if scope.is_empty() || scope.contains('(') {
            return None;
        }
        (&rest[..open], Some(scope.to_string()))
    } else {
        (head, None)
    };

    if ty.is_empty() || !ty.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }

    Some(ParsedCommit {
        ty: ty.to_string(),
        scope,
        description: description.to_string(),
        raw: trimmed.to_string(),
    })
}

pub fn validate(message: &str, config: &Config) -> Vec<ValidationError> {
    let Some(parsed) = parse(message) else {
        return vec![ValidationError::new(
            "format",
            "Commit message must follow the format: type(scope): description".into(),
        )];
    };

    let mut errors = Vec::new();
    let rules = &config.rules;

    if !rules.types.is_empty() && !rules.types.contains(&parsed.ty) {
        errors.push(ValidationError::new(
            "type",
            format!(
                "Type \"{}\" is not allowed. Allowed types: {}",
                parsed.ty,
                rules.types.join(", ")
            ),
        ));
    }

    if rules.requireScope && parsed.scope.is_none() {
        errors.push(ValidationError::new("scope", "Scope is required".into()));
    }
    if let Some(scope) = &parsed.scope
        && !rules.scopes.is_empty()
        && !rules.scopes.contains(scope)
    {
        errors.push(ValidationError::new(
            "scope",
            format!(
                "Scope \"{}\" is not allowed. Allowed scopes: {}",
                scope,
                rules.scopes.join(", ")
            ),
        ));
    }

    let len = parsed.raw.chars().count();
    if rules.maxLength > 0 && len > rules.maxLength {
        errors.push(ValidationError::new(
            "max-length",
            format!("Commit message is too long ({} > {})", len, rules.maxLength),
        ));
    }
    if rules.minLength > 0 && len < rules.minLength {
        errors.push(ValidationError::new(
            "min-length",
            format!(
                "Commit message is too short ({} < {})",
                len, rules.minLength
            ),
        ));
    }

    let first = parsed.description.chars().next();
    if let Some(first) = first
        && first.to_lowercase().to_string() != first.to_string()
    {
        errors.push(ValidationError::new(
            "description-case",
            "Description must start with lowercase".into(),
        ));
    }
    if parsed.description.ends_with('.') {
        errors.push(ValidationError::new(
            "description-period",
            "Description must not end with a period".into(),
        ));
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Config, Rules};

    fn cfg() -> Config {
        Config::default()
    }

    #[test]
    fn parses_type_and_optional_scope() {
        let p = parse("feat(api): add endpoint").unwrap();
        assert_eq!(p.ty, "feat");
        assert_eq!(p.scope.as_deref(), Some("api"));
        assert_eq!(p.description, "add endpoint");

        let p = parse("fix: resolve crash").unwrap();
        assert_eq!(p.ty, "fix");
        assert_eq!(p.scope, None);
    }

    #[test]
    fn rejects_malformed_messages() {
        assert!(parse("no commit format").is_none());
        assert!(parse("feat(api):").is_none());
        assert!(parse("feat(: desc").is_none());
        assert!(parse(": desc").is_none());
    }

    #[test]
    fn accepts_valid_commit() {
        assert!(validate("feat(api): add endpoint", &cfg()).is_empty());
    }

    #[test]
    fn rejects_disallowed_type() {
        let mut c = cfg();
        c.rules.types = vec!["feat".into()];
        let errs = validate("fix: resolve crash", &c);
        assert_eq!(errs[0].rule, "type");
        assert!(errs[0].message.contains("fix"));
    }

    #[test]
    fn scope_required_and_allowlist() {
        let mut c = cfg();
        c.rules.requireScope = true;
        assert_eq!(validate("feat: add x", &c)[0].rule, "scope");

        let mut c = cfg();
        c.rules.scopes = vec!["api".into()];
        let errs = validate("feat(ui): add x", &c);
        assert_eq!(errs[0].rule, "scope");
    }

    #[test]
    fn length_limits() {
        let mut c = cfg();
        c.rules.minLength = 20;
        let errs = validate("feat: add x", &c);
        assert_eq!(errs[0].rule, "min-length");

        let mut c = Config::default();
        c.rules.maxLength = 12;
        let errs = validate("feat: add something long", &c);
        assert_eq!(errs[0].rule, "max-length");
    }

    #[test]
    fn description_case_and_period() {
        let errs = validate("feat: Add endpoint.", &cfg());
        let rules: Vec<&str> = errs.iter().map(|e| e.rule.as_str()).collect();
        assert!(rules.contains(&"description-case"));
        assert!(rules.contains(&"description-period"));
    }

    #[test]
    fn format_error_for_unparseable() {
        let errs = validate("wip", &cfg());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].rule, "format");
    }

    #[test]
    fn custom_defaults_are_used() {
        let rules = Rules::default();
        assert_eq!(rules.maxLength, 100);
        assert_eq!(rules.minLength, 10);
    }
}
