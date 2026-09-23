use nyar_package_registry::VersionBump;

use crate::{PackageManagerError, Result};

/// Semantic version (`major.minor.patch` with optional pre-release suffix).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

/// Yearly version (`yearly.major.minor.patch`) used by packages such as `std`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct YearlyVersion {
    pub yearly: u64,
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedVersion {
    Yearly(YearlyVersion),
    Semantic(SemanticVersion),
}

impl SemanticVersion {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim().trim_start_matches('v');
        let (core, pre) = split_version_core(value)?;
        let mut parts = core.split('.');
        let major = parts
            .next()
            .ok_or_else(|| PackageManagerError::message(format!("invalid version: {value}")))?
            .parse()
            .map_err(|_| PackageManagerError::message(format!("invalid version: {value}")))?;
        let minor = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let patch = parts.next().unwrap_or("0").parse().unwrap_or(0);
        Ok(Self { major, minor, patch, pre })
    }

    pub fn bump(&self, bump: VersionBump) -> Self {
        match bump {
            VersionBump::Patch => Self { major: self.major, minor: self.minor, patch: self.patch + 1, pre: None },
            VersionBump::Minor => Self { major: self.major, minor: self.minor + 1, patch: 0, pre: None },
            VersionBump::Major => Self { major: self.major + 1, minor: 0, patch: 0, pre: None },
        }
    }
}

impl YearlyVersion {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim().trim_start_matches('v');
        let (core, pre) = split_version_core(value)?;
        if pre.is_some() {
            return Err(PackageManagerError::message(format!("invalid yearly version: {value}")));
        }
        let parts = parse_numeric_parts(&core, value)?;
        if parts.len() != 4 {
            return Err(PackageManagerError::message(format!("invalid yearly version: {value}")));
        }
        Ok(Self { yearly: parts[0], major: parts[1], minor: parts[2], patch: parts[3] })
    }
}

impl std::fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for YearlyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.yearly, self.major, self.minor, self.patch)
    }
}

fn split_version_core(value: &str) -> Result<(String, Option<String>)> {
    let value = value.trim().trim_start_matches('v');
    let (core, pre) = match value.split_once('-') {
        Some((core, pre)) => (core.to_string(), Some(pre.to_string())),
        None => (value.to_string(), None),
    };
    Ok((core, pre))
}

fn parse_numeric_parts(core: &str, original: &str) -> Result<Vec<u64>> {
    if core.is_empty() {
        return Err(PackageManagerError::message(format!("invalid version: {original}")));
    }
    let mut parts = Vec::new();
    for part in core.split('.') {
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(PackageManagerError::message(format!("invalid version: {original}")));
        }
        parts.push(part.parse().map_err(|_| PackageManagerError::message(format!("invalid version: {original}")))?);
    }
    Ok(parts)
}

fn parse_version(value: &str) -> Result<ParsedVersion> {
    let value = value.trim().trim_start_matches('v');
    let (core, _) = split_version_core(value)?;
    let parts = parse_numeric_parts(&core, value)?;
    if parts.len() == 4 && parts[0] >= 1000 {
        Ok(ParsedVersion::Yearly(YearlyVersion { yearly: parts[0], major: parts[1], minor: parts[2], patch: parts[3] }))
    }
    else {
        Ok(ParsedVersion::Semantic(SemanticVersion::parse(value)?))
    }
}

/// Whether `version` satisfies a version constraint.
///
/// Supported: `*`, `latest`, exact, `^`, `~`, wildcard (`*.*`), and two-part shorthand.
/// Unsupported: `>=`, `<=`, `<`, `>` and compound ranges.
pub fn satisfies_version_constraint(version: &str, constraint: &str) -> bool {
    let version = version.trim().trim_start_matches('v');
    let constraint = constraint.trim();
    if constraint.is_empty() || constraint == "*" || constraint == "latest" {
        return true;
    }
    if constraint.starts_with(">=") || constraint.starts_with("<=") || constraint.starts_with('<') || constraint.starts_with('>') {
        return false;
    }
    if constraint.ends_with(".*") {
        return satisfies_wildcard(version, constraint);
    }
    if let Some(base) = constraint.strip_prefix('^') {
        return satisfies_caret(version, base);
    }
    if let Some(base) = constraint.strip_prefix('~') {
        return satisfies_tilde(version, base);
    }
    if version == constraint.trim_start_matches('v') {
        return true;
    }
    satisfies_minor_line(version, constraint)
}

