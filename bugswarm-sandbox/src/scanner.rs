use regex::Regex;

use crate::config;
use crate::error::SandboxResult;

/// Scanner for PII, secrets, and escape attempts in sandbox inputs and outputs.
pub struct OutputScanner {
    pii_patterns: Vec<(String, Regex)>,
    escape_patterns: Vec<Regex>,
}

impl OutputScanner {
    pub fn new() -> SandboxResult<Self> {
        let pii_patterns: Result<Vec<_>, _> = config::PII_PATTERNS
            .iter()
            .map(|(name, pattern)| {
                Regex::new(pattern)
                    .map(|re| (name.to_string(), re))
                    .map_err(|e| format!("Invalid PII pattern '{}': {}", name, e))
            })
            .collect();

        let pii_patterns = pii_patterns.map_err(|e| {
            crate::error::SandboxError::Other(format!("Failed to compile PII patterns: {}", e))
        })?;

        let escape_patterns: Result<Vec<_>, _> = config::ESCAPE_PATTERNS
            .iter()
            .map(|pattern| {
                let escaped = regex::escape(pattern);
                Regex::new(&escaped).map_err(|e| {
                    format!("Invalid escape pattern '{}': {}", pattern, e)
                })
            })
            .collect();

        let escape_patterns = escape_patterns.map_err(|e| {
            crate::error::SandboxError::Other(format!("Failed to compile escape patterns: {}", e))
        })?;

        Ok(Self {
            pii_patterns,
            escape_patterns,
        })
    }

    /// Scan content for PII. Returns (redacted_content, count_of_redactions).
    pub fn scan_pii(&self, content: &str) -> (String, usize) {
        let mut redacted = content.to_string();
        let mut count = 0;

        for (pii_type, pattern) in &self.pii_patterns {
            // Collect match ranges first to avoid borrow conflicts
            let ranges: Vec<(usize, usize)> = pattern
                .find_iter(&redacted)
                .map(|m| (m.start(), m.end()))
                .collect();
            count += ranges.len();

            // Replace from end to start to preserve indices
            for (start, end) in ranges.iter().rev() {
                let replacement = format!("[REDACTED: type={}]", pii_type);
                redacted.replace_range(*start..*end, &replacement);
            }
        }

        (redacted, count)
    }

    /// Scan PoC code for escape attempt patterns.
    pub fn scan_escape_attempts(&self, code: &str) -> Vec<String> {
        self.escape_patterns
            .iter()
            .filter_map(|pattern| {
                pattern.find(code).map(|m| m.as_str().to_string())
            })
            .collect()
    }

    /// Determine if a file path matches a sensitive pattern.
    pub fn is_sensitive_path(path: &str) -> bool {
        let path_lower = path.to_lowercase();
        config::SENSITIVE_PATH_PATTERNS
            .iter()
            .any(|pattern| path_lower.contains(pattern))
    }

    /// Determine if a variable name is sensitive (for probe filtering).
    pub fn is_sensitive_variable(name: &str) -> bool {
        let name_lower = name.to_lowercase();
        config::SENSITIVE_VAR_PATTERNS
            .iter()
            .any(|pattern| name_lower.contains(pattern))
    }

    /// Compute Shannon entropy of a string to detect high-entropy tokens (API keys, etc.).
    pub fn shannon_entropy(s: &str) -> f64 {
        if s.is_empty() {
            return 0.0;
        }

        let mut freq = [0usize; 256];
        let len = s.len() as f64;

        for byte in s.bytes() {
            freq[byte as usize] += 1;
        }

        freq.iter()
            .filter(|&&count| count > 0)
            .map(|&count| {
                let p = count as f64 / len;
                -p * p.log2()
            })
            .sum()
    }

    /// Detect high-entropy strings (> 4.5 shannon entropy) over 20 chars — likely API keys.
    pub fn detect_high_entropy_tokens(&self, content: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        for word in content.split_whitespace() {
            if word.len() > 20 && Self::shannon_entropy(word) > 4.5 {
                tokens.push(word.to_string());
            }
        }
        tokens
    }
}

impl Default for OutputScanner {
    fn default() -> Self {
        Self::new().expect("Failed to create default OutputScanner")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanner() -> OutputScanner {
        OutputScanner::new().unwrap()
    }

    #[test]
    fn test_scan_email() {
        let s = scanner();
        let input = "Contact user@example.com for help";
        let (redacted, count) = s.scan_pii(input);
        assert_eq!(count, 1);
        assert!(!redacted.contains("user@example.com"));
        assert!(redacted.contains("[REDACTED: type=email]"));
    }

    #[test]
    fn test_scan_credit_card() {
        let s = scanner();
        let input = "Card: 4111-1111-1111-1111 was charged";
        let (redacted, count) = s.scan_pii(input);
        assert_eq!(count, 1);
        assert!(!redacted.contains("4111-1111-1111-1111"));
    }

    #[test]
    fn test_scan_no_pii() {
        let s = scanner();
        let input = "Normal log output with no personal data";
        let (redacted, count) = s.scan_pii(input);
        assert_eq!(count, 0);
        assert_eq!(redacted, input);
    }

    #[test]
    fn test_scan_aws_key() {
        let s = scanner();
        let input = "AWS key: AKIAIOSFODNN7EXAMPLE";
        let (redacted, count) = s.scan_pii(input);
        assert_eq!(count, 1);
        assert!(!redacted.contains("AKIA"));
    }

    #[test]
    fn test_escape_detection() {
        let s = scanner();
        let code = "system('chroot /tmp/evil')";
        let matches = s.scan_escape_attempts(code);
        assert!(!matches.is_empty());
        assert!(matches.contains(&"chroot".to_string()));
    }

    #[test]
    fn test_escape_detection_clean() {
        let s = scanner();
        let code = "print('hello world')";
        let matches = s.scan_escape_attempts(code);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_sensitive_path() {
        assert!(OutputScanner::is_sensitive_path("/app/.env"));
        assert!(OutputScanner::is_sensitive_path("id_rsa"));
        assert!(!OutputScanner::is_sensitive_path("/app/main.py"));
    }

    #[test]
    fn test_sensitive_variable() {
        assert!(OutputScanner::is_sensitive_variable("password"));
        assert!(OutputScanner::is_sensitive_variable("API_KEY"));
        assert!(!OutputScanner::is_sensitive_variable("user_name"));
    }

    #[test]
    fn test_shannon_entropy() {
        let low = "aaaaaaa";
        let high = "xK9#mP2$vL7@qR4!wN8^";
        assert!(OutputScanner::shannon_entropy(high) > OutputScanner::shannon_entropy(low));
        assert!(OutputScanner::shannon_entropy(high) > 3.0);
    }

    #[test]
    fn test_multiple_pii_types() {
        let s = scanner();
        let input = "Email: alice@example.com, Card: 4111-1111-1111-1111, Phone: 555-123-4567";
        let (_redacted, count) = s.scan_pii(input);
        assert!(count >= 2);
    }
}
