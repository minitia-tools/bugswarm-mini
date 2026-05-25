BugSwarm — Automated Bug Hunting Pipeline

```text
$ bugswarm --help

Usage:  bugswarm <COMMAND>

Commands:
  sandbox    Isolated PoC execution engine (Rust daemon)
  cpg        Code Property Graph indexing & taint analysis (Rust daemon)
  evidence   Evidence graph persistence & query (Rust daemon)
  gateway    LLM provider abstraction with failover (Python)
  agent      Multi-agent bug hunting swarm (Python)
  tui        Terminal UI for real-time swarm monitoring (Python)
  minitia    Engine orchestration platform (Python)
  config     View or validate unified config
  ci         CI/CD integration (SARIF export, PR annotations)

Options:
  -c, --config <PATH>     Config file [/etc/bugswarm/config.yaml]
      --json              Structured JSON output
  -v, --verbose...        Increase log verbosity
  -q, --quiet             Suppress non-fatal output
      --version           Print version
  -h, --help              Print help

  ▸ bugswarm v1.0.0 — Linux x86_64 — Rust 1.75+ / Python 3.11+
```

```text
$ bugswarm info


  ▞▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
  ▐                                                                                                                               ▐
  ▐   ARCHITECTURE                                                                                                                ▐
  ▐                                                                                                                               ▐
  ▐   ┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                                              ▐
  ▐   │  bugswarm-   │    │  bugswarm-   │    │  bugswarm-   │    │  bugswarm-   │                                              ▐
  ▐   │   sandbox    │───▶│     cpg      │───▶│  evidence    │───▶│    swarm     │                                              ▐
  ▐   │  (Rust daemon)│   │  (Rust daemon)│   │  (Rust daemon)│   │  (Python)    │                                              ▐
  ▐   └──────────────┘    └──────────────┘    └──────────────┘    └──────┬───────┘                                              ▐
  ▐        │                                                             │                                                        ▐
  ▐        │  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐   │                                                        ▐
  ▐        └──│  Docker      │    │  LLM         │    │  Minitia     │   │                                                        ▐
  ▐           │  containers  │    │  Gateway     │    │  Platform    │   │                                                        ▐
  ▐           └──────────────┘    └──────────────┘    └──────────────┘   │                                                        ▐
  ▐                                                                      ▼                                                        ▐
  ▐                                                              ┌──────────────┐                                               ▐
  ▐                                                              │     TUI      │                                               ▐
  ▐                                                              │  (Textual)   │                                               ▐
  ▐                                                              └──────────────┘                                               ▐
  ▐                                                                                                                               ▐
  ▐   SYSTEM REQUIREMENTS                                                                                                         ▐
  ▐   • Linux x86_64 (kernel 5.10+)    • Docker Engine 24.0+                                                                     ▐
  ▐   • Python 3.11+                   • Rust 1.75+                                                                              ▐
  ▐   • Z3 Solver (symbolic execution)                                                                                            ▐
  ▐                                                                                                                               ▐
  ▐   CRATES                                                                                                                      ▐
  ▐   bugswarm-sandbox   │ Containerized PoC execution, seccomp isolation, OOM/crash detection                                    ▐
  ▐   bugswarm-cpg       │ Multi-language CPG builder, taint tracking, call graph analysis                                       ▐
  ▐   bugswarm-evidence  │ Immutable evidence graph with WAL persistence, query, scoring                                          ▐
  ▐   bugswarm-symbolic  │ Z3-based symbolic/concolic execution engine                                                            ▐
  ▐   bugswarm-gateway   │ LLM provider adapter: OpenAI, Anthropic, Google, DeepSeek, Ollama w/ failover                          ▐
  ▐   bugswarm-agent     │ Multi-agent swarm: debate, adjudication, loop detection, budget enforcement                            ▐
  ▐   bugswarm-swarm     │ Orchestrator: state machine, WAL, checkpointing, TUI data model                                       ▐
  ▐   minitia            │ Engine registry, install/run/stop lifecycle, multi-engine dashboard                                    ▐
  ▐                                                                                                                               ▐
  ▐   DOCUMENTATION                                                                                                               ▐
  ▐   ./doc/AGENTS.md                    Enterprise quality standards & phase stress tests                                        ▐
  ▐   ./doc/CHANGELOG.md                 Release history                                                                          ▐
  ▐   ./doc/milestone.md                 Implementation milestones (M001-M035)                                                    ▐
  ▐   ./doc/invincible.md                7 missing layers: data flow, symbolic, fuzzing, etc.                                     ▐
  ▐   ./doc/bugswarm_strategic_analysis.md   Strategic roadmap                                                                     ▐
  ▐   ./doc/q1_all_bug_coverage.md .. q7_*  Quarter roadmap documents                                                            ▐
  ▐                                                                                                                               ▐
  ▞▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
  ▐
```