fn satisfies_wildcard(version: &str, constraint: &str) -> bool {
    let prefix = constraint.strip_suffix(".*").unwrap_or(constraint);
    let Ok(actual) = parse_version(version)
    else {
        return false;
    };
    let parts: Vec<&str> = prefix.split('.').collect();
    if parts.iter().any(|part| part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit())) {
        return false;
    }
    match actual {
        ParsedVersion::Yearly(actual) => match parts.len() {
            1 => parts[0].parse::<u64>().ok() == Some(actual.yearly),
            2 => {
                let Ok(yearly) = parts[0].parse::<u64>()
                else {
                    return false;
                };
                let Ok(major) = parts[1].parse::<u64>()
                else {
                    return false;
                };
                actual.yearly == yearly && actual.major == major
            }
            3 => {
                let Ok(yearly) = parts[0].parse::<u64>()
                else {
                    return false;
                };
                let Ok(major) = parts[1].parse::<u64>()
                else {
                    return false;
                };
                let Ok(minor) = parts[2].parse::<u64>()
                else {
                    return false;
                };
                actual.yearly == yearly && actual.major == major && actual.minor == minor
            }
            4 => {
                let Ok(yearly) = parts[0].parse::<u64>()
                else {
                    return false;
                };
                let Ok(major) = parts[1].parse::<u64>()
                else {
                    return false;
                };
                let Ok(minor) = parts[2].parse::<u64>()
                else {
                    return false;
                };
                let Ok(patch) = parts[3].parse::<u64>()
                else {
                    return false;
                };
                actual.yearly == yearly && actual.major == major && actual.minor == minor && actual.patch == patch
            }
            _ => false,
        },
        ParsedVersion::Semantic(actual) => match parts.len() {
            1 => parts[0].parse::<u64>().ok() == Some(actual.major),
            2 => {
                let Ok(major) = parts[0].parse::<u64>()
                else {
                    return false;
                };
                let Ok(minor) = parts[1].parse::<u64>()
                else {
                    return false;
                };
                actual.major == major && actual.minor == minor
            }
            3 => {
                let Ok(major) = parts[0].parse::<u64>()
                else {
                    return false;
                };
                let Ok(minor) = parts[1].parse::<u64>()
                else {
                    return false;
                };
                let Ok(patch) = parts[2].parse::<u64>()
                else {
                    return false;
                };
                actual.major == major && actual.minor == minor && actual.patch == patch
            }
            _ => false,
        },
    }
}

fn satisfies_minor_line(version: &str, constraint: &str) -> bool {
    let parts: Vec<&str> = constraint.split('.').collect();
    if parts.len() != 2 {
        return false;
    }
    if !parts.iter().all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit())) {
        return false;
    }
    satisfies_wildcard(version, &format!("{constraint}.*"))
}

fn satisfies_caret(version: &str, base: &str) -> bool {
    let Ok(actual) = parse_version(version)
    else {
        return false;
    };
    let Ok(expected) = parse_version(base)
    else {
        return false;
    };
    match (actual, expected) {
        (ParsedVersion::Yearly(actual), ParsedVersion::Yearly(expected)) => {
            if expected.major > 0 {
                actual.yearly == expected.yearly && actual.major == expected.major && actual >= expected
            }
            else if expected.minor > 0 {
                actual.yearly == expected.yearly && actual.major == 0 && actual.minor == expected.minor && actual >= expected
            }
            else {
                actual.yearly == expected.yearly && actual.major == 0 && actual.minor == 0 && actual.patch == expected.patch
            }
        }
        (ParsedVersion::Semantic(actual), ParsedVersion::Semantic(expected)) => {
            if expected.major > 0 {
                actual.major == expected.major && actual >= expected
            }
            else if expected.minor > 0 {
                actual.major == 0 && actual.minor == expected.minor && actual >= expected
            }
            else {
                actual.major == 0 && actual.minor == 0 && actual.patch == expected.patch
            }
        }
        _ => false,
    }
}

fn satisfies_tilde(version: &str, base: &str) -> bool {
    let Ok(actual) = parse_version(version)
    else {
        return false;
    };
    let Ok(expected) = parse_version(base)
    else {
        return false;
    };
    match (actual, expected) {
        (ParsedVersion::Yearly(actual), ParsedVersion::Yearly(expected)) => {
            actual.yearly == expected.yearly && actual.major == expected.major && actual.minor == expected.minor && actual >= expected
        }
        (ParsedVersion::Semantic(actual), ParsedVersion::Semantic(expected)) => {
            actual.major == expected.major && actual.minor == expected.minor && actual >= expected
        }
        _ => false,
    }
}

impl PartialOrd for SemanticVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemanticVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch, &self.pre).cmp(&(other.major, other.minor, other.patch, &other.pre))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_and_tilde_constraints() {
        assert!(satisfies_version_constraint("1.2.3", "^1.0.0"));
        assert!(!satisfies_version_constraint("2.0.0", "^1.0.0"));
        assert!(satisfies_version_constraint("1.2.9", "~1.2.0"));
        assert!(!satisfies_version_constraint("1.3.0", "~1.2.0"));
    }

    #[test]
    fn wildcard_and_minor_line_constraints() {
        assert!(satisfies_version_constraint("0.1.0.0", "0.1.*"));
        assert!(satisfies_version_constraint("0.1.3", "0.1"));
        assert!(!satisfies_version_constraint("0.2.0", "0.1.*"));
        assert!(satisfies_version_constraint("2025.4.1", "2025.*"));
    }

    #[test]
    fn yearly_version_constraints() {
        assert!(satisfies_version_constraint("2020.0.0.0", "2020.*"));
        assert!(satisfies_version_constraint("2020.0.1.0", "2020.0.*"));
        assert!(satisfies_version_constraint("2020.0.0.2", "2020.0.0.*"));
        assert!(satisfies_version_constraint("2020.0.0.2", "2020.0"));
        assert!(!satisfies_version_constraint("2020.0.0.1", "2020.1.*"));
        assert!(!satisfies_version_constraint("2021.0.0.0", "2020.*"));
        assert_eq!(YearlyVersion::parse("2020.0.0.0").unwrap(), YearlyVersion { yearly: 2020, major: 0, minor: 0, patch: 0 });
    }

    #[test]
    fn comparator_constraints_are_rejected() {
        assert!(!satisfies_version_constraint("0.1.0", ">=0.1.0"));
        assert!(!satisfies_version_constraint("0.1.0", "<2.0.0"));
    }
}
