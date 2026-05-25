# Phase 23 Trigger Matrix — Architecture Integration Remediation Plan

**Audit:** Phase 23 Trigger Matrix Audit
**Findings:** F1, F7, F8, F9, F10, F11, F12
**Classification:** CRITICAL — Architecture/Integration Gaps
**Date:** Generated from audit evidence

---

## Problem Summary

The trigger module at `bugswarm-evidence/src/trigger.rs` is fully implemented (392 lines, 650+ test assertions) with trigger conditions, deduplication, completeness scoring, Phase 30 gating, and a TriggerManager. However, it is a **standalone island** — zero integration with the rest of the codebase. None of the three Rust crates, the Python agent, the Python swarm orchestrator, or the Python sandbox daemon call into it. The module exists in a vacuum.

---

## Root Cause Classification (shared across all 7 findings)

The bugswarm codebase has **no integration enforcement mechanism**. Each module is developed as an independent unit with its own Cargo.toml, its own daemon, and its own set of types. The project lacks:

1. **A workspace-level Cargo.toml** that declares cross-crate dependencies, making it impossible to import types across crates at compile time.
2. **A mandatory integration contract** that requires every module's public API to have at least one external caller.
3. **A handler registration pattern** in daemons — each daemon manually enumerates its methods in a `match` arm; there is no trait or registry that forces adding new RPC methods.
4. **A tool registry pattern** in the agent — agent tools are manually registered in `wiring.py` with no compile-time or load-time check that every service module has a corresponding client.
5. **No orchestration hooks** — the orchestrator has no `on_confirm_bug` callback that downstream modules can hook into.

These are not individual oversights — they are systemic architectural gaps that guarantee any new module will have zero integration unless integration is manually wired for every single connection point.

---

## Finding F1: Zero integration with EvidenceGraph

**Severity:** CRITICAL
**Module:** `bugswarm-evidence/src/trigger.rs:219` (TriggerManager)
**Impact:** Trigger conditions are stored in an in-memory HashMap that disappears on process exit. No graph nodes or edges are created. The EvidenceGraph has `NodeKind::TriggerMatrix` and `NodeKind::TriggerCondition` variants (types.rs:27-28) and `EdgeKind::Triggers`, `EdgeKind::ContributedBy`, `EdgeKind::ConditionEquivalent` variants (types.rs:75-79), but the trigger module never creates any of them.

### Root Cause Analysis

The `TriggerManager` struct (`trigger.rs:219-259`) uses a `HashMap<String, TriggerMatrix>` as its sole storage. It was designed as a self-contained unit with no awareness of `EvidenceGraph`. The `EvidenceGraph` was designed with trigger-related `NodeKind` and `EdgeKind` variants pre-declared in `types.rs`, but no bridge code was written to connect the two. This is a classic "both sides have the interface, neither side calls it" gap.

### Permanent Fix

**Add a `trigger_graph_bridge` module** inside `bugswarm-evidence` that implements bidirectional synchronization between TriggerManager and EvidenceGraph. The bridge must:

1. Accept a shared `Arc<EvidenceGraph>` and a `TriggerManager`.
2. Implement `fn push_condition_to_graph(graph: &EvidenceGraph, condition: &TriggerCondition) -> (NodeId, NodeId)` that:
   - Creates a `NodeKind::TriggerCondition` node in the graph with the condition's data
   - Creates/finds a `NodeKind::TriggerMatrix` node for the bug
   - Creates a `Triggers` edge from condition node to matrix node
   - Creates a `ContributedBy` edge from the condition node to the agent/fuzzer node
3. Implement `fn push_matrix_to_graph(graph: &EvidenceGraph, matrix: &TriggerMatrix) -> Vec<NodeId>` that syncs an entire matrix.
4. Implement `fn graph_to_trigger_manager(graph: &EvidenceGraph) -> TriggerManager` for reading back from the graph.
5. Modify `TriggerManager::add_condition` (`trigger.rs:235`) to accept an optional `Arc<EvidenceGraph>` and automatically push to both stores when present.

**Change `TriggerManager` to be graph-aware:**

```rust
// trigger.rs:219
pub struct TriggerManager {
    matrices: HashMap<String, TriggerMatrix>,
    graph: Option<Arc<EvidenceGraph>>,  // NEW: optional graph backend
}
```

And modify `add_condition` to conditionally sync:

```rust
// trigger.rs:235 — modified
pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
    let bug_id = condition.bug_id.clone();
    let matrix = self.get_or_create(&bug_id);
    let is_new = matrix.add_condition(condition.clone());
    if is_new {
        if let Some(ref graph) = self.graph {
            trigger_graph_bridge::push_condition_to_graph(graph, &condition);
        }
    }
    is_new
}
```

### Immunity Mechanism

**Trait-based storage backend with compile-time enforcement.** Define a `TriggerStorage` trait that both `HashMap` and `EvidenceGraph` must implement:

```rust
// New file: bugswarm-evidence/src/trigger_storage.rs
pub trait TriggerStorage {
    fn store_condition(&self, condition: &TriggerCondition) -> bool;
    fn get_matrix(&self, bug_id: &str) -> Option<TriggerMatrix>;
    fn get_incomplete_bugs(&self) -> Vec<TriggerMatrix>;
}
```

The `TriggerManager` is parameterized by `T: TriggerStorage`. A compile-time test verifies that both `impl TriggerStorage for EvidenceGraph` and the existing HashMap backend compile. This makes it **impossible** to write a `TriggerManager` that doesn't have a storage backend — and the graph backend is always available as an option.

Additionally, add a **compile-time assertion** in `trigger.rs` that verifies the `NodeKind` enum has `TriggerMatrix` and `TriggerCondition` variants (using a `match` on a const `NodeKind::TriggerMatrix` reference — this already compiles given the existing enum, but the assertion documents the invariant).

### Implementation Checklist

1. **Create** `bugswarm-evidence/src/trigger_graph_bridge.rs`:
   - `push_condition_to_graph(graph, condition) -> (NodeId, NodeId)`
   - `push_matrix_to_graph(graph, matrix) -> Vec<NodeId>`
   - `graph_to_trigger_manager(graph) -> TriggerManager`
   - `find_or_create_matrix_node(graph, bug_id) -> NodeId`

2. **Modify** `bugswarm-evidence/src/trigger.rs:219-222`:
   - Add `graph: Option<Arc<EvidenceGraph>>` field to `TriggerManager`
   - Add `fn with_graph(graph: Arc<EvidenceGraph>) -> Self` constructor
   - Modify `add_condition` at line 235 to call bridge on new conditions

3. **Modify** `bugswarm-evidence/src/lib.rs:4`:
   - Add `pub mod trigger_graph_bridge;`

4. **Create** `bugswarm-evidence/src/trigger_storage.rs`:
   - Define `pub trait TriggerStorage`
   - Implement `TriggerStorage` for the existing HashMap backend

