# BugSwarm

Automated bug hunting pipeline with CPG indexing, sandbox verification, evidence graph assembly, and multi-agent adjudication.

## System Requirements
- Linux x86_64 (kernel 5.10+)
- Docker Engine 24.0+
- Python 3.11+
- Rust 1.75+

## Z3 Solver (required for symbolic/concolic execution)
```bash
# Ubuntu/Debian
apt-get install -y libz3-dev z3

# macOS
brew install z3

# Verify
z3 --version
```
