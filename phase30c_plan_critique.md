# Phase 30-C Plan Critique — 19 Flaws

**Critiqued**: `/root/a/phase30c_critical_fix_plan.md` (1,615 lines)

---

## ARCHITECTURAL FLAWS (3)

### A1: Scope creep — new crate + new package for config
**Line**: 84-150, 400-520
The plan creates a NEW Rust crate (`bugswarm-config`) and a NEW Python package (`bugswarm-common`) just for config reading. This adds 2 new workspace members, new CI steps, new dependency chains — all for a single `serde::Deserialize` struct. CPG and Evidence currently have NO config file support. Adding `--config` flag + a simple `serde_yaml::from_str` in main.rs achieves the same result without a new crate. Creating infrastructure crates for a 50-line deserialize is over-engineering.
**Fix**: Don't create new crates. Add `--config` flag to CPG/Evidence main.rs. Define the shared YAML schema as a doc. Each daemon reads its own section via `serde_yaml::Value["daemons"]["cpg"]`.

### A2: 8 Dockerfiles referenced, 4 missing
**Line**: 1194-1340
The docker-compose references 8 Dockerfiles: sandbox, cpg, evidence, symbolic, gateway, agent, swarm, minitia. Only 4 exist (cpg, evidence, symbolic, sandbox-base). The 4 Python components have no Dockerfiles. The compose will fail to build.
**Fix**: Either add Python Dockerfiles (multi-stage: `python:3.11-slim`, `COPY`, `pip install`, `CMD`) or exclude Python components from compose and document that they run separately.

### A3: `/var/run/docker.sock` mounted into sandbox container
**Line**: 1208
```yaml
- /var/run/docker.sock:/var/run/docker.sock
```
This gives the sandbox container **full Docker daemon control**. Any sandbox escape becomes a host compromise. The sandbox uses `bollard` crate (Rust Docker client) which talks to Docker via the socket. The socket should be attached to the **host** sandbox daemon, not mounted into the execution container. The daemon runs on the host, receives PoCs, creates containers via the host's Docker socket — the socket never enters the sandbox.
**Fix**: Remove this volume mount. The sandbox DAEMON already has Docker access via the host. The sandbox EXECUTION CONTAINER must never touch docker.sock.

---

## CONSISTENCY FLAWS (4)

### C1: Image tags inconsistent between services
**Lines**: 1203, 1231, 1256, 1279, 1302, 1325
```
bugswarm/sandbox:latest        ← unversioned
bugswarm/symbolic:1.0.0        ← versioned
bugswarm/gateway:latest        ← unversioned
bugswarm/agent:latest          ← unversioned
```
After we spent H1 pinning all images to `:1.0.0`, the compose plan regresses to `:latest`. This undoes the H1 fix.
**Fix**: All images must be `:{version}` where version comes from `BUGSWARM_VERSION` env var or git tag.

### C2: Port mappings contradict the config YAML spec
**Config YAML** (lines 102,115,121): CPG http_port=8080, Evidence http_port=8081
**docker-compose** (lines 1204,1232,1257): CPG 8081:8080, Evidence 8082:8081, Gateway 8084:8080
The compose maps host:container ports DIFFERENTLY from what the config specifies. If someone changes the config `http_port` value, the compose port mapping becomes stale. These must be synchronized.
**Fix**: Use `${BGSWARM_CPG_PORT:-8080}` style variable substitution in the compose. Bind config YAML ports to compose via env vars.