5. **Update** `bugswarm-evidence/src/graph.rs`:
   - Add `fn add_trigger_condition_node(...)` method
   - Add `fn add_trigger_matrix_node(...)` method
   - Add `fn find_matrix_node(&self, bug_id: &str) -> Option<NodeId>`

6. **Add test** at `bugswarm-evidence/tests/trigger_audit.rs:799` (after existing integration stubs):
   - Test: add condition to TriggerManager with graph backend, verify node appears in EvidenceGraph
   - Test: verify Triggers edge connects condition node to matrix node

### Success Criteria

- [ ] `TriggerCondition::new(...)` + `TriggerManager::add_condition(...)` with graph backend creates a `NodeKind::TriggerCondition` node in EvidenceGraph
- [ ] Two semantically equivalent conditions from different layers create only 1 condition node but 2 `ContributedBy` edges
- [ ] `EvidenceGraph::query()` can find trigger condition nodes by author/layer
- [ ] `EvidenceGraph::stats()` counts trigger nodes in confirmed_bugs or a new trigger_nodes field
- [ ] Integration test passes: `cargo test --test trigger_audit -- integration_graph_bridge`

---

## Finding F7: No evidence daemon handlers for trigger methods

**Severity:** CRITICAL
**Module:** `bugswarm-evidence/src/daemon.rs` (process function, lines 122-183)
**Impact:** The evidence daemon handles `add_claim`, `add_sandbox_run`, `confirm_bug`, `stats`, `query`, `score_agent`, `verify`, and `health` — but has no `add_trigger_condition` or `get_trigger_matrix` RPC. Any client (agent, fuzzer, orchestrator) cannot submit trigger conditions through the daemon.

### Root Cause Analysis

The daemon's `process` function (`daemon.rs:122`) uses a hardcoded `match req.method.as_str()` dispatch with manually enumerated RPC methods. There is no trait, no registry, and no code generation that forces a new module to register its RPC methods. When `trigger.rs` was written, the `daemon.rs` file was not updated because there is nothing in the architecture that requires it. The developer must remember to do it manually — and didn't.

### Permanent Fix

**Replace the manual `match` dispatch with a `DaemonHandler` trait registry.** Create a trait that every daemon-handled module must implement:

```rust
// bugswarm-evidence/src/daemon_registry.rs
pub trait DaemonHandler: Send + Sync {
    fn method_name(&self) -> &'static str;
    fn handle(&self, req: &DaemonRequest, graph: &EvidenceGraph) -> DaemonResponse;
}

pub struct DaemonRegistry {
    handlers: HashMap<String, Box<dyn DaemonHandler>>,
}
```

Then add a **mandatory test** that verifies every public module in the crate has a registered daemon handler:

```rust
#[test]
fn all_public_modules_have_daemon_handlers() {
    // Enumerate every method that must be supported
    let required = &["add_claim", "add_sandbox_run", "link_result", "confirm_bug",
                     "stats", "query", "score_agent", "verify", "health",
                     "add_trigger_condition", "get_trigger_matrix"];
    let registry = build_daemon_registry();
    for method in required {
        assert!(registry.handlers.contains_key(*method),
            "Missing daemon handler for method: {}", method);
    }
}
```

Concrete trigger handlers to add:

```rust
// In daemon.rs or a new daemon_trigger.rs module:
struct AddTriggerConditionHandler;
impl DaemonHandler for AddTriggerConditionHandler {
    fn method_name(&self) -> &'static str { "add_trigger_condition" }
    fn handle(&self, req: &DaemonRequest, graph: &EvidenceGraph) -> DaemonResponse {
        // Deserialize TriggerCondition from request fields
        // Call TriggerManager (or graph bridge) to store
        // Return success/failure
    }
}

struct GetTriggerMatrixHandler;
impl DaemonHandler for GetTriggerMatrixHandler {
    fn method_name(&self) -> &'static str { "get_trigger_matrix" }
    fn handle(&self, req: &DaemonRequest, graph: &EvidenceGraph) -> DaemonResponse {
        // Query graph for trigger matrix node, return serialized
    }
}
```

### Immunity Mechanism

**Compile-time + test-time double enforcement:**

1. **Compile-time:** The `DaemonHandler` trait is sealed (cannot be implemented outside the crate). Every handler must be instantiated in the `build_daemon_registry()` function, which lives in `daemon.rs`. Any new public module that exports data types must add its handler to this function or the crate won't compile (because the test verifies it).

2. **Test-time CI gate:** A `#[test]` function (shown above) enumerates every required RPC method. CI runs this test and **blocks merge** if any method is missing. The test list of required methods is generated from the module list in `lib.rs` — each `pub mod` declaration maps to at least one daemon handler.

3. **Lint rule:** A custom clippy lint (or a simple grep in CI) verifies that every `pub mod` in `lib.rs` has a corresponding `"method_name"` string in the test's required list.

### Implementation Checklist

1. **Create** `bugswarm-evidence/src/daemon_registry.rs`:
   - Define `DaemonHandler` trait (sealed via a private `Sealed` supertrait)
   - Define `DaemonRegistry` struct with `HashMap<String, Box<dyn DaemonHandler>>`
   - Implement `build_daemon_registry() -> DaemonRegistry`

2. **Create** `bugswarm-evidence/src/daemon_trigger.rs`:
   - `AddTriggerConditionHandler` — deserializes request fields into `TriggerCondition`, pushes to graph
   - `GetTriggerMatrixHandler` — queries graph for matrix data, returns serialized JSON
   - `ListIncompleteBugsHandler` — returns incomplete bugs from TriggerManager/Graph

3. **Modify** `bugswarm-evidence/src/daemon.rs:57-60`:
   - Change `let graph = Arc::new(EvidenceGraph::new());` to also construct `TriggerManager::with_graph(graph.clone())`
   - Store `TriggerManager` alongside `graph` in the daemon's state

4. **Modify** `bugswarm-evidence/src/daemon.rs:80-92` (handle_connection):
   - Pass `&DaemonRegistry` to `process()` instead of matching on method string

5. **Modify** `bugswarm-evidence/src/daemon.rs:122-183` (process function):
   - Replace manual `match` with `registry.handlers.get(req.method).map(|h| h.handle(req, graph))`
   - Keep `Unknown method` fallback for unrecognized methods

6. **Modify** `bugswarm-evidence/src/lib.rs:4`:
   - Add `pub mod daemon_registry;` and `pub mod daemon_trigger;`

7. **Add test** at `bugswarm-evidence/tests/trigger_audit.rs`:
   - Test: call daemon's `add_trigger_condition` through registry, verify response
   - Test: call daemon's `get_trigger_matrix` through registry, verify response
   - Test: `all_handlers_registered` — verify trigger handlers are in registry

### Success Criteria

