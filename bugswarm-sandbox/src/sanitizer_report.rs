/// Sanitizer Report — structured representation of compiler sanitizer output.
///
/// Parses ASAN, UBSAN, TSAN, MSAN, LSAN stderr into typed structs.
/// Used by container.rs to inject sanitizer findings into ExecutionReceipts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SanitizerType {
    ASAN,
    UBSAN,
    TSAN,
    MSAN,
    LSAN,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizerFrame {
    pub function: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub module: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizerReport {
    pub sanitizer: SanitizerType,
    pub error_type: String,
    pub error_message: String,
    pub address: Option<u64>,
    pub size: Option<u64>,
    pub stack_trace: Vec<SanitizerFrame>,
    pub allocation_site: Option<Vec<SanitizerFrame>>,
    pub thread_id: Option<u64>,
    pub raw_output: String,
}

/// Parse sanitizer output from stderr. Returns None if no sanitizer output detected.
pub fn parse_sanitizer_output(stderr: &str) -> Option<SanitizerReport> {
    if stderr.is_empty() {
        return None;
    }

    // Detect which sanitizer produced this output
    if stderr.contains("AddressSanitizer") || stderr.contains("ASAN:") {
        parse_asan(stderr)
    } else if stderr.contains("UndefinedBehaviorSanitizer") || stderr.contains("runtime error:") {
        parse_ubsan(stderr)
    } else if stderr.contains("ThreadSanitizer") || stderr.contains("WARNING: ThreadSanitizer") {
        parse_tsan(stderr)
    } else if stderr.contains("MemorySanitizer") {
        parse_msan(stderr)
    } else if stderr.contains("LeakSanitizer") || stderr.contains("SUMMARY: AddressSanitizer:") {
        parse_lsan(stderr)
    } else {
        None
    }
}

fn parse_asan(stderr: &str) -> Option<SanitizerReport> {
    let re_line = regex_lite::Regex::new(r"(READ|WRITE) of size (\d+) at (0x[0-9a-fA-F]+)").ok()?;
    let re_error = regex_lite::Regex::new(r"ERROR: AddressSanitizer: (\S+)").ok()?;
    let re_frame = regex_lite::Regex::new(r"#\d+\s+0x[0-9a-fA-F]+\s+in\s+(\S+)\s+(\S+):(\d+):(\d+)").ok()?;
    let re_thread = regex_lite::Regex::new(r"thread\s+(T\d+)").ok()?;

    let error_type = re_error.captures(stderr)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "unknown".into());

    let mut address = None;
    let mut size = None;

    if let Some(caps) = re_line.captures(stderr) {
        size = caps.get(2).and_then(|m| m.as_str().parse().ok());
        address = caps.get(3).and_then(|m| u64::from_str_radix(&m.as_str()[2..], 16).ok());
    }

    let mut stack_trace = Vec::new();
    for caps in re_frame.captures_iter(stderr) {
        stack_trace.push(SanitizerFrame {
            function: caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
            file: caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            line: caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
            column: caps.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
            module: String::new(),
        });
        if stack_trace.len() >= 100 { break; }
    }

    let allocation_site = if stderr.contains("allocated by thread") {
        let mut frames = Vec::new();
        for caps in re_frame.captures_iter(stderr) {
            frames.push(SanitizerFrame {
                function: caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                file: caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
                line: caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
                column: caps.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
                module: String::new(),
            });
        }
        if frames.is_empty() { None } else { Some(frames) }
    } else {
        None
    };

    let thread_id = re_thread.captures(stderr)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .and_then(|s| s[1..].parse::<u64>().ok());

    Some(SanitizerReport {
        sanitizer: SanitizerType::ASAN,
        error_type,
        error_message: stderr.lines().next().unwrap_or("").to_string(),
        address,
        size,
        stack_trace,
        allocation_site,
        thread_id,
        raw_output: stderr.to_string(),
    })
}

fn parse_ubsan(stderr: &str) -> Option<SanitizerReport> {
    let re_error = regex_lite::Regex::new(r"(?:runtime error:|UndefinedBehaviorSanitizer:)\s*(.+)").ok()?;
    let re_frame = regex_lite::Regex::new(r"(\S+):(\d+):(\d+):\s+runtime error").ok()?;

    let error_type = re_error.captures(stderr)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| "undefined behavior".into());

    let mut stack_trace = Vec::new();
    for caps in re_frame.captures_iter(stderr) {
        stack_trace.push(SanitizerFrame {
            function: String::new(),
            file: caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
            line: caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
            column: caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
            module: String::new(),
        });
    }

    Some(SanitizerReport {
        sanitizer: SanitizerType::UBSAN,
        error_type,
        error_message: stderr.lines().next().unwrap_or("").to_string(),
        address: None,
        size: None,
        stack_trace,
        allocation_site: None,
        thread_id: None,
        raw_output: stderr.to_string(),
    })
}