```bash
# ── Quick Start ──────────────────────────────────────────────────────────────

# 1. Install Z3 solver (symbolic execution)
$ apt install -y libz3-dev z3

# 2. Build all Rust daemons
$ cargo build --release

# 3. Start daemons (systemd or direct)
$ sudo ./target/release/bugswarm-sandbox run-server --http-port 8080
$ sudo ./target/release/bugswarm-cpg run-server --http-port 8081
$ sudo ./target/release/bugswarm-evidence run-server --http-port 8082

# 4. Install Python dependencies
$ pip install -r bugswarm-agent/requirements.txt

# 5. Launch a hunt
$ bugswarm agent run ./target-repo --budget 50 --swarm 12
```

```text
$ bugswarm config validate

  ✓ /etc/bugswarm/config.yaml — schema version 1
  ✓ daemons.sandbox — socket /var/run/bugswarm/sandbox.sock
  ✓ daemons.sandbox — http_port 8080
  ✓ daemons.cpg — socket /var/run/bugswarm/cpg.sock
  ✓ daemons.cpg — http_port 8081
  ✓ daemons.evidence — socket /var/run/bugswarm/evidence.sock
  ✓ daemons.evidence — http_port 8082
  ✓ daemons.evidence — state_dir /var/lib/bugswarm
  ✓ storage.state_dir — /var/lib/bugswarm
  ✓ storage.chroma_persist_dir — /var/lib/bugswarm/chroma
  ✓ logging.level — info
  Config is valid (11 checks passed)
```

```text
$ bugswarm agent run ./repo --swarm 12 --budget 100

  ╭──────────────────────────────────────────────────────────────────────────╮
  │                          BugSwarm v1.0.0                                 │
  │                       12-Agent Swarm · Budget: 100                       │
  ├──────────────────────────────────────────────────────────────────────────┤
  │ RND │ AGENT              │ STATUS    │ BUGS │ TOKENS  │  COST │ TOOLS  │
  ├─────┼────────────────────┼───────────┼──────┼─────────┼───────┼────────┤
  │  1  │ hunter-alpha       │ ACTIVE    │   —  │  4,230  │ $0.02 │     7  │
  │  1  │ hunter-beta        │ ACTIVE    │   1  │  5,102  │ $0.03 │     9  │
  │  1  │ hunter-gamma       │ ACTIVE    │   —  │  3,987  │ $0.02 │     6  │
  │  1  │ hunter-delta       │ THINKING  │   —  │  2,101  │ $0.01 │     3  │
  │  1  │ analyst-epsilon    │ REVIEWING │   2  │  7,820  │ $0.04 │    12  │
  │  1  │ analyst-zeta       │ ACTIVE    │   1  │  4,556  │ $0.02 │     8  │
  │  1  │ analyst-eta        │ FETCHING  │   —  │  1,234  │ $0.01 │     2  │
  │  1  │ analyst-theta      │ ACTIVE    │   —  │  5,678  │ $0.03 │    10  │
  │  1  │ fuzzer-iota        │ FUZZING   │   3  │     —   │ $0.00 │ 1.2K/s │
  │  1  │ fuzzer-kappa       │ FUZZING   │   1  │     —   │ $0.00 │ 1.8K/s │
  │  1  │ symbolic-lambda    │ SOLVING   │   —  │     —   │ $0.00 │    —   │
  │  1  │ judge-mu           │ BENCH     │   —  │  2,340  │ $0.01 │     5  │
  ├─────┴────────────────────┴───────────┴──────┴─────────┴───────┴────────┤
  │  ⏱  Round 1   Total bugs: 7   Confirmed: 3   Budget: $0.19/$5.00       │
  │  🟢🟢🟢🟢🟢🟢🟡🟡🟡🟡⚪⚪                                           │
  ╰──────────────────────────────────────────────────────────────────────────╯
```