### C3: "8 services" in compose but plan says "8 remaining gaps keep deployment impossible"
**Line**: 16
The plan says "No deployment is possible until all 8 are closed." But the compose defines services for ALL components — if we build C6→C3→C4→C1 in order, C1 (compose) requires Dockerfiles for Python components that are NOT part of the C6→C3→C4 pipeline. The dependency order is wrong.
**Fix**: Either make C1 Python Dockerfiles part of C6 (since they're config-dependent), or split C1 into C1a (Rust daemons compose) and C1b (Python compose).

### C4: `command` in compose duplicates `--config` and `--socket` flags
**Lines**: 1213-1215
```
command: ["run-server", "--config", "/etc/bugswarm/config.yaml",
          "--socket", "/var/run/bugswarm/sandbox.sock",
          "--http-port", "8080"]
```
If the config YAML already specifies these values, passing them as CLI flags creates confusion about precedence. Does CLI override config? Does config override CLI? The plan doesn't specify override order.
**Fix**: Either pass ONLY `--config` (config defines everything) or ONLY CLI flags (compose defines everything). Not both.

---

## CODE QUALITY FLAWS (5)

### Q1: Manual HTTP parsing is fragile
**Lines**: 860-930, 971-999
The health and metrics handlers use raw TCP + `line.starts_with("GET /health")`. This breaks on:
- HTTP/1.0 requests (no Host header, different format)
- Chunked requests
- Partial reads (TCP may deliver half a line)
- Concurrent requests on keepalive connections
- Malformed requests (no crash handling, just panic via unwrap)
**Fix**: Use `hyper` or `axum` with proper HTTP parsing. At minimum, use `tokio::io::AsyncBufReadExt::read_line` with proper buffer management.

### Q2: AtomicU64 counters not wrapped in Arc for concurrency
**Lines**: 936-937, 958-968
Metrics structs use `AtomicU64` directly but the HTTP handler spawns `tokio::spawn` — meaning the metrics must be `Arc<DaemonMetrics>`. The plan says "handle_connection must accept the metrics Arc" but the metric struct definitions don't include `Arc` wrappers.
**Fix**: Wrap in `Arc<DaemonMetrics>` at declaration time and clone into each spawned task.

### Q3: `handle_connection` already 526 lines — adding metrics injection makes it worse
**Line**: 936
The plan says "increment counters inside each match arm" — the sandbox daemon's `handle_connection` is ALREADY 526 lines (sprawl audit #4.9) with 14+ match arms. Adding `metrics.xxx.fetch_add(1)` to every arm makes it 550+. The daemon needs refactoring BEFORE adding metrics, not pushed further past the spaghetti cliff.
**Fix**: Exract each match arm into a separate handler function. Inject metrics via function parameter. This is a pre-requisite.

### Q4: Config YAML has duplicate keys
**Lines**: 122-123, 136-139
```yaml
daemons.evidence.state_dir: /var/lib/bugswarm
daemons.evidence.chroma_path: /var/lib/bugswarm/chroma
storage.state_dir: /var/lib/bugswarm
storage.chroma_persist_dir: /var/lib/bugswarm/chroma
```
`state_dir` and `chroma_path` appear in BOTH `daemons.evidence` and `storage`. Which one takes precedence? If they differ, which value does the evidence daemon use?
**Fix**: Single source of truth. Remove duplicates. Either all storage config lives under `storage.*` or under `daemons.evidence.*` — not both.

### Q5: `danger_sinks` in config YAML is a security footgun
**Line**: 130
```yaml
danger_sinks: [exec, eval, system, popen, subprocess, os_system, shell_exec, Runtime_exec, ProcessBuilder_start, system, CreateProcess, ShellExecute, winexec]
```
This list duplicates `DEFAULT_SINKS` in `bugswarm-cpg/src/danger_map.rs`. If someone changes one but not the other, the fuzzer uses different sink definitions than the CPG. Also, `system` appears TWICE in the list.
**Fix**: Config should READ from the code's DEFAULT_SINKS constant, not duplicate it. Add a test that verifies config YAML's sink list matches the constant.

---

## MISSING ELEMENTS (4)

### M1: No migration path from existing SandboxConfig
The plan says "switch to unified config" but existing deployments have `--config /etc/bugswarm/sandbox.yaml` with the OLD SandboxConfig format (which has different field names). The plan doesn't specify a migration tool, backward compatibility period, or deprecation warning for the old format.
**Fix**: Add migration function: `fn migrate_old_config(old_yaml: &str) -> UnifiedConfig`. Support both old and new format for 2 releases, with deprecation warning.

### M2: No rollback strategy
If the unified config breaks something, there's no way to roll back to per-daemon configs. The plan doesn't specify a `--legacy-config` flag or fallback mechanism.
**Fix**: Keep old `--config` behavior as fallback. New `--unified-config` flag reads unified file. Old deployments continue working.

### M3: No health check for ChromaDB in Evidence daemon's `/ready`
The evidence daemon depends on ChromaDB but the `/ready` endpoint doesn't check if ChromaDB is reachable. If ChromaDB is down, the evidence daemon would report "ready" but be non-functional.
**Fix**: `/ready` must ping ChromaDB. Return 503 if unreachable.

### M4: No Swarm component in docker-compose
The plan's compose has 7 services (sandbox, cpg, evidence, symbolic, gateway, agent, minitia) but NOT the swarm orchestrator. The swarm is the MAIN orchestrator — without it, agents don't coordinate. Either swarm should be in the compose, or the plan should explain why it's excluded.
**Fix**: Add swarm service or document that swarm runs separately (e.g., launched by CI, not daemonized).

---

## IMMUNITY GAPS (3)

### I1: CI check for `docker-compose.yml` validity doesn't verify Dockerfile existence
The plan says "CI check that docker-compose.yml exists and is valid YAML" but doesn't check that every referenced `build.context` directory has a `Dockerfile`. A valid YAML compose can still fail to build.
**Fix**: CI step: `for ctx in $(yq '.services[].build.context' docker-compose.yml); do test -f "$ctx/Dockerfile" || exit 1; done`

### I2: CI health check doesn't verify /ready endpoint
The plan says "CI check every daemon responds to GET /health" but `/ready` is the endpoint that checks DEPENDENCIES. A daemon can be "healthy" (process alive) but "not ready" (cannot reach database). CI should check both.
**Fix**: CI step: `curl -f localhost:8080/health && curl -f localhost:8080/ready`

### I3: No test that docker-compose actually starts successfully
The plan's verification is manual (`curl localhost:8080/health`). No automated CI test that runs `docker compose up -d`, waits for healthy, then `docker compose down`.
**Fix**: Add integration test: `docker compose up -d --wait && curl -f localhost:*/health && docker compose down`

---

## SUMMARY

| Severity | Count | IDs |
|----------|-------|-----|
| **ARCHITECTURAL** | 3 | A1 (scope creep — new crates), A2 (4 missing Dockerfiles), A3 (docker.sock mounted into sandbox = host compromise) |
| **CONSISTENCY** | 4 | C1 (image tags inconsistent), C2 (port mappings contradict config), C3 (wrong dependency order), C4 (CLI+config conflict) |
| **CODE QUALITY** | 5 | Q1 (manual HTTP parsing fragile), Q2 (missing Arc), Q3 (526-line function made worse), Q4 (duplicate config keys), Q5 (sink list duplicated) |
| **MISSING** | 4 | M1 (no migration), M2 (no rollback), M3 (ChromaDB not in /ready), M4 (swarm missing from compose) |
| **IMMUNITY** | 3 | I1 (Dockerfile existence not checked), I2 (/ready not checked), I3 (no compose integration test) |
| **TOTAL** | **19** | |

### Top 3 Must-Fix:
1. **A3** — `/var/run/docker.sock` mount is a host-compromise security hole. BLOCKING.
2. **A1** — Creating 2 new crates for config is scope creep. Simpler approach exists. BLOCKING.
3. **Q1** — Manual HTTP parsing will break on first real request. Use hyper/axum instead.