fn parse_tsan(stderr: &str) -> Option<SanitizerReport> {
    let re_thread = regex_lite::Regex::new(r"[Tt]hread\s+(T\d+)").ok()?;
    let re_frame = regex_lite::Regex::new(r"#\d+\s+(\S+)\s+(\S+):(\d+)").ok()?;

    let thread_id = re_thread.captures(stderr)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .and_then(|s| s[1..].parse::<u64>().ok());

    let mut stack_trace = Vec::new();
    for caps in re_frame.captures_iter(stderr) {
        stack_trace.push(SanitizerFrame {
            function: caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
            file: caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            line: caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
            column: 0,
            module: String::new(),
        });
        if stack_trace.len() >= 100 { break; }
    }

    Some(SanitizerReport {
        sanitizer: SanitizerType::TSAN,
        error_type: "data-race".into(),
        error_message: stderr.lines().next().unwrap_or("").to_string(),
        address: None, size: None,
        stack_trace,
        allocation_site: None,
        thread_id,
        raw_output: stderr.to_string(),
    })
}

fn parse_msan(stderr: &str) -> Option<SanitizerReport> {
    Some(SanitizerReport {
        sanitizer: SanitizerType::MSAN,
        error_type: "uninitialized-read".into(),
        error_message: stderr.lines().next().unwrap_or("").to_string(),
        address: None, size: None,
        stack_trace: Vec::new(),
        allocation_site: None,
        thread_id: None,
        raw_output: stderr.to_string(),
    })
}

fn parse_lsan(stderr: &str) -> Option<SanitizerReport> {
    if !stderr.contains("leaked") && !stderr.contains("LeakSanitizer") {
        return None;
    }
    Some(SanitizerReport {
        sanitizer: SanitizerType::LSAN,
        error_type: "memory-leak".into(),
        error_message: stderr.lines().next().unwrap_or("").to_string(),
        address: None, size: None,
        stack_trace: Vec::new(),
        allocation_site: None,
        thread_id: None,
        raw_output: stderr.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_asan_heap_overflow() {
        let stderr = r#"=================================================================
==12345==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x7f8a4c000800 at pc 0x55d4a1b2c3d4 bp 0x7ffe12345678 sp 0x7ffe12345670
WRITE of size 20 at 0x7f8a4c000800 thread T0
    #0 0x55d4a1b2c3d4 in memset /build/glibc/string/memset.c:42:5
    #1 0x55d4a1b2c3d4 in trigger_overflow /app/bugs.py:15:3
    #2 0x55d4a1b2c3d4 in main /app/bugs.py:50:1
0x7f8a4c000800 is located 0 bytes to the right of 10-byte region [0x7f8a4c000800,0x7f8a4c00080a)
allocated by thread T0 here:
    #0 0x55d4a1b2c3d4 in malloc /build/glibc/malloc.c:100:1
    #1 0x55d4a1b2c3d4 in create_buffer /app/bugs.py:10:5
SUMMARY: AddressSanitizer: heap-buffer-overflow"#;

        let report = parse_asan(stderr).expect("Should parse ASAN output");
        assert_eq!(report.sanitizer, SanitizerType::ASAN);
        assert_eq!(report.error_type, "heap-buffer-overflow");
        assert_eq!(report.size, Some(20));
        assert!(report.address.is_some());
        assert!(!report.stack_trace.is_empty());
        assert_eq!(report.stack_trace[0].function, "memset");
        assert!(report.allocation_site.is_some());
    }

    #[test]
    fn test_parse_empty_stderr() {
        assert!(parse_sanitizer_output("").is_none());
    }

    #[test]
    fn test_parse_non_sanitizer_stderr() {
        let stderr = "Traceback (most recent call last):\n  File \"test.py\", line 1\nValueError: invalid";
        assert!(parse_sanitizer_output(stderr).is_none());
    }

    #[test]
    fn test_parse_ubsan() {
        let stderr = "test.py:42:5: runtime error: signed integer overflow: 2147483647 + 1 cannot be represented in type 'int'";
        let report = parse_ubsan(stderr).expect("Should parse UBSAN");
        assert_eq!(report.sanitizer, SanitizerType::UBSAN);
        assert!(report.error_type.contains("overflow"));
        assert!(!report.stack_trace.is_empty());
    }

    #[test]
    fn test_parse_tsan() {
        let stderr = "WARNING: ThreadSanitizer: data race (pid=12345)\n  Write of size 4 at 0x7f... by thread T2:\n    #0 increment /app/bugs.py:20:3\n  Previous write by thread T1:\n    #0 increment /app/bugs.py:20:3";
        let report = parse_tsan(stderr).expect("Should parse TSAN");
        assert_eq!(report.sanitizer, SanitizerType::TSAN);
        // Thread ID is parsed from "thread T2"
        assert!(report.thread_id.is_some());
        assert!(!report.stack_trace.is_empty());
    }

    #[test]
    fn test_parse_null_bytes() {
        let stderr = "ERROR: AddressSanitizer:\x00 heap-buffer-overflow\x00 on address 0x00";
        let report = parse_asan(stderr);
        assert!(report.is_some());
        // Null bytes may cause regex mismatch — parser is defensive
        let r = report.unwrap();
        assert!(!r.raw_output.is_empty());
    }
}