```text
$ bugswarm ci --report sarif

  ╭──────────────────────────────────────────────────────────────╮
  │  CI/CD Integration                                           │
  │                                                              │
  │  Scanning ./repo on branch main...                           │
  │                                                              │
  │  ┌──────┬────────────────────────────────┬────────┬────────┐ │
  │  │ CWE  │ Description                    │ Severity │ File  │ │
  │  ├──────┼────────────────────────────────┼────────┼────────┤ │
  │  │ CWE89│ SQL Injection in query builder │ 9.1    │ db.rs  │ │
  │  │ CWE79│ XSS in render_template         │ 8.4    │ ui.py  │ │
  │  │ CWE22│ Path traversal in file_handler  │ 7.8    │ serve  │ │
  │  └──────┴────────────────────────────────┴────────┴────────┘ │
  │                                                              │
  │  ✓ SARIF report written to bugswarm-report.sarif             │
  │  ✓ PR annotations posted (3 findings)                        │
  │  ✗ Exit code 1 — findings detected                           │
  ╰──────────────────────────────────────────────────────────────╯
```

```text
$ cat doc/AGENTS.md | head -3

  ── Enterprise Quality Standards ──────────────────────────────────
  File: doc/AGENTS.md (763 lines)
  Description: Immutable invariants, phase stress tests, code
  standards, security, testing pyramid, observability, release
  process, review standards. Read before contributing.
```

```text
$ bugswarm help doc

  ┌───────────────────────────────────────────────────────────────┐
  │ Documentation                                                │
  ├───────────────────────────────────────────────────────────────┤
  │ doc/AGENTS.md                    Quality standards & phases  │
  │ doc/CHANGELOG.md                 Release history             │
  │ doc/milestone.md                 35 implementation milestones │
  │ doc/invincible.md                Data flow, symbolic, fuzzing │
  │ doc/bugswarm_strategic_analysis.md   Strategic roadmap        │
  │ doc/bugswarm_tool_superpowers.md     Tool capability overview │
  │ doc/bugswarm_mythos_gap_analysis.md  Gap analysis             │
  │ doc/chain_universal_expansion.md     Universal chain expansion│
  │ doc/production_audit.md              105 production findings  │
  │ doc/enterprise_audit.md               Enterprise audit report │
  │ doc/rust_audit.md                     Rust codebase audit     │
  │ doc/rust_extermination_plan.md        Rust refactoring plan   │
  │ doc/agent_refactoring_plan.md         Agent refactoring plan  │
  │ doc/aftermath.md                      Post-mortem analysis    │
  │ doc/ARIA.md                           ARIA integration        │
  │ doc/dominance_techniques.md           Advanced techniques     │
  │ doc/invincible_phases.md              Phase overview          │
  │ doc/plan_standard.md                  Planning standards      │
  │ doc/bug_swarm_interrogation_100qa.md  100 Q&A                 │
  │ doc/bug_swarm_technical_requirements.md  Tech requirements    │
  │ doc/phase*.md                         Phase plans (15-30)     │
  │ doc/q1*.md .. q7*.md                  Quarterly roadmaps     │
  └───────────────────────────────────────────────────────────────┘
```

```text
$ _  # <-- interactive prompt awaits your command
```