- [ ] Daemon accepts `{"method":"add_trigger_condition","bug_id":"BUG-001","dimension":"Input","description":"x=null","layer":"Agent"}` and returns `{"success":true,...}`
- [ ] Daemon accepts `{"method":"get_trigger_matrix","bug_id":"BUG-001"}` and returns the stored matrix
- [ ] Unknown methods still return `{"success":false,"error":"Unknown method: ..."}`
- [ ] Registry test verifies all `pub mod` entries in `lib.rs` have at least one handler
- [ ] `cargo test --test trigger_audit -- integration_daemon_trigger` passes

---

## Finding F8: Agent tools are pure stubs

**Severity:** CRITICAL
**Modules:** `bugswarm-agent/src/agent/cli/wiring.py:337-371`
**Impact:** `describe_trigger` (`wiring.py:337`) creates a dict with `hashlib.sha256`, serializes it to JSON, and returns it — data is created and immediately discarded. `get_trigger_matrix` (`wiring.py:356`) returns a hardcoded empty structure with `"conditions": []` and `"completeness_score": 0.0`. Agent tools provide zero real functionality.

### Root Cause Analysis

The agent tools are registered in `wiring.py:142-163` with proper `ToolDefinition` entries (name, description, parameters, handler), but the handler functions (`_describe_trigger`, `_get_trigger_matrix`) were written as placeholders. They were never connected to a real EvidenceClient because:
1. No `EvidenceClient` class exists (see F9).
2. The tool wiring pattern has no mandatory "connect to client" validation step.
3. There is no test that verifies tool outputs contain real data (non-zero conditions, non-zero completeness).

### Permanent Fix

**Replace stub handlers with real `EvidenceClient` calls.** Once F9 creates the `EvidenceClient`, the fix is:

1. Instantiate `EvidenceClient` in `setup_tools()` / `wiring.py` (alongside `CPGClient` and `SandboxClient`).
2. Replace `_describe_trigger` (`wiring.py:337-353`) with an async call to `evidence_client.add_trigger_condition(bug_id, dimension, description, layer="Agent")`.
3. Replace `_get_trigger_matrix` (`wiring.py:356-371`) with an async call to `evidence_client.get_trigger_matrix(bug_id)`.

Additionally, implement a **tool output validation pattern**: every `ToolResult` returned by a tool must include a `tool_name` field and a `backend_call_succeeded` flag. A CI test iterates all registered tools and verifies:
- `tool_name` is non-empty
- `backend_call_succeeded` is true (meaning the tool reached a real backend, not a local stub)
- The output contains operation-specific data (e.g., trigger tools must have `completeness_score` in their output)

### Immunity Mechanism

**Tool audit test with backend reachability verification.** Create a test at `bugswarm-agent/src/agent/tests/` that:

1. Enumerates all registered `ToolDefinition` instances from `ToolRegistry`.
2. For each tool, calls the handler with a minimal valid input.
3. Verifies the `ToolResult.metadata` dict contains a `"stub"` key set to `false` (stubs set it to `true`).
4. Verifies that tools in the "evidence" family (describe_trigger, get_trigger_matrix) produce non-empty results.

The test runs in CI and **blocks merge** if any tool returns `{"stub": true}`.

Additionally, convert stub detection into a **runtime assertion**: when `ToolResult(True, ...)` is returned by a stub handler, include `{"stub": true, "warning": "This tool is a placeholder stub"}` in the metadata. The agent's `IEPEngine` checks for this flag and logs a warning, refusing to count stub results toward agent scores.

### Implementation Checklist

1. **Create** `bugswarm-agent/src/agent/evidence_client.py` (see F9) with:
   - `EvidenceClient` class
   - `add_trigger_condition(bug_id, dimension, description, layer)` async method
   - `get_trigger_matrix(bug_id)` async method
   - `list_incomplete_bugs()` async method

2. **Modify** `bugswarm-agent/src/agent/cli/wiring.py:15-20`:
   - Add `from agent.evidence_client import EvidenceClient` import
   - Instantiate `evidence = EvidenceClient()` alongside `cpg` and `sandbox`

3. **Modify** `bugswarm-agent/src/agent/cli/wiring.py:150`:
   - Change `handler=lambda args: _describe_trigger(args)` to `handler=lambda args: _describe_trigger(evidence, args)`

4. **Modify** `bugswarm-agent/src/agent/cli/wiring.py:160`:
   - Change `handler=lambda args: _get_trigger_matrix(args)` to `handler=lambda args: _get_trigger_matrix(evidence, args)`

5. **Rewrite** `wiring.py:337-371`:
   - `_describe_trigger(evidence, args)` — calls `evidence.add_trigger_condition(...)`, returns real result
   - `_get_trigger_matrix(evidence, args)` — calls `evidence.get_trigger_matrix(...)`, returns real result

6. **Create** `bugswarm-agent/src/agent/tests/test_tool_audit.py`:
   - Test that iterates all tools, verifies no stubs
   - Test that trigger tools return real data (non-zero conditions, valid completeness_score)

7. **Modify** `bugswarm-agent/src/agent/tools.py`:
   - Add `stub: bool = False` field to `ToolDefinition`
   - Add `stub: bool = False` field to `ToolResult` metadata

8. **Modify** `bugswarm-agent/src/agent/cli/wiring.py:142-163`:
   - Set `stub=False` on `describe_trigger` and `get_trigger_matrix` ToolDefinitions

### Success Criteria

- [ ] `describe_trigger` tool call with `bug_id="BUG-001", dimension="Input", description="x=null"` returns a real condition with a server-generated ID
- [ ] `get_trigger_matrix` tool call with `bug_id="BUG-001"` returns the previously stored condition
- [ ] `get_trigger_matrix` for a bug with no conditions returns an empty matrix (not a hardcoded empty stub)
- [ ] Tool audit test passes: zero stubs detected among evidence-family tools
- [ ] Agent `IEPEngine` no longer receives `{"stub": true}` from trigger tools

---

## Finding F9: No evidence client exists

**Severity:** CRITICAL
**Module:** Missing file — no `bugswarm-agent/src/agent/evidence_client.py`
**Impact:** The agent has `CPGClient` (`cpg_client.py`, 213 lines) and `SandboxClient` (`sandbox_client.py`, 315 lines) but no `EvidenceClient`. The agent cannot communicate with the evidence daemon to submit trigger conditions, query trigger matrices, or interact with the evidence graph from Python.

### Root Cause Analysis

The agent's client pattern was established with `CPGClient` and `SandboxClient` — both wrap subprocess execution of Rust binaries with JSON stdin/stdout communication. When the evidence daemon was created (`daemon.rs`), the corresponding Python client was never written because:
1. The evidence daemon's Unix socket protocol differs from CPG/sandbox subprocess patterns.
2. The trigger module was developed in isolation without client requirements.
3. No architectural rule says "every Rust daemon must have a corresponding Python client in the agent."

### Permanent Fix

Create `EvidenceClient` class with the same pattern as `CPGClient` and `SandboxClient`, using **Unix socket communication** since the evidence daemon listens on `/var/run/bugswarm/evidence.sock`. The client must support:

1. **Connection management:** `connect()` / `disconnect()` to the Unix socket.
2. **Request/response protocol:** Send JSON line, receive JSON line (matching `daemon.rs`'s `DaemonRequest`/`DaemonResponse` format).
3. **Trigger methods:**
   - `add_trigger_condition(bug_id, dimension, description, layer)` → calls `{"method":"add_trigger_condition",...}`
   - `get_trigger_matrix(bug_id)` → calls `{"method":"get_trigger_matrix",...}`
   - `list_incomplete_bugs()` → calls `{"method":"list_incomplete_bugs",...}`
4. **Evidence graph methods** (for future use):
   - `add_claim(...)`, `add_sandbox_run(...)`, `confirm_bug(...)`, `stats()`, `query(...)`

### Immunity Mechanism

**Client generation from API spec.** Create a JSON schema file (`evidence_api.json`) that defines every RPC method the evidence daemon supports. A CI script auto-generates the `EvidenceClient` skeleton from this schema. If a new RPC is added to the daemon but the schema isn't updated, the daemon test fails (because the daemon handler list check, see F7, compares against the schema). If the schema is updated but the client isn't regenerated, the client test fails (because it checks all methods in the schema have corresponding Python methods).

The schema file at `bugswarm-agent/src/agent/schemas/evidence_api.json`:
```json
{
  "service": "evidence",
  "methods": {
    "add_claim": {"params": ["claim", "author", "location", "severity"], "returns": "node_id"},
    "add_trigger_condition": {"params": ["bug_id", "dimension", "description", "layer"], "returns": "condition_id"},
    "get_trigger_matrix": {"params": ["bug_id"], "returns": "matrix"},
    ...
  }
}
```

A CI script compares the schema against:
- The daemon's handler registry (must match)
- The Python client's method list (must match)

### Implementation Checklist

1. **Create** `bugswarm-agent/src/agent/evidence_client.py`:
   ```python
   class EvidenceClient:
       def __init__(self, socket_path: str = "/var/run/bugswarm/evidence.sock"):
           self.socket_path = socket_path
           self._socket = None

       async def connect(self): ...
       async def disconnect(self): ...
       async def _send_request(self, method: str, **params) -> dict: ...
       async def add_trigger_condition(self, bug_id: str, dimension: str, description: str, layer: str = "Agent") -> dict: ...
       async def get_trigger_matrix(self, bug_id: str) -> dict: ...
       async def list_incomplete_bugs(self) -> list[dict]: ...
       async def health(self) -> bool: ...
   ```

2. **Create** `bugswarm-agent/src/agent/schemas/evidence_api.json`:
   - Define every evidence daemon method with params and return types.

3. **Modify** `bugswarm-agent/src/agent/cli/wiring.py:22`:
   - Add `from agent.evidence_client import EvidenceClient`

4. **Modify** `bugswarm-agent/src/agent/orchestrator.py:70-73` (in `setup_tools`):
   - Add `evidence = EvidenceClient()`
   - Add `await evidence.connect()`

5. **Create** `bugswarm-agent/src/agent/tests/test_evidence_client.py`:
   - Test: `connect()` / `disconnect()` lifecycle
   - Test: `add_trigger_condition()` returns valid dict with ID
   - Test: `get_trigger_matrix()` returns stored conditions
   - Test: `health()` returns True

6. **Create CI script** at `scripts/verify_client_schema.py`:
   - Compares `evidence_api.json` against daemon handler registry
   - Compares `evidence_api.json` against `EvidenceClient` method list

### Success Criteria

- [ ] `EvidenceClient` connects to `/var/run/bugswarm/evidence.sock`
- [ ] `add_trigger_condition("BUG-TEST", "Input", "x=null", "Agent")` returns `{"success":true,"data":"tc-BUG-TEST-Input-<hash>"}`
- [ ] `get_trigger_matrix("BUG-TEST")` returns the stored condition
- [ ] `health()` returns `{"success":true}`
- [ ] Client gracefully handles daemon not running (returns error, doesn't crash)
- [ ] Schema verification script passes in CI

---

## Finding F10: No orchestrator trigger calls

**Severity:** CRITICAL
**Module:** `bugswarm-swarm/src/swarm/orchestrator.py`
**Impact:** The swarm orchestrator manages 12 agents hunting bugs. When an agent confirms a bug via sandbox execution (line 290: `if f.verified`), the orchestrator updates agent scores (`trust_score`, `hallucination_rate`) but **never documents trigger conditions**. The `confirm_bug` flow in orchestrator.py has no trigger documentation step.

### Root Cause Analysis

The orchestrator's `_execute_round` method (`orchestrator.py:248-301`) processes findings in a tight loop. Each finding has a `verified` flag (line 290), `claim` text, and `location` string. This data is sufficient to auto-generate initial trigger conditions (e.g., "Input: <claim_text>", "CodeLocation: <file_path>"). However, the orchestrator has no EvidenceClient and no awareness of trigger matrices. The bug confirmation pipeline is: Agent → IEPEngine → Sandbox → verified finding → agent score update. It should be: Agent → IEPEngine → Sandbox → verified finding → **EvidenceClient.add_trigger_condition** → agent score update.

### Permanent Fix

**Add an `EvidenceClient` to the orchestrator's `setup_tools()` and create a `_document_trigger_conditions()` hook** that is called whenever a finding is verified.

1. In `SwarmOrchestrator.__init__` (`orchestrator.py:40`), add `self._evidence: EvidenceClient | None = None`.

2. In `setup_tools()` (`orchestrator.py:65`), instantiate `EvidenceClient` alongside `CPGClient` and `SandboxClient`.

3. Create method `_document_trigger_conditions(self, finding: dict)` that auto-extracts trigger conditions from the finding data:
   - **Input dimension:** Extract from `finding["claim"]` (e.g., "input contains SQL injection" → TriggerDimension::Input)
   - **CodeLocation:** Extract from `finding["location"]` (e.g., "auth.py:42" → maps to DataState or Environment)
   - **Severity:** Extract from `finding.get("severity_estimate", 5)`
   - Call `evidence.add_trigger_condition(bug_id, dimension, description, layer="Agent")` for each auto-extracted dimension

4. Hook into `_execute_round` at `orchestrator.py:290` — after `if f.verified` is checked and before `slot.trust_score` is updated.

5. Also hook into the existing `pdb.store_finding()` block at `orchestrator.py:409` — after persisting to PatternDB, also push to evidence graph.

### Immunity Mechanism

**Orchestration hook registry with mandatory implementation.** Define a Python protocol class `BugConfirmationHook`:

```python
# bugswarm-swarm/src/swarm/hooks.py
from typing import Protocol, runtime_checkable

@runtime_checkable
class BugConfirmationHook(Protocol):
    def on_bug_confirmed(self, finding: dict) -> None: ...
    def on_bug_contradicted(self, finding: dict) -> None: ...
```

The orchestrator maintains a `self._hooks: list[BugConfirmationHook]`. Every downstream service (evidence client, pattern DB, hotspot tracker) registers as a hook. A CI test verifies:
- At least 1 hook is registered (EvidenceClient)
- The `evidence_hook` is in the list
- Calling `on_bug_confirmed(mock_finding)` on the evidence hook results in a real daemon call (not a no-op)

The orchestrator's `run()` method calls a validator:

```python
def _validate_hooks(self):
    if not self._hooks:
        raise RuntimeError("Orchestrator has zero BugConfirmationHooks — must register at least EvidenceHook")
```

### Implementation Checklist

1. **Create** `bugswarm-swarm/src/swarm/hooks.py`:
   - Define `BugConfirmationHook` protocol
   - Implement `EvidenceConfirmationHook` using `EvidenceClient`
   - Implement `PatternDBConfirmationHook` wrapping existing pdb.store_finding() logic
   - Implement `HotspotConfirmationHook` wrapping existing ht.record_bug() logic

2. **Modify** `bugswarm-swarm/src/swarm/orchestrator.py:21-22`:
   - Add `from agent.evidence_client import EvidenceClient`
   - Add `from .hooks import BugConfirmationHook, EvidenceConfirmationHook`

3. **Modify** `bugswarm-swarm/src/swarm/orchestrator.py:40-53` (`__init__`):
   - Add `self._hooks: list[BugConfirmationHook] = []`
   - Add `self._evidence: EvidenceClient | None = None`

4. **Modify** `bugswarm-swarm/src/swarm/orchestrator.py:65-118` (`setup_tools`):
   - Add `evidence = EvidenceClient()` instantiation
   - Add `await evidence.connect()`
   - Add `self._hooks.append(EvidenceConfirmationHook(evidence))`
   - Add `self._hooks.append(PatternDBConfirmationHook(...))`
   - Add `self._hooks.append(HotspotConfirmationHook(...))`

5. **Modify** `bugswarm-swarm/src/swarm/orchestrator.py:248-301` (`_execute_round`):
   - After line 290 (`if f.verified:`), call `for hook in self._hooks: hook.on_bug_confirmed(finding_dict)`

6. **Modify** `bugswarm-swarm/src/swarm/orchestrator.py:396-426` (`run`):
   - Replace manual `pdb.store_finding()` block with hook iteration
   - Call `self._validate_hooks()` before starting rounds

7. **Create** `bugswarm-swarm/src/swarm/tests/test_confirmation_hooks.py`:
   - Test: mock finding verified → mock evidence client receives `add_trigger_condition` call
   - Test: `_validate_hooks()` raises RuntimeError when no hooks registered
   - Test: all hooks called for each verified finding

### Success Criteria

- [ ] When orchestrator processes a verified finding, `EvidenceClient.add_trigger_condition()` is called
- [ ] At minimum, Input dimension is auto-extracted from the claim text
- [ ] Source/dimension mapping: claim → Input, location → DataState, severity → stored in condition metadata
- [ ] Orphan bug: when a bug is contradicted (not confirmed), no trigger condition is created
- [ ] Hook validation raises RuntimeError if evidence hook is not registered
- [ ] Evidence daemon receives and stores trigger conditions from orchestrator test run

---

## Finding F11: No fuzzer trigger contribution

**Severity:** HIGH
**Module:** `bugswarm-sandbox/src/fuzzer.rs` (FuzzCrash at line 166, FuzzController at line 516)
**Impact:** `FuzzCrash` contains `crash_id`, `signal`, `signal_name`, `crash_address`, `stack_trace`, `crashing_input`, `input_size`, `classification`, and `discovered_at` — all the data needed to create a `TriggerCondition` with `ContributionLayer::Fuzzer`. Yet the fuzzer never creates one. The `record_crash` method (`fuzzer.rs:602-681`) processes crash data, deduplicates, updates stats, but never calls into the evidence system.

### Root Cause Analysis

The sandbox crate cannot import types from `bugswarm-evidence` because there is no workspace-level `Cargo.toml` (see F12). Even if the developer wanted to create `TriggerCondition` objects from fuzzer crashes, they couldn't reference the type. Additionally, the `FuzzController` has no reference to an evidence client, evidence daemon, or any evidence-related trait. The fuzzer is designed as a self-contained crash-finding engine with no external reporting interface beyond its own `FuzzCrash` struct.

### Permanent Fix

**Two-step fix:** First, resolve F12 (workspace Cargo.toml + cross-crate dependency). Then, add an `EvidenceReporter` trait in the sandbox crate that the fuzzer calls when a unique crash is discovered.

1. **Workspace dependency** (see F12): Add `bugswarm-evidence` as a dependency of `bugswarm-sandbox`.

2. **Define `EvidenceReporter` trait** (in sandbox crate):
   ```rust
   // bugswarm-sandbox/src/evidence_reporter.rs
   pub trait EvidenceReporter: Send + Sync {
       fn report_trigger_condition(
           &self,
           bug_id: &str,
           crash: &FuzzCrash,
       ) -> Vec<TriggerCondition>;
   }
   ```

3. **Implement `FuzzCrash` → `TriggerCondition` mapping:**
   - `crash_address` + `input_size` → maps to `TriggerDimension::Input` (the crashing input triggered the bug)
   - `signal_name` + `classification` → maps to `TriggerDimension::DataState` (program state at crash)
   - `stack_trace` → maps to `TriggerDimension::Environment` (which code paths are involved)
   - `crashing_input` → maps to `TriggerDimension::Configuration` (input configuration)
   - `discovered_at` timestamp → stored in condition metadata
   - Layer = `ContributionLayer::Fuzzer`

4. **Integrate into `FuzzController`:** Add an `Option<Box<dyn EvidenceReporter>>` field to `FuzzController`. In `record_crash` (`fuzzer.rs:602`), after a new unique crash is confirmed (line 676), call the reporter.

5. **Production implementation:** In the sandbox daemon (`daemon.rs`), wire a real `EvidenceReporter` that connects to the evidence daemon's Unix socket and submits `add_trigger_condition` RPCs. Or, if the sandbox process can import `evidence` types directly (after F12), construct `TriggerCondition` structs and push them to a shared `TriggerManager` instance.

### Immunity Mechanism

**Mandatory `EvidenceReporter` in FuzzController with compile-time check.** The `EvidenceReporter` trait is required in the `FuzzController::new()` constructor. Instead of `Option<Box<dyn EvidenceReporter>>`, make it a required parameter:

```rust
pub fn new(config: FuzzConfig, dedup_config: DedupConfig, reporter: Box<dyn EvidenceReporter>) -> Self
```

At compile time, any code constructing a `FuzzController` must provide an `EvidenceReporter`. For tests, provide a `NoopEvidenceReporter` that logs but does nothing. For production, provide a `DaemonEvidenceReporter` that connects to the evidence daemon.

A **CI test** verifies that the `EvidenceReporter` trait is implemented by at least one concrete struct and that the `NoopEvidenceReporter` logs a warning about being a no-op (so it's never accidentally used in production).

### Implementation Checklist

1. **Resolve F12** — create workspace `Cargo.toml` with `bugswarm-evidence` as a dependency of `bugswarm-sandbox`.

2. **Create** `bugswarm-sandbox/src/evidence_reporter.rs`:
   - Define `pub trait EvidenceReporter: Send + Sync`
   - `fn report_trigger_condition(&self, bug_id: &str, crash: &FuzzCrash) -> Vec<TriggerCondition>`
   - Implement `NoopEvidenceReporter` for tests
   - Implement `DaemonEvidenceReporter` for production

3. **Modify** `bugswarm-sandbox/src/fuzzer.rs:516-529` (`FuzzController` struct):
   - Add `reporter: Box<dyn EvidenceReporter>` field

4. **Modify** `bugswarm-sandbox/src/fuzzer.rs:530-566` (`FuzzController::new`):
   - Add `reporter: Box<dyn EvidenceReporter>` parameter
   - Store in struct

5. **Modify** `bugswarm-sandbox/src/fuzzer.rs:602-681` (`record_crash`):
   - After line 676 (`self.crashes.push(crash.clone());`), call `self.reporter.report_trigger_condition(&bug_id, &crash)`
   - `bug_id` = format!("FUZZ-{}", crash.crash_id)

6. **Modify** `bugswarm-sandbox/src/daemon.rs:159-175` (fuzz handler):
   - Construct `DaemonEvidenceReporter` and pass to `FuzzController::new()`

7. **Modify** `bugswarm-sandbox/src/lib.rs:10`:
   - Add `pub mod evidence_reporter;`

8. **Update tests** at `bugswarm-sandbox/tests/fuzzer_test.rs`:
   - Update all `FuzzController::new()` calls to pass `Box::new(NoopEvidenceReporter)`

9. **Create** `bugswarm-sandbox/tests/evidence_reporter_test.rs`:
   - Test: `FuzzCrash` → `TriggerCondition` mapping produces correct dimension/layer
   - Test: `NoopEvidenceReporter` produces zero conditions (returns empty vec)
   - Test: Mock `EvidenceReporter` receives call with correct bug_id and crash data

### Success Criteria

- [ ] `FuzzController` cannot be constructed without an `EvidenceReporter` (compile-time enforcement)
- [ ] When a unique crash is recorded, `report_trigger_condition` is called with the crash data
- [ ] Fuzzer-created `TriggerCondition` has `layer = ContributionLayer::Fuzzer`
- [ ] Crash data maps to at least 2 trigger dimensions (Input + DataState)
- [ ] `NoopEvidenceReporter` correctly returns empty and logs a warning
- [ ] Existing fuzzer tests pass with `NoopEvidenceReporter` injected

---

## Finding F12: No cross-crate dependency — Sandbox can't import evidence types

**Severity:** CRITICAL (enabler of F1, F7, F11)
**Module:** Missing file — no workspace `Cargo.toml` at `/root/a/`
**Impact:** Each Rust crate (`bugswarm-cpg`, `bugswarm-sandbox`, `bugswarm-evidence`) has its own independent `Cargo.toml` and `Cargo.lock`. There is no workspace. Consequently:
- `bugswarm-sandbox` cannot add `bugswarm-evidence` as a dependency (no `[dependencies] bugswarm-evidence = { path = "../bugswarm-evidence" }`)
- Types like `TriggerCondition`, `TriggerDimension`, `ContributionLayer` cannot be imported
- The fuzzer cannot create trigger conditions (F11)
- The sandbox daemon cannot call evidence daemon types directly

This is the **root enabler** of F1, F11, and limits the scope of F7/F8/F10.

### Root Cause Analysis

The project was developed as three independent Rust crates. Each was created with `cargo init` independently, following a "microservices" pattern where crates communicate only over the network (daemon sockets). However, the **types** (TriggerCondition, TriggerDimension, etc.) need to be shared at the type level — not just as serialized JSON over a socket. The absence of a workspace Cargo.toml makes this impossible. The decision to keep crates independent was architectural neglect, not a deliberate design choice.

### Permanent Fix

**Create a workspace-level `Cargo.toml`** and extract shared types into a common crate:

1. **Create `/root/a/Cargo.toml`** (workspace manifest):
   ```toml
   [workspace]
   resolver = "2"
   members = [
       "bugswarm-types",
       "bugswarm-cpg",
       "bugswarm-sandbox",
       "bugswarm-evidence",
   ]
   ```

2. **Extract shared types** into a new crate `bugswarm-types`:
   - `TriggerDimension`, `ContributionLayer`, `TriggerCondition`, `TriggerMatrix` (from `bugswarm-evidence/src/trigger.rs`)
   - `NodeKind`, `EdgeKind`, `EvidenceNode`, `EvidenceEdge`, `NodeId`, `AgentScore`, `GraphStats` (from `bugswarm-evidence/src/types.rs`)
   - `DangerMapEntry`, `DangerConfig` (from `bugswarm-sandbox/src/danger_map.rs` and `bugswarm-cpg/src/danger_map.rs`)

3. **Update each crate's `Cargo.toml`** to depend on `bugswarm-types`:
   ```toml
   [dependencies]
   bugswarm-types = { path = "../bugswarm-types" }
   ```

4. **Update imports** in all `.rs` files that reference shared types to use `bugswarm_types::` instead of `crate::` for those types.

5. **Remove duplicate `danger_map.rs` implementations** — there are two different `DangerConfig`/`DangerMap` implementations (one in `sandbox`, one in `cpg`). Unify under `bugswarm-types`.

### Immunity Mechanism

**Workspace-level dependency ordering test + CI gate.** A CI step runs:

```bash
cargo check --workspace --all-features
```

This verifies that every crate in the workspace compiles with every other crate's types accessible. If a crate references a type that doesn't exist in `bugswarm-types`, compilation fails.

Additionally, a **build-time lint** (custom cargo subcommand or CI script) verifies:

```bash
# Verify no crate has a Cargo.lock at its own level (only workspace-level lock)
for crate in bugswarm-*; do
  if [ -f "$crate/Cargo.lock" ]; then
    echo "ERROR: $crate has its own Cargo.lock — use workspace-level lock only"
    exit 1
  fi
done
```

And a **type-sharing validation** that verifies the `bugswarm-types` crate `lib.rs` re-exports every `pub` type from each crate's public API:

```rust
// bugswarm-types/src/lib.rs
// Re-exports every shared type; CI verifies this file is complete
pub use bugswarm_evidence_types::*;
pub use bugswarm_shared_danger_map::*;
// etc.
```

A CI script diff's `bugswarm-types/src/lib.rs` against the union of `pub` exports from all crates and fails if any type is missing.

### Implementation Checklist

1. **Create** `/root/a/Cargo.toml` with workspace declaration:
   ```toml
   [workspace]
   resolver = "2"
   members = ["bugswarm-types", "bugswarm-cpg", "bugswarm-sandbox", "bugswarm-evidence"]
   ```

2. **Create** `/root/a/bugswarm-types/Cargo.toml`:
   ```toml
   [package]
   name = "bugswarm-types"
   version = "0.1.0"
   edition = "2021"
   
   [dependencies]
   serde = { version = "1.0", features = ["derive"] }
   chrono = { version = "0.4", features = ["serde"] }
   uuid = { version = "1.11", features = ["v4", "serde"] }
   sha2 = "0.10"
   hex = "0.4"
   ```

3. **Create** `/root/a/bugswarm-types/src/lib.rs`:
   - Re-export all shared types from submodules
   - `pub mod trigger_types;` — TriggerDimension, ContributionLayer, TriggerCondition, TriggerMatrix
   - `pub mod evidence_types;` — NodeKind, EdgeKind, EvidenceNode, EvidenceEdge, etc.
   - `pub mod danger_map_types;` — DangerConfig, DangerMap (unified version)
   - Flatten re-exports: `pub use trigger_types::*;`

4. **Extract types** from `bugswarm-evidence/src/trigger.rs` into `bugswarm-types/src/trigger_types.rs`:
   - Move lines 1-177 (`TriggerDimension`, `ContributionLayer`, `TriggerCondition`, `TriggerMatrix` with all impls)
   - Keep `TriggerManager` and utility functions (`normalize_description`, `is_semantically_equivalent`) in `bugswarm-evidence`

5. **Extract types** from `bugswarm-evidence/src/types.rs` into `bugswarm-types/src/evidence_types.rs`:
   - Move all `NodeKind`, `EdgeKind`, `EvidenceNode`, `EvidenceEdge`, etc.
   - Keep `EvidenceNode::seal()` method (it depends on `sha2` only, not on `EvidenceGraph`)

6. **Unify danger_map types** — compare `bugswarm-sandbox/src/danger_map.rs` and `bugswarm-cpg/src/danger_map.rs` and merge into `bugswarm-types/src/danger_map_types.rs`.

7. **Update** `bugswarm-evidence/Cargo.toml`:
   - Add `bugswarm-types = { path = "../bugswarm-types" }` to `[dependencies]`

8. **Update** `bugswarm-sandbox/Cargo.toml`:
   - Add `bugswarm-types = { path = "../bugswarm-types" }` to `[dependencies]`
   - Add `bugswarm-evidence = { path = "../bugswarm-evidence" }` to `[dependencies]` (needed for F11)

9. **Update** `bugswarm-cpg/Cargo.toml`:
   - Add `bugswarm-types = { path = "../bugswarm-types" }` to `[dependencies]`

10. **Update all imports** across the codebase:
    - In `bugswarm-evidence/src/trigger.rs`: `use bugswarm_types::trigger_types::*;` for the types that were moved out
    - In `bugswarm-evidence/src/types.rs`: `use bugswarm_types::evidence_types::*;`
    - In `bugswarm-evidence/src/graph.rs`: import from `bugswarm_types`
    - In `bugswarm-sandbox/src/fuzzer.rs`: import `bugswarm_types::trigger_types::*` (after F11)
    - In `bugswarm-sandbox/src/danger_map.rs`: import unified types
    - In `bugswarm-cpg/src/danger_map.rs`: import unified types

11. **Remove** individual `Cargo.lock` files:
    - `rm bugswarm-evidence/Cargo.lock bugswarm-cpg/Cargo.lock bugswarm-sandbox/Cargo.lock`

12. **Run** `cargo check --workspace --all-features` to verify compilation.

13. **Create CI script** at `scripts/verify_workspace.sh`:
    ```bash
    #!/bin/bash
    set -e
    # Verify no per-crate Cargo.lock
    for crate in bugswarm-{cpg,sandbox,evidence,types}; do
      if [ -f "$crate/Cargo.lock" ]; then
        echo "ERROR: $crate/Cargo.lock exists — use workspace-level lock only"
        exit 1
      fi
    done
    cargo check --workspace --all-features
    cargo test --workspace
    ```

### Success Criteria

- [ ] `/root/a/Cargo.toml` exists and defines `[workspace]` with all 4 members
- [ ] `bugswarm-types` crate exists with `pub use` re-exports of all shared types
- [ ] `cargo check --workspace` passes for all crates
- [ ] `cargo test --workspace` passes for all crates
- [ ] No crate has its own `Cargo.lock` (only workspace-level)
- [ ] `bugswarm-sandbox/src/fuzzer.rs` can import `bugswarm_types::trigger_types::TriggerCondition` (enables F11)
- [ ] `bugswarm-evidence/src/trigger.rs` `TriggerManager` has `use bugswarm_types::trigger_types::*` for types moved to shared crate

---

## Systemic Architecture Fix: Phase Integration by Default

Beyond the 7 individual fixes, a system-level architectural change is needed so that adding a new phase module automatically enforces integration rather than requiring manual wiring for every connection point.

### The Problem

Every new module (like `trigger.rs`) follows the same failure pattern:
1. Module is developed in isolation.
2. Types are added to `NodeKind`/`EdgeKind` enums.
3. Daemon handlers, agent tools, and orchestrator hooks are NOT added.
4. Cross-crate dependencies are NOT declared.
5. The module compiles and tests pass, giving a false sense of completion.

### The Solution: `PhaseTrait` — a mandatory integration contract

Create a Rust trait that **every phase module** must implement. The trait defines all required integration points, and failing to implement any method is a **compile error**.

```rust
// bugswarm-types/src/phase_trait.rs (in the shared types crate)
use std::sync::Arc;

pub trait PhaseModule: Send + Sync {
    /// Unique phase identifier (e.g., "phase23_trigger")
    fn phase_id(&self) -> &'static str;

    /// Daemon handler methods this module exposes (method_name → handler_fn)
    fn daemon_handlers(&self) -> Vec<(&'static str, DaemonHandlerFn)>;

    /// Agent tool definitions this module provides (tool_name → parameter_schema)
    fn agent_tool_schemas(&self) -> Vec<(&'static str, serde_json::Value)>;

    /// Orchestrator hooks this module hooks into
    fn orchestrator_hooks(&self) -> Vec<OrchestratorHook>;

    /// Cross-crate dependencies this module requires
    fn required_dependencies(&self) -> Vec<&'static str>;

    /// Node kinds this module adds to the evidence graph
    fn node_kinds(&self) -> Vec<NodeKind>;

    /// Edge kinds this module adds to the evidence graph
    fn edge_kinds(&self) -> Vec<EdgeKind>;
}
```

Then, in `bugswarm-evidence/src/lib.rs`:

```rust
// At compile time, verify every module in this crate implements PhaseModule
#[cfg(test)]
mod phase_integration_checks {
    use bugswarm_types::phase_trait::PhaseModule;

    #[test]
    fn all_modules_implement_phase_trait() {
        // Each module's PhaseModule implementation is verified
        let trigger = crate::trigger::TriggerPhaseModule::new();
        assert!(!trigger.daemon_handlers().is_empty(), "trigger module has no daemon handlers");
        assert!(!trigger.agent_tool_schemas().is_empty(), "trigger module has no agent tools");
        assert!(!trigger.orchestrator_hooks().is_empty(), "trigger module has no orchestrator hooks");
        assert!(!trigger.required_dependencies().is_empty(), "trigger module declares no dependencies");
    }

    #[test]
    fn all_node_kinds_covered_by_modules() {
        // Iterate all NodeKind variants, verify each is returned by some module's node_kinds()
        let all_kinds = [NodeKind::Claim, NodeKind::Prediction, NodeKind::SandboxRun,
                         NodeKind::CodeLocation, NodeKind::Agent, NodeKind::ConfirmedBug,
                         NodeKind::TriggerMatrix, NodeKind::TriggerCondition];
        // ... verify each has an owner module
    }
}
```

### The Agent-Side Equivalent: `ToolProvider` Protocol

```python
# bugswarm-agent/src/agent/tool_provider.py
from typing import Protocol, runtime_checkable

@runtime_checkable
class ToolProvider(Protocol):
    service_name: str  # e.g., "evidence"
    def register_tools(self, registry: ToolRegistry) -> None: ...
    def health_check(self) -> bool: ...
```

Each service client (CPGClient, SandboxClient, EvidenceClient) implements `ToolProvider`. The agent's `setup_tools()` function auto-discovers all `ToolProvider` implementations and calls `register_tools()`:

```python
# wiring.py — auto-discover all ToolProviders
def setup_tools(repo_path: Path, auto_discover: bool = True) -> ToolRegistry:
    registry = ToolRegistry()
    # Manual registration for backward compatibility:
    cpg = CPGClient()
    sandbox = SandboxClient()
    evidence = EvidenceClient()
    
    for provider in [cpg, sandbox, evidence]:
        provider.register_tools(registry)
    
    if auto_discover:
        # Find all classes implementing ToolProvider and register them
        ...
    
    return registry
```

### The Daemon Equivalent: `HandlerRegistry` with Verification

```rust
// bugswarm-evidence/src/daemon_registry.rs
pub struct DaemonRegistry {
    handlers: HashMap<String, Box<dyn DaemonHandler>>,
}

impl DaemonRegistry {
    /// Build the full registry — every module MUST register here.
    /// This function is the single source of truth for all RPC methods.
    pub fn build() -> Self {
        let mut registry = Self { handlers: HashMap::new() };
        
        // Graph module handlers (always required)
        crate::daemon_graph::register_handlers(&mut registry);
        // Trigger module handlers
        crate::daemon_trigger::register_handlers(&mut registry);
        
        registry
    }
}

#[cfg(test)]
mod verification_tests {
    #[test]
    fn all_required_methods_registered() {
        let registry = DaemonRegistry::build();
        let required = get_required_methods_from_phase_modules();
        for method in &required {
            assert!(registry.handlers.contains_key(method),
                "Missing daemon handler for method: {} — add to DaemonRegistry::build()", method);
        }
    }
}
```

### CI/CD Integration Gates

1. **`cargo test --workspace -- phase_integration`** — Runs all integration-verification tests. Must pass.
2. **`python -m pytest agent/tests/test_tool_audit.py`** — Verifies no stubs in agent tools. Must pass.
3. **`python -m pytest swarm/tests/test_confirmation_hooks.py`** — Verifies orchestrator hooks. Must pass.
4. **`scripts/verify_workspace.sh`** — Verifies no per-crate Cargo.lock, workspace compiles. Must pass.
5. **`scripts/verify_client_schema.py`** — Verifies evidence API schema matches daemon + client. Must pass.

These gates are enforced in CI and **block PR merge** if any fail.

### The New Module Addition Workflow

After these changes, adding a new phase module (e.g., Phase 24 "Differential Testing") follows this workflow:

1. Create the module in the appropriate crate.
2. Add shared types to `bugswarm-types`.
3. Implement `PhaseModule` trait for the new module (compile-time enforced).
4. Add daemon handlers via `register_handlers()` in `DaemonRegistry::build()`.
5. Add agent `ToolProvider` on the Python side.
6. Add `BugConfirmationHook` if the module hooks into bug confirmation.
7. Update `DaemonRequest` struct (or use a generic `Value` field for extension data).
8. Run `cargo test --workspace -- phase_integration` — any missing integration point fails the test.
9. Run `python -m pytest agent/tests/test_tool_audit.py` — any stub tool fails.
10. Merge.

The **architectural invariant** is: a module that passes compile + CI cannot be unintegrated.

---

## Dependency Graph of Fixes

```
F12 (Workspace Cargo.toml)
  ├── enables F11 (Fuzzer can import evidence types)
  ├── enables F1  (TriggerManager can reference EvidenceGraph types directly)
  └── simplifies F7 (Daemon can import trigger types for handler)

F9 (EvidenceClient in Python)
  ├── enables F8  (Agent tools can call real backend instead of stubs)
  └── enables F10 (Orchestrator can call EvidenceClient from confirmation hooks)

F7 (Daemon trigger RPCs)
  ├── enables F9  (EvidenceClient has a backend to connect to)
  └── enables F8  (Agent tools can reach daemon via client)
```

**Recommended implementation order:** F12 → F7 → F9 → F1 → F8 → F10 → F11 → Systemic Fix (PhaseTrait)

---

## Summary Table

| Finding | Module | Fix Type | Key File(s) | Immunity |
|---------|--------|----------|-------------|----------|
| F1 | Evidence graph integration | Bridge module | `trigger_graph_bridge.rs` (new) | `TriggerStorage` trait |
| F7 | Daemon handlers | Handler registry | `daemon_registry.rs` (new), `daemon_trigger.rs` (new) | CI test enumerates required methods |
| F8 | Agent tool stubs | EvidenceClient integration | `wiring.py:337-371` | Tool audit test + `stub` flag |
| F9 | Missing evidence client | New Python module | `evidence_client.py` (new) | Schema-driven code generation |
| F10 | Orchestrator hooks | Confirmation hook protocol | `orchestrator.py:290`, `hooks.py` (new) | `BugConfirmationHook` protocol + validation |
| F11 | Fuzzer trigger contribution | EvidenceReporter trait | `fuzzer.rs:530,602`, `evidence_reporter.rs` (new) | Required constructor parameter |
| F12 | Cross-crate dependency | Workspace Cargo.toml | `/root/a/Cargo.toml` (new) | CI workspace check + no per-crate locks |
| Systemic | Phase integration by default | PhaseTrait + ToolProvider + HandlerRegistry | Multiple files | Compile-time trait enforcement + CI gates |
