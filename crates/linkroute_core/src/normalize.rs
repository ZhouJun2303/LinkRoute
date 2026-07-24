use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizeError {
    EmptyDomain,
    InvalidDomain(String),
    EmptyPath,
    NotExecutable(String),
}

impl fmt::Display for NormalizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDomain => write!(f, "domain is empty"),
            Self::InvalidDomain(value) => write!(f, "domain is invalid: {value}"),
            Self::EmptyPath => write!(f, "path is empty"),
            Self::NotExecutable(value) => write!(f, "path is not an .exe: {value}"),
        }
    }
}

impl std::error::Error for NormalizeError {}

pub fn normalize_domain(input: &str) -> Result<String, NormalizeError> {
    let mut value = input.trim().trim_end_matches('.').to_lowercase();
    if let Some(stripped) = value.strip_prefix("*.") {
        value = stripped.to_string();
    }

    if value.is_empty() {
        return Err(NormalizeError::EmptyDomain);
    }
    if value.len() > 253
        || value.contains("://")
        || value.contains('/')
        || value.contains('\\')
        || value.contains(':')
        || value.split('.').any(|part| part.is_empty() || part.len() > 63)
        || value.chars().any(char::is_whitespace)
    {
        return Err(NormalizeError::InvalidDomain(input.to_string()));
    }

    Ok(value)
}

pub fn domain_matches(rule_domain: &str, candidate: &str, suffix: bool) -> bool {
    let Ok(rule_domain) = normalize_domain(rule_domain) else {
        return false;
    };
    let Ok(candidate) = normalize_domain(candidate) else {
        return false;
    };

    if candidate == rule_domain {
        return true;
    }
    suffix && candidate.ends_with(&format!(".{rule_domain}"))
}

pub fn normalize_exe_path(input: &str) -> Result<String, NormalizeError> {
    let value = input.trim().trim_matches('"').replace('/', "\\");
    if value.is_empty() {
        return Err(NormalizeError::EmptyPath);
    }
    if !value.to_ascii_lowercase().ends_with(".exe") {
        return Err(NormalizeError::NotExecutable(input.to_string()));
    }
    Ok(value)
}

pub fn exe_paths_equal(left: &str, right: &str) -> bool {
    let Ok(left) = normalize_exe_path(left) else {
        return false;
    };
    let Ok(right) = normalize_exe_path(right) else {
        return false;
    };
    left.eq_ignore_ascii_case(&right)
}
