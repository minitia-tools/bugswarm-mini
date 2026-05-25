# Phase 17: Real Data Flow Analysis — SSA-Based Taint Tracking

**Status**: NOT_STARTED
**Estimated Effort**: 20 hours
**Depends On**: Phase 2 (CPG), Phase 16 (Sanitizer Instrumentation)
**Unblocks**: Phase 21 (Taint-Guided Fuzzing), Phase 27 (Symbolic Execution), Phase 28 (Concolic Execution)

---

## A1. What is being built?

Replace the current substring-based taint detection in the CPG with a Static Single Assignment (SSA) pass that tracks taint through variable assignments, function calls, return values, field accesses, and collection operations. Every variable gets a unique version. Taint propagates through version chains. A variable `x₁` assigned from a source taints every subsequent use of `x₁` until explicitly sanitized.

---

## A2. Which specific gap does it fill?

**Gap ID**: INV-001 (from `invincible.md`)
**Current behavior**: CPG detects taint via substring matching on AST node text. It finds `os.system(user_input)` but completely misses:
```python
x = request.form.get('cmd')     # Source — detected
y = normalize(x)                 # Transform — taint lost
z = build_command(y)             # Wrap — taint lost
os.system(z)                     # Sink — MISSED because 'z' ≠ 'request'
```
**Target behavior**: Taint flows from source through every intermediate variable, function call, return value, and data structure until it reaches a sink or is explicitly sanitized. `z` at line 4 IS tainted because it was assigned from `y₁` which was assigned from `x₁` which was assigned from the source.

---

## A3. What is the success criteria?

| Metric | Target | Measurement |
|--------|--------|-------------|
| Taint paths found (vs current) | ≥3x more on extreme-bugs repo | Compare `find_taint_paths().len()` before/after SSA pass |
| False positive rate | <10% (was >40% with substring matching) | Agent confirms or refutes 100 taint paths |
| Taint propagation depth | ≥10 variable assignments | Test with deliberately long chain: `a = src; b = a; c = b; ... j = i; sink(j)` |
| Through-function taint | Working | `foo(tainted)` → `foo`'s body uses tainted parameter → sink reached |
| Through-return taint | Working | `def foo(): return src; x = foo(); sink(x)` — taint propagates |
| Through-collection taint | Working | `list.append(tainted); sink(list[0])` — taint detected |
| Cross-file taint | Working | `from module import func; x = func(tainted); sink(x)` |
| Performance impact | <2x indexing time | Compare CPG stats time before/after SSA pass on 10K-line repo |
| Backward compat | 100% existing tests pass | All 5 CPG integration tests green |

---

## A4. What is the priority and why?

**Priority**: 2nd in the invincibility stack (Phase 17 of 30). Immediately after Sanitizer Instrumentation.
**Justification**: Data flow analysis is the foundation for everything above it. Without accurate taint tracking:
- Fuzzing can't be steered (Layer 13 needs accurate danger maps)
- Symbolic execution aims at wrong targets (Layer 2 needs accurate sink locations)
- Agents waste tokens chasing false positives (substring matching produces ~40% FP rate)
- The entire invincibility stack depends on knowing where data actually flows

**Dependency graph**:
```
Phase 2 (CPG) + Phase 16 (Sanitizers)
    ↓
Phase 17 (Data Flow Analysis)  ← WE ARE HERE
    ↓
Phase 21 (Taint-Guided Fuzzing needs accurate danger maps)
Phase 27 (Symbolic Execution needs accurate sink locations)
Phase 28 (Concolic Execution needs path constraint targets)
```

---

## A5. What is NOT being built?

- NOT inter-procedural whole-program analysis (file-by-file with cross-file edges — full whole-program is a future phase)
- NOT points-to analysis for heap objects (SSA handles stack variables. Heap aliasing is Phase 17b)
- NOT a new CPG binary — this is a pass inside the existing `bugswarm-cpg` crate
- NOT replacing the existing substring matcher — the SSA pass runs AFTER the AST walk and augments/replaces the taint edges
- NOT symbolic execution (that's Phase 27)
- NOT dynamic taint tracking (that requires runtime instrumentation — out of scope)

---

## B1. Integration point?

**Primary**: `bugswarm-cpg/src/graph.rs` — adds new methods to `CodePropertyGraph`
**New files**:
- `bugswarm-cpg/src/ssa.rs` — SSA transform: variable versioning, use-def chains
- `bugswarm-cpg/src/taint.rs` — Taint propagation engine: BFS from sources through SSA edges
- `bugswarm-cpg/src/dataflow.rs` — Data flow graph construction (new graph layer alongside call graph)

**Modified files**:
- `bugswarm-cpg/src/graph.rs:192-211` — `add_node` updated to track SSA versions
- `bugswarm-cpg/src/graph.rs:325-379` — `bfs_taint` updated to use data flow edges
- `bugswarm-cpg/src/parser.rs:462-493` — Assignment handler enhanced to build SSA versions
- `bugswarm-cpg/src/parser.rs:243-272` — Call handler enhanced to propagate taint through parameters
- `bugswarm-cpg/src/lib.rs` — Add `pub mod ssa`, `pub mod taint`, `pub mod dataflow`

---

## B2. Data flow?

```
CPG Indexes Repository
  │
  ├─→ Phase 1: AST Walk (existing)
  │     └─ Parses files, builds call graph, creates nodes
  │
  ├─→ Phase 2: SSA Transform (NEW)
  │     └─ For every function:
  │         ├─ Assign unique version to every variable assignment
  │         │   x = source()  → x₁ (tainted)
  │         │   x = sanitize(x) → x₂ (clean)
  │         ├─ Build use-def chains: x₂ depends on x₁
  │         └─ Create SSA edges in the graph
  │
  ├─→ Phase 3: Data Flow Graph (NEW)
  │     └─ Add data flow edges alongside call edges:
  │         ├─ Assignment edge: variable₁ → variable₂
  │         ├─ Parameter edge: caller_var → callee_param
  │         ├─ Return edge: callee_return → caller_var
  │         ├─ Field edge: obj.field = var → obj.field read
  │         └─ Collection edge: list.append(var) → list[i] for all i
  │
  ├─→ Phase 4: Taint Propagation (NEW)
  │     └─ BFS from all source nodes through data flow edges
  │         ├─ Source → x₁ (assignment)
  │         ├─ x₁ → foo(x₁) (parameter pass)
  │         ├─ foo returns x₁ (return value)
  │         ├─ y₁ = foo(x₁) (return assignment)
  │         └─ sink(y₁) (sink reached via 4 hops through variables)
  │
  └─→ Phase 5: Taint Path Output
        └─ `find_taint_paths()` returns paths with:
            - Path length in SSA hops (not AST nodes)
            - Sanitized flag if any hop passed through a sanitizer
            - Confidence based on SSA certainty (0.95 for static, 0.7 for dynamic/heap)
```

---

## B3. New types/schemas?

### Rust: SSA types (in `bugswarm-cpg/src/ssa.rs`)

```rust
/// An SSA variable version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsaVariable {
    pub name: String,           // Original variable name ("x")
    pub version: u32,           // SSA version (1, 2, 3...)
    pub node_id: NodeId,        // Corresponding CPG node
    pub defined_at: usize,      // Line number where defined
    pub used_at: Vec<usize>,    // Lines where this version is used
}

/// A use-def chain link.
#[derive(Debug, Clone)]
pub struct UseDefEdge {
    pub use_var: (String, u32),   // (name, version) that uses
    pub def_var: (String, u32),   // (name, version) that defines
    pub edge_type: UseDefType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UseDefType {
    Assignment,      // y = x
    CallParameter,   // foo(x) — x passed as parameter
    ReturnValue,     // return x — x returned to caller
    FieldStore,      // obj.f = x
    FieldLoad,       // y = obj.f
    CollectionAdd,   // list.append(x)
    CollectionGet,   // y = list[i]
}
```

### Rust: Taint propagation result (in `bugswarm-cpg/src/taint.rs`)

```rust
/// Result of SSA-based taint propagation.
#[derive(Debug, Clone, Serialize)]
pub struct SsaTaintResult {
    pub taint_paths: Vec<SsaTaintPath>,
    pub stats: SsaTaintStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct SsaTaintPath {
    pub source: NodeId,
    pub sink: NodeId,
    pub path: Vec<SsaHop>,       // Ordered list of SSA hops
    pub sanitized: bool,
    pub sanitizer: Option<NodeId>,
    pub length: usize,
    pub confidence: f64,         // 0.95 for static SSA, lower for heap/dynamic
}

#[derive(Debug, Clone, Serialize)]
pub struct SsaHop {
    pub from_var: String,        // "x₁"
    pub to_var: String,          // "y₂"
    pub hop_type: UseDefType,
    pub location: String,        // "file.py:42"
}

#[derive(Debug, Clone, Serialize)]
pub struct SsaTaintStats {
    pub total_variables: usize,
    pub ssa_versions: usize,
    pub use_def_chains: usize,
    pub taint_paths_found: usize,
    pub taint_paths_sanitized: usize,
    pub functions_analyzed: usize,
    pub avg_path_length: f64,
}
```

---

## B4. Modified modules?

| File | Change | Impact |
|------|--------|--------|
| `bugswarm-cpg/src/ssa.rs` | NEW — SSA transform engine | Core algorithm |
| `bugswarm-cpg/src/taint.rs` | NEW — Taint propagation engine | Core algorithm |
| `bugswarm-cpg/src/dataflow.rs` | NEW — Data flow graph construction | Graph layer |
| `bugswarm-cpg/src/graph.rs:192-211` | `add_node` tracks SSA version in metadata | Schema change — backward compat via metadata |
| `bugswarm-cpg/src/graph.rs:325-379` | `bfs_taint` uses data flow edges + per-(node,sanitized) visited | Algorithm replacement |
| `bugswarm-cpg/src/graph.rs:382-419` | `stats()` includes SSA stats | Output change |
| `bugswarm-cpg/src/parser.rs:462-493` | Assignment handler builds SSA versions | AST integration |
| `bugswarm-cpg/src/parser.rs:243-272` | Call handler propagates taint through parameters | AST integration |
| `bugswarm-cpg/src/parser.rs:499-545` | Return handler adds return-to-caller data flow edges | AST integration |
| `bugswarm-cpg/src/lib.rs` | Add `pub mod ssa, pub mod taint, pub mod dataflow` | Module registration |
| `bugswarm-cpg/src/main.rs` | Add `Commander::TaintSsa` command for testing | CLI enhancement |
| `bugswarm-cpg/Cargo.toml` | No new dependencies needed | — |

---

## B5. New dependencies?

| Dependency | Version | Purpose | Justification |
|-----------|---------|---------|---------------|
| None | — | — | All data structures use existing `petgraph` and `HashMap`. SSA transform is a custom pass over the existing AST. |

---

## C1. Core algorithm?

### SSA Transform Algorithm

```
function build_ssa(cpg: &mut CodePropertyGraph, function: NodeId) -> SsaResult:
    versions = HashMap<String, u32>()     // variable name → current version
    for each node in function body (in execution order):
        match node.kind:
            Assignment:
                // Left side gets new version
                lhs_name = extract_lhs(node)
                versions[lhs_name] += 1
                new_version = versions[lhs_name]
                ssa_var = SsaVariable(lhs_name, new_version, node.id, line)
                
                // Right side uses current versions of referenced variables
                rhs_vars = extract_variables(extract_rhs(node))
                for var in rhs_vars:
                    if var in versions:
                        current = versions[var]
                        add_use_def_edge((var, current), (lhs_name, new_version), Assignment)
                
                cpg.add_dataflow_edge(rhs_node, lhs_node, Assignment)
            
            Call:
                // Arguments pass current versions to callee parameters
                callee = resolve_callee(node)
                for (arg, param) in zip(node.args, callee.params):
                    if arg in versions:
                        current = versions[arg]
                        add_use_def_edge((arg, current), (param, versions[param]), CallParameter)
                        cpg.add_dataflow_edge(arg_node, param_node, CallParameter)
                
                // Return value gets new version
                if callee.returns_value:
                    lhs_name = assign_target(node)
                    versions[lhs_name] += 1
                    add_use_def_edge((callee.return_var, current), (lhs_name, versions[lhs_name]), ReturnValue)
            
            Return:
                return_var = extract_return_value(node)
                if return_var in versions:
                    add_return_edge_to_all_callers(function, (return_var, versions[return_var]))
            
            FieldStore:
                obj.field = expr
                // Mark field as tainted if expr is tainted
                add_use_def_edge((expr, versions[expr]), (field_key, 0), FieldStore)
            
            FieldLoad:
                x = obj.field
                // x gets taint from obj.field
                versions["x"] += 1
                add_use_def_edge((field_key, 0), ("x", versions["x"]), FieldLoad)
            
            CollectionAdd:
                list.append(expr)
                add_use_def_edge((expr, versions[expr]), (list_key, 0), CollectionAdd)
            
            CollectionGet:
                x = list[i]
                versions["x"] += 1
                add_use_def_edge((list_key, 0), ("x", versions["x"]), CollectionGet)

    return SsaResult(versions, use_def_chains)
```

### Taint Propagation Algorithm

```
function propagate_taint(cpg: &CodePropertyGraph) -> Vec<SsaTaintPath>:
    paths = []
    
    for source in cpg.sources:
        // BFS from source through data flow edges
        queue = [(source, [SsaHop(source)], false, None)]
        visited = HashSet<(NodeId, bool)>()  // (node, sanitized?)
        
        while queue not empty:
            (current, path, sanitized, sanitizer) = queue.pop()
            
            if current in cpg.sinks:
                paths.push(SsaTaintPath(current, path, sanitized, sanitizer))
                if paths.len() >= 20: break
            
            for edge in cpg.dataflow_edges_from(current):
                target = edge.target
                becomes_sanitized = sanitized or is_sanitizer(target)
                key = (target, becomes_sanitized)
                
                if key not in visited:
                    visited.insert(key)
                    queue.push((target, path + [SsaHop(edge)], becomes_sanitized, ...))
    
    return paths
```

**Complexity**: SSA transform: O(N) where N = AST nodes. Taint propagation: O(S × E) where S = sources, E = data flow edges.

---

## C2. Failure modes?

| Failure | Handling | Recovery |
|---------|----------|----------|
| SSA build fails for a function (syntax tree malformed) | Skip function. Log warning. Continue indexing other functions. | Function marked as `SSA_FAILED`. Taint paths from that function use fallback substring matching. |
| Variable referenced before assignment in function | SSA assigns version 0 (implicit "undefined"). Taint not propagated through undefined. | Logged as `SSA_UNINITIALIZED_USE`. Agent may flag this as a potential bug. |
| Recursive function creates infinite SSA chain | Max SSA depth per function: 100. After 100 hops, truncate. | Path marked `SSA_TRUNCATED`. Confidence reduced to 0.5. |
| Cross-file call to function not yet indexed | Deferred resolution. SSA marks as `UNRESOLVED`. On second pass, resolves if function is now indexed. | Two-pass SSA: Pass 1 collects all functions. Pass 2 resolves cross-file edges. |
| Dynamic dispatch (virtual method, Python `getattr`) | SSA creates "soft edges" with multiple possible targets. All targets tainted. | Confidence reduced to 0.6. Annotation: `DYNAMIC_DISPATCH`. |

---

## C3. Edge cases?

| Edge Case | Behavior |
|-----------|----------|
| Global variables | Single SSA version per global. Mutations create new versions. Taint propagates to all readers after mutation. |
| Closure capturing outer variable | Outer variable's SSA version propagated into closure body. Closure treated as function with implicit parameter. |
| Generator functions (`yield`) | Yield points treated as return values. Each `yield` creates a new SSA version for the yielded expression. |
| Exception handlers | Variables assigned in `try` block may not be initialized if exception occurs. SSA inserts φ-nodes at catch boundary. |
| List comprehensions | `[expr for x in iterable]` — `x` gets new SSA version per iteration. Aggregate taint propagated to resulting list. |
| Decorators | `@decorator def func(): ...` — SSA processes `func` first, then applies decorator as a function call: `func = decorator(func)`. |
| Multiple assignment (`a, b = expr`) | Both `a` and `b` get same SSA version from `expr`. Independent taint thereafter. |
| Augmented assignment (`x += 1`) | `x` gets new version. Use-def edge: `x_old → x_new`. Taint preserved (numeric ops don't sanitize). |
| 50-variable chain | `a = src; b = a; c = b; ...; sink(z50)` — SSA must track through all 50. |
| Unicode variable names | Full Unicode support. `привет = source(); sink(привет)` — taint propagates. |

---

## C4. Concurrency?

**Locking strategy**: CPG indexing is single-threaded (files processed sequentially in `index_directory`). SSA transform runs per-function, sequentially. No concurrency concerns in Phase 17.

**Future**: When CPG adds parallel file parsing, SSA transform must be per-function (no shared state between functions except global variables). Global variable SSA versions use `Arc<RwLock<HashMap>>` for thread safety.

**Race conditions**: None in current implementation. Per-function SSA is embarrassingly parallel — no shared mutable state between functions.

---

## C5. Performance budget?

| Metric | Target | Measurement |
|--------|--------|-------------|
| SSA transform time | <500ms per 1000 functions | Wall-clock time of `build_ssa()` |
| Taint propagation time | <200ms for 50-source × 50-sink graph | Wall-clock time of `propagate_taint()` |
| Memory overhead | <50MB for 10K-function codebase | Peak RSS during SSA pass |
| Indexing time increase | <2x current | Compare `cpg stats --repo` before/after SSA |
| Path depth limit | 100 hops | Hard cap per taint path |

---

## C6. Algorithmic Peak Analysis

### C6.1 Algorithm Inventory

| # | Component | Current Approach | Naive/Peak | Peak Algorithm |
|---|-----------|-----------------|------------|----------------|
| 1 | SSA construction | Linear scan per function, `versions[name] += 1`. No φ-nodes. No CFG. | **NAIVE** | → Cytron et al. dominance-frontier SSA (C6.2.1) |
| 2 | Variable extraction | `extract_variable_refs()` — character-by-character scan of raw assignment text | **NAIVE** | → Tree-sitter AST query (C6.2.2) |
| 3 | Taint propagation | Single-function BFS. No inter-procedural propagation through call graph summaries. | **NAIVE** | → Worklist algorithm with call graph summaries (C6.2.3) |
| 4 | Sanitizer detection | `is_sanitizer_node()` — substring matching: `lower.contains("escape")` | **NAIVE** | → AST contract checking + sandbox confirmation (C6.2.4) |
| 5 | Cross-file resolution | Single pass. Functions processed in file order. Cross-file calls → stubs. | **NAIVE** | → Two-pass deferred resolution (C6.2.5) |
| 6 | Confidence scoring | Hardcoded: 0.95 (unsanitized), 0.3 (sanitized). No graduated scale. | **NAIVE** | → Path-sensitive multiplicative confidence (C6.2.6) |

### C6.2 Peak Algorithm Specifications

#### C6.2.1 SSA Construction — Cytron et al. Dominance-Frontier Algorithm

**C6.2.1.1 What is the peak algorithm?**
```
Algorithm: Cytron, Ferrante, Rosen, Wegman, Zadeck (1991) — "Efficiently Computing Static Single Assignment Form and the Control Dependence Graph"
Complexity: O(N × α(N)) where N = AST nodes, α = inverse Ackermann (essentially linear)

Steps:
  1. Build CFG from AST: basic blocks = straight-line code sequences.
     Edges = control flow (branches, loops, function calls).
  2. Compute dominator tree: block A dominates block B if every path
     from entry to B passes through A. Use Lengauer-Tarjan algorithm (O(N log N)).
  3. Compute dominance frontiers: DF(X) = {Y | X dominates a predecessor of Y
     but does not strictly dominate Y}. These are φ-node insertion points.
  4. Insert φ-nodes: for each variable V, for each node in DF(definition of V),
     insert φ(V, ...) at that node.
  5. Rename variables: DFS over dominator tree. Each assignment creates new version.
     φ-node operands reference versions from predecessor blocks.

Fixes the current critical bug:
  if cond: x = "tainted"  → x₁
  else:    x = "clean"    → x₂
  sink(x)                  → x₃ = φ(x₁, x₂) — sees BOTH
                            Currently: sees only x₂ (FALSE NEGATIVE)

Reference: Cytron, R., et al. "Efficiently Computing Static Single Assignment Form..."
           ACM Transactions on Programming Languages and Systems, 13(4), 1991.
```

**C6.2.1.2 Quantitative improvement?**
```
Before (linear scan): 30% false negative rate on conditional taint paths.
                      Zero φ-nodes. Max 3 function call depth.
After (Cytron SSA): <5% false negative rate on conditional taint.
                    Full φ-node support. Arbitrary call depth via summaries.
Improvement: 85% reduction in false negatives for conditional paths.
```

**C6.2.1.3 Edge cases handled?**
```
Naive linear scan:
  - if/else → one branch taint lost (no φ-node)
  - Loops → variable overwrites previous version, taint from prior iteration lost
  - try/except → exception path taint lost
  - Nested conditions → exponential path count, only last assignment tracked

Peak Cytron SSA:
  - φ-nodes at every join point capture all reaching definitions
  - Loops → φ-node at loop header merges entry + back-edge values
  - try/except → φ-node at merge point captures both paths
  - Nested conditions → φ-nodes cascade correctly through dominance frontiers
```

**C6.2.1.4 Verification strategy?**
```
Test suite: 50 Python functions with known taint paths (ground truth).
  - 25 conditional paths (if/else, try/except, match/case)
  - 15 loop paths (for, while, nested loops)
  - 10 nested conditional paths (3+ levels deep)

Assert: Cytron SSA finds ≥95% of ground-truth taint paths.
Assert: Linear scan (current) finds ≤70% of same paths.
Delta = 25% absolute improvement in path discovery.
```

#### C6.2.2 Variable Extraction — Tree-Sitter AST Query

**C6.2.2.1 What is the peak algorithm?**
```
Algorithm: Tree-sitter query on existing AST — not raw string scanning
Complexity: O(N) where N = AST nodes in the expression subtree

Query for assignment LHS:
  (assignment left: (identifier) @lhs) → returns exact variable name

Query for RHS variable refs:
  (identifier) @ref → returns all variable references in subtree

Eliminates:
  - Keyword filtering (tree-sitter distinguishes identifiers from keywords)
  - String splitting fragility (no confusion between "a = b" and "a = b + c * func(d)")
  - Unicode handling (tree-sitter natively handles Unicode identifiers)

Reference: tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries
```

**C6.2.2.2 Quantitative improvement?**
```
Before (string scan): ~85% accuracy on complex expressions (f-string, multiline, walrus operator)
After (tree-sitter query): 100% accuracy on all AST-valid expressions
Improvement: 15 percentage point accuracy gain. Zero false positives.
```

**C6.2.2.3 Edge cases handled?**
```
Naive string scan:
  - "a, b = func()" → extracts "a", "b", "func" correctly (tuple unpack)
  - "x = f'Hello {name}'" → extracts "x", "Hello", "name" — "Hello" is not a variable (FP)
  - "x = (a if cond else b)" → extracts "x", "a", "cond", "b" — "cond" is not assigned to x
  - Unicode: "привет = source()" → works but fragile

Peak tree-sitter:
  - Tuple unpack: query returns both LHS identifiers correctly
  - f-strings: identifiers inside { } are correctly identified as refs, not LHS
  - Ternary: ternary expression structure preserved. "cond" is a condition ref, not RHS.
  - Unicode: fully supported by tree-sitter's Unicode-aware lexer
```

**C6.2.2.4 Verification strategy?**
```
Test corpus: 200 Python files from open-source repos (Django, Flask, FastAPI, requests).
Run both string-scan and tree-sitter extraction on every assignment.
Compare results. Tree-sitter results are ground truth.

Assert: string-scan matches tree-sitter on ≥85% of assignments.
Assert: tree-sitter finds ≥15% more variable refs on complex assignments.
Assert: 0 false positives from tree-sitter (every extracted name is genuinely a variable).
```

#### C6.2.3 Taint Propagation — Inter-Procedural Worklist Algorithm

**C6.2.3.1 What is the peak algorithm?**
```
Algorithm: Context-insensitive inter-procedural taint propagation with worklist
Complexity: O(F × E × D) where F = functions, E = edges per function, D = call depth
            Terminates at fixed point (no new taint discovered).

Phase 1: Bottom-up summary computation
  For each function in reverse topological order (callees before callers):
    For each parameter P:
      Run intra-procedural taint propagation assuming P is tainted
      Record which outputs become tainted (return value, modified globals,
        mutated arguments, exceptions thrown)
    Store result as function_taint_summary[func_name]

Phase 2: Top-down propagation
  Worklist = {source nodes}
  While worklist not empty:
    node = worklist.pop()
    For each edge from node:
      If edge is intra-procedural (DataFlow, Assigns, References):
        Propagate taint to target normally
      If edge is inter-procedural (Calls with parameter mapping):
        For each tainted argument:
          Look up callee's summary for that parameter position
          Mark all outputs from summary as tainted
          Add those outputs to worklist
    If edge reaches a sink:
      Record taint path

Phase 3: Fixed-point iteration
  Repeat Phase 2 until worklist is empty (no new taint discovered).
  This handles recursive and mutually-recursive functions.
```

**C6.2.3.2 Quantitative improvement?**
```
Before (single-function BFS): Max 3 call levels. 20% of cross-function paths found.
After (worklist): Arbitrary call depth (to fixed point). 95% of cross-function paths.
Improvement: 75 percentage point gain in cross-function path discovery.
```

**C6.2.3.3 Edge cases handled?**
```
Naive BFS:
  - a() → b() → c() → d() → sink — stops at c() (depth limit)
  - Recursive function — infinite loop without depth cap
  - Mutually recursive a() ↔ b() — neither analyzed correctly

Peak worklist:
  - Fixed point covers arbitrary depth
  - Recursion terminates at fixed point (no new taint after N iterations)
  - Mutual recursion: summaries converge after 2-3 iterations
```

**C6.2.3.4 Verification strategy?**
```
Test suite: 30 functions with known cross-file taint paths.
  - 10 direct: caller → callee (1 level)
  - 10 chained: a → b → c → d → sink (4 levels)
  - 5 recursive: a → a (self-call)
  - 5 mutually recursive: a → b → a

Compare: worklist vs single-function BFS.
Assert: worklist finds ≥95% of paths. BFS finds ≤40%.
```

#### C6.2.4 Sanitizer Detection — AST Contract Checking

**C6.2.4.1 What is the peak algorithm?**
```
Algorithm: Multi-factor sanitizer classification combining AST analysis + sandbox confirmation
Complexity: O(F) per function where F = function body size

Factors (combined with weighted scoring):
  1. AST-level check (weight 0.4):
     - Does the function RETURN a new value derived from its input?
       (sanitizers produce output; passthroughs return the input unchanged)
     - Does the function call a KNOWN sanitizer? (html.escape, bleach.clean, shlex.quote)
       → If yes, score += 0.4 immediately
     - Does the function call a KNOWN sink? (exec, system, eval)
       → If yes, NOT a sanitizer (score = 0)

  2. Library membership (weight 0.3):
     - Is the function from a recognized sanitization library?
       (html, bleach, markupsafe, django.utils.html, urllib.parse, mysql_real_escape)
     - Does the function name match known patterns?
       (escape, sanitize, clean, strip_tags, encode, quote, filter)

  3. Sandbox confirmation (weight 0.3):
     - Submit tainted input to the function in sandbox
     - Check output against known dangerous patterns (SQL metacharacters, HTML tags, shell metacharacters)
     - If output is safe for ALL tested dangerous patterns → CONFIRMED sanitizer

Sanitizer threshold: score ≥ 0.7 → classified as sanitizer.
```

**C6.2.4.2 Quantitative improvement?**
```
Before (substring): ~15% false positive rate on sanitizer detection.
                    "unescape_html_entities" flagged as sanitizer (contains "escape").
After (AST + sandbox): <2% false positive rate.
                    "unescape" correctly NOT classified (it REVERSES sanitization).
Improvement: 87% reduction in sanitizer false positives.
```

**C6.2.4.3 Edge cases handled?**
```
Naive substring:
  - "unescape" → matches "escape" → FALSE POSITIVE (reverses sanitization)
  - "escape_room" → matches "escape" → FALSE POSITIVE (unrelated function)
  - "my_escape" → matches "escape" → MAYBE (user-defined, unknown behavior)

Peak AST + sandbox:
  - "unescape" → AST shows it reverses html.escape output → NOT sanitizer
  - "escape_room" → sandbox: outputs contain HTML tags → NOT sanitizer
  - "my_escape" → sandbox: outputs safe for all tests → CONFIRMED sanitizer
```

**C6.2.4.4 Verification strategy?**
```
Test set: 50 functions — 25 known sanitizers, 25 known non-sanitizers.
Run both substring and AST+sandbox classification.
Assert: AST+sandbox accuracy ≥98%. Substring accuracy ≤85%.
Assert: Zero false positives on "unescape", "escape_room", "unescape_html".
```

#### C6.2.5 Cross-File Resolution — Two-Pass Deferred Resolution

**C6.2.5.1 What is the peak algorithm?**
```
Algorithm: Two-pass SSA with deferred cross-file edge resolution
Complexity: O(F × 2) where F = total functions across all files

Pass 1: Collection
  For each file (in any order):
    Parse AST. Extract function signatures:
      - Function name
      - Parameter names and types
      - Return type annotation (if present)
      - File path
      - Line number
    Store in GlobalFunctionRegistry {name → (file, signature)}

Pass 2: Resolution
  For each file (in any order):
    For each function:
      Build SSA body (intra-procedural, as before)
      For each call site:
        callee_name = extract_callee(call_node)
        If callee_name in GlobalFunctionRegistry:
          Resolve cross-file edge with confidence 0.9
          Map caller arguments to callee parameters by position
          Create ParameterPass use-def edge
        Else:
          Create external stub with confidence 0.5 (library/dependency call)

Edge confidence:
  0.95 — Same-file, statically resolvable
  0.90 — Cross-file, resolved via registry
  0.70 — Cross-file, dynamic import (importlib, __import__)
  0.50 — External (stdlib, dependency)
```

**C6.2.5.2 Quantitative improvement?**
```
Before (single pass): Cross-file calls → stubs with confidence 0.5. 0% resolved.
After (two-pass): Cross-file calls → resolved edges with confidence 0.9. ~80% resolved.
Improvement: 80% of cross-file calls resolved vs 0%.
```

**C6.2.5.3 Edge cases handled?**
```
Naive single pass:
  - file_b imports from file_a → file_a processed first → works
  - file_a imports from file_b → file_b not yet processed → STUB (lost)
  - Circular import → both lost

Peak two-pass:
  - All functions collected in pass 1 regardless of order
  - All cross-file edges resolved in pass 2
  - Circular imports: edges created in both directions (both resolved)
```

**C6.2.5.4 Verification strategy?**
```
Test repo: 5 files, 20 functions with known cross-file call chains.
Compare single-pass vs two-pass SSA.
Assert: two-pass resolves ≥80% of cross-file edges. Single-pass resolves 0-30%.
Assert: all circular imports resolved correctly.
```

#### C6.2.6 Confidence Scoring — Path-Sensitive Multiplicative Propagation

**C6.2.6.1 What is the peak algorithm?**
```
Algorithm: Confidence degrades multiplicatively with each hop type.
Complexity: O(1) per hop (already computed during propagation).

Hop type → confidence multiplier:
  Direct assignment (x = y):             ×0.99
  Through function parameter:           ×0.95
  Through function return:              ×0.95
  Through field store/load:             ×0.85  (heap aliasing possible)
  Through collection add/get:           ×0.80  (index uncertainty)
  Through conditional (φ-node):         ×0.70  (path uncertainty)
  Through dynamic dispatch:             ×0.50  (getattr, virtual method)
  Through loop-carried dependency:      ×0.60  (iteration count unknown)
  Through global variable:              ×0.75  (concurrent modification possible)

Final confidence = 0.95 (initial) × ∏(hop_multiplier_i)

Example: 10-hop path through 3 fields + 2 conditionals:
  0.95 × 0.85³ × 0.70² = 0.95 × 0.614 × 0.49 = 0.286
  → Agent sees confidence 0.29, investigates accordingly.
```

**C6.2.6.2 Quantitative improvement?**
```
Before (hardcoded): All paths confidence 0.95 or 0.30. No discrimination.
After (multiplicative): Confidence accurately reflects path uncertainty.
  1-hop direct: 0.94  (nearly certain)
  5-hop through function: 0.77  (reasonably certain)
  10-hop through fields: 0.29  (uncertain — needs verification)
Improvement: Agents can PRIORITIZE by confidence. 30% token savings.
```

**C6.2.6.3 Edge cases handled?**
```
Naive hardcoded:
  - A 1-hop path and a 100-hop path both get 0.95 → indistinguishable
  - A sanitized 1-hop path gets 0.30 — lower than a deeply uncertain unsanitized path
  - Agents waste tokens investigating long paths that are likely false

Peak multiplicative:
  - 1-hop path: 0.94 → high priority
  - 100-hop path: 0.95 × 0.80^50 ≈ 0.00001 → effectively zero, ignored
  - Sanitized 1-hop: 0.95 × 0.99 = 0.94, but sanitized=true → both factors visible
```

**C6.2.6.4 Verification strategy?**
```
Gold-standard annotation: 50 taint paths manually labeled with ground-truth confidence.
Assert: Pearson correlation between multiplicative confidence and ground truth > 0.80.
Assert: Hardcoded confidence correlation < 0.30 (no discrimination).
Assert: Top-10 paths by multiplicative confidence contain ≥8 true positives.
```

### C6.3 Zero-Gap Guarantee

```
Component: SSA Construction
  [x] No algorithm implemented naively without peak specification
  [x] NAIVE → Cytron et al. dominance-frontier SSA specified in C6.2.1
  [x] Quantitative target: 85% reduction in false negatives
  [x] Edge cases: φ-nodes, loops, try/except, nested conditions
  [x] Verification: 50-function ground truth test suite

Component: Variable Extraction
  [x] NAIVE → Tree-sitter AST query specified in C6.2.2
  [x] Quantitative target: 100% accuracy (vs 85% for string scan)
  [x] Edge cases: tuple unpack, f-strings, ternary, Unicode
  [x] Verification: 200-file open-source corpus comparison

Component: Taint Propagation
  [x] NAIVE → Worklist algorithm with call summaries specified in C6.2.3
  [x] Quantitative target: 75pp gain in cross-function path discovery
  [x] Edge cases: deep chains, recursion, mutual recursion
  [x] Verification: 30-function cross-file test suite

Component: Sanitizer Detection
  [x] NAIVE → AST contract checking + sandbox confirmation in C6.2.4
  [x] Quantitative target: 87% reduction in sanitizer false positives
  [x] Edge cases: "unescape", unrelated names, user-defined sanitizers
  [x] Verification: 50-function known sanitizer/non-sanitizer test set

Component: Cross-File Resolution
  [x] NAIVE → Two-pass deferred resolution in C6.2.5
  [x] Quantitative target: 80% of cross-file calls resolved (vs 0%)
  [x] Edge cases: forward references, circular imports
  [x] Verification: 5-file, 20-function known chain test

Component: Confidence Scoring
  [x] NAIVE → Path-sensitive multiplicative propagation in C6.2.6
  [x] Quantitative target: Pearson correlation >0.80 with ground truth
  [x] Edge cases: 100-hop path degradation, sanitized-vs-uncertain discrimination
  [x] Verification: 50-path gold-standard annotation
```

### C6.4 Peak Deferral Justification

None of the six deferral reasons apply. All peak algorithms are mandatory.

| Component | Deferral Reason | Status |
|-----------|----------------|--------|
| SSA Construction | None apply | Must implement Cytron et al. |
| Variable Extraction | Tree-sitter already integrated in CPG (Phase 2) | Must implement |
| Taint Propagation | No blocking dependencies | Must implement |
| Sanitizer Detection | Sandbox already available (Phase 1) | Must implement |
| Cross-File Resolution | Two-pass is purely architectural — no new deps | Must implement |
| Confidence Scoring | Purely algorithmic — no new deps | Must implement |


---

## D1. Aggressive unit tests? (25 tests)

| Test Name | Attack Vector | Expected Behavior |
|-----------|---------------|-------------------|
| `test_ssa_simple_assignment_chain` | `a=src; b=a; c=b; d=c; e=d; sink(e)` — 5-hop chain | Taint propagates through all 5 hops. Path length=5. |
| `test_ssa_through_function_call` | `x=src; foo(x)` where `foo(param): sink(param)` | Taint enters `foo` via parameter. Sink reached inside callee. |
| `test_ssa_through_return_value` | `def foo(): return src; x=foo(); sink(x)` | Taint returns from `foo` into `x`. `x` is tainted. |
| `test_ssa_sanitizer_breaks_chain` | `x=src; y=html.escape(x); sink(y)` | Taint stops at sanitizer. Path marked `sanitized=true`. |
| `test_ssa_multiple_variable_versions` | `x=src; x=sanitize(x); sink(x)` — x gets version 2 (clean) | x₁ is tainted, x₂ is clean. Sink uses x₂ → no taint path. |
| `test_ssa_field_store_and_load` | `obj.f=src; x=obj.f; sink(x)` | Taint stored via field, loaded via field. x is tainted. |
| `test_ssa_list_append_and_index` | `lst=[]; lst.append(src); x=lst[0]; sink(x)` | Taint added via append, retrieved via index. x is tainted. |
| `test_ssa_dict_set_and_get` | `d={}; d['k']=src; x=d['k']; sink(x)` | Taint stored via dict, retrieved via dict. x is tainted. |
| `test_ssa_global_variable` | `global g; g=src; foo()` where `foo(): sink(g)` | Global taint propagates across function boundaries. |
| `test_ssa_if_else_both_branches` | `if cond: x=src else: x=safe; sink(x)` | **AGGRESSIVE**: Both branches tracked. x may-or-may-not be tainted. SSA φ-node marks as `TAINTED_CONDITIONALLY`. |
| `test_ssa_for_loop_accumulator` | `for item in items: x += src; sink(x)` | **AGGRESSIVE**: Loop creates multiple x versions. All tainted. Sink reached. |
| `test_ssa_recursive_function_depth_limit` | `def f(n): x=src; f(n-1) if n>0 else sink(x)` | **AGGRESSIVE**: Recursion creates SSA versions. Depth limit enforced at 100. |
| `test_ssa_try_except_finally` | `try: x=src except: x=safe; finally: sink(x)` | **AGGRESSIVE**: Both `try` and `except` paths tracked. x may be tainted. |
| `test_ssa_list_comprehension` | `lst = [src for _ in range(10)]; sink(lst[0])` | Comprehension result is tainted. Index access preserves taint. |
| `test_ssa_closure_capture` | `def outer(): x=src; return lambda: sink(x)` | Closure captures tainted variable. Inner function reaches sink. |
| `test_ssa_generator_yield` | `def gen(): yield src; for x in gen(): sink(x)` | Yielded value is tainted. Loop variable receives taint. |
| `test_ssa_multiple_return_paths` | `def f(flag): return src if flag else safe` | Both return paths tracked. Caller receives conditional taint. |
| `test_ssa_50_variable_chain` | **AGGRESSIVE**: 50 sequential assignments | All 50 versions tracked. Memory <50MB. Time <500ms. |
| `test_ssa_unicode_variable_names` | `привет = source(); sink(привет)` | Unicode variable tracked correctly. |
| `test_ssa_shadowed_variable` | `x=src; def inner(): x=safe; sink(x)` — inner `x` shadows outer | Inner `x` is clean. Outer `x` is tainted. No false positive. |
| `test_ssa_augmented_assignment` | `x=src; x+="suffix"; sink(x)` | **AGGRESSIVE**: `x +=` creates new version. Taint preserved (string concat doesn't sanitize). |
| `test_ssa_empty_function` | `def f(): pass` | No SSA variables created. No crash. |
| `test_ssa_function_with_only_returns` | `def f(): return 42` | SSA tracks return expression. No taint (42 is a literal). |
| `test_ssa_concurrent_indexing` | **AGGRESSIVE**: Parse 100 files simultaneously | All SSA transforms complete. No shared mutable state corruption. |
| `test_ssa_backward_compat` | Run existing 5 CPG tests after SSA pass | All 5 pass. Existing call graph unaffected. |

**Total: 25 unit tests. 17 standard + 8 aggressive.**

---

## D2. Aggressive integration tests? (7 tests)

| Test Name | Attack Vector | Expected Behavior |
|-----------|---------------|-------------------|
| `test_ssa_taint_finds_deep_bug` | extreme-bugs repo with `x=src; y=wrap(x); z=build(y); exec(z)` | Current system: 0 taint paths. SSA system: ≥1 taint path with 4+ hops. |
| `test_ssa_cross_file_taint` | `a.py: import b; x=src; b.process(x)` → `b.py: def process(data): sink(data)` | Taint crosses file boundary via import. Cross-file edge created. |
| `test_ssa_real_world_pattern` | Django view: `request.POST.get('name')` → form → model.save() → SQL | **AGGRESSIVE**: Full Django-style chain. Source at HTTP handler, sink at ORM. |
| `test_ssa_false_positive_rate` | Run SSA on 10 known-clean repos | **AGGRESSIVE**: <10% false positive rate. Compare with existing 40% substring FP rate. |
| `test_ssa_performance_on_large_repo` | **AGGRESSIVE**: 50K-line Python repo | Indexing time <2x baseline. Memory <100MB. |
| `test_ssa_combined_with_sandbox` | Agent identifies SSA taint path → sandbox confirms exploitable | Full loop: SSA taint → agent investigation → PoC → sandbox receipt. |
| `test_ssa_gate_regression` | Run existing Phase 2 gate (20 tests) after SSA | All 20 pass. SSA is additive. |

**Total: 7 integration tests. 3 standard + 4 aggressive.**

---

## D3. Extreme gate test — The Data Flow Crucible? (10 attack vectors)

**MANDATORY**: Designed to BREAK the SSA implementation.

**Design**: A single Python file with 10 deliberately crafted taint scenarios. Each tests a different SSA capability. The file is indexed by CPG with SSA enabled. `find_taint_paths()` must find all 10.

```python
# GATE TEST: The Data Flow Crucible
# Current CPG: finds 2/10 taint paths
# SSA CPG: must find 10/10

import os

# ── Attack 1: Simple assignment chain (5 hops) ──
def bug1_chain():
    a = request_get('cmd')   # Source
    b = a                     # Hop 1
    c = b                     # Hop 2
    d = c                     # Hop 3
    e = d                     # Hop 4
    os.system(e)              # Sink — must find via 5-hop chain

# ── Attack 2: Through function call ──
def bug2_through_call():
    cmd = request_get('cmd')  # Source
    execute_command(cmd)      # Passes to callee

def execute_command(x):
    os.system(x)              # Sink — x is tainted from caller

# ── Attack 3: Through return value ──
def bug3_return():
    cmd = get_user_input()    # get_user_input returns source
    os.system(cmd)

def get_user_input():
    return request_get('cmd') # Source returned to caller

# ── Attack 4: Sanitizer breaks chain ──
def bug4_sanitized():
    cmd = request_get('cmd')  # Source
    safe = sanitize(cmd)      # Sanitizer
    os.system(safe)           # Sink — taint should be SANITIZED

# ── Attack 5: Field store/load ──
class Cmd:
    def __init__(self): self.cmd = ""
obj = Cmd()
def bug5_field():
    obj.cmd = request_get('cmd')  # Source via field store
    os.system(obj.cmd)            # Sink via field load

# ── Attack 6: List append/index ──
def bug6_list():
    cmds = []
    cmds.append(request_get('cmd'))  # Source via append
    os.system(cmds[0])               # Sink via index

# ── Attack 7: Conditional taint ──
def bug7_conditional(flag):
    if flag:
        cmd = request_get('cmd')     # Source (tainted branch)
    else:
        cmd = "safe"                 # Clean (safe branch)
    os.system(cmd)                   # Sink — CONDITIONALLY TAINTED

# ── Attack 8: Loop accumulator ──
def bug8_loop():
    cmd = ""
    for part in request_get_parts():  # Source in loop
        cmd += part                   # Accumulate taint
    os.system(cmd)                    # Sink

# ── Attack 9: Closure capture ──
def bug9_closure():
    cmd = request_get('cmd')          # Source
    runner = lambda: os.system(cmd)   # Closure captures cmd
    runner()                          # Sink via closure

# ── Attack 10: Dictionary store/get ──
def bug10_dict():
    data = {}
    data['cmd'] = request_get('cmd')  # Source via dict
    os.system(data['cmd'])            # Sink via dict

def request_get(key): return "user_input"  # Stub source
def sanitize(x): return x.replace(";", "")  # Stub sanitizer
def request_get_parts(): return ["a", "b"]  # Stub source
```

**Pass condition**: All 10 attack vectors produce taint paths. Attack 4 (sanitized) must have `sanitized=true`. Attack 7 (conditional) must have `confidence < 0.95`. All 10 have `length >= number_of_hops + 1`.

**Fail condition**: Any attack vector missed. Any false negative. Sanitized path not flagged.

**Gate receipt**:
```json
{
  "phase": 17,
  "gate": "data_flow_crucible",
  "attack_vectors": 10,
  "expected_paths": 10,
  "actual_paths": "TBD",
  "false_positives": "TBD",
  "false_negatives": "TBD",
  "verdict": "PHASE 17 PENDING"
}
```

---

## D4. Golden dataset?

**Applicable**: Yes. Run the 200 golden dataset bugs through SSA-based taint tracking. Compare:
- **Before (substring)**: ~40% of bugs detected, ~40% false positive rate
- **After (SSA)**: target >80% of bugs detected, <10% false positive rate

The golden dataset bugs have ground-truth taint paths. SSA must find them.

---

## D5. Regression test?

**Name**: `test_ssa_regression`

**What**: The Data Flow Crucible gate test (D3) runs on every CI push. If any of the 10 attack vectors loses its taint path after a code change, the build fails. This prevents SSA regressions.

**How**: `cargo test --test ssa_regression` in the CPG crate. Uses a fixed test Python file committed to the repo.

---

## E1. Estimated cost?

| Cost Type | Estimate | Assumptions |
|-----------|----------|-------------|
| Development | 20 hours | 1 engineer |
| CPG indexing overhead | 1.5-2x time increase | SSA pass adds per-function analysis |
| Memory overhead | ~20MB for 10K functions | SSA versions stored per function |
| Token cost impact | NEGATIVE (saves tokens) | More accurate taint = fewer false positives for agents to chase. Estimated 30% token savings. |
| Infrastructure | $0 | No new services. Runs inside existing CPG binary. |

---

## E2. Observability?

### Logs

| Level | Message | When |
|-------|---------|------|
| INFO | `ssa_transform_start` | SSA pass begins for a function |
| DEBUG | `ssa_variable_versioned` | Variable assigned new SSA version |
| DEBUG | `ssa_use_def_edge` | Use-def chain created |
| WARN | `ssa_transform_failed` | SSA failed for a function (falls back to substring) |
| WARN | `ssa_depth_limit_reached` | Max SSA depth (100) hit |
| INFO | `ssa_transform_complete` | SSA pass complete with stats |

### Metrics (Prometheus)

```
bugswarm_cpg_ssa_functions_total          counter
bugswarm_cpg_ssa_variables_total          counter
bugswarm_cpg_ssa_use_def_chains_total     counter
bugswarm_cpg_ssa_taint_paths_found        gauge
bugswarm_cpg_ssa_transform_duration_ms    histogram
bugswarm_cpg_ssa_fallback_count           counter  (times substring used instead)
```

---

## E3. Configuration?

| Parameter | Default | Range | Env Var | CLI Flag |
|-----------|---------|-------|---------|----------|
| `ssa_enabled` | `true` | bool | `BGSWARM_SSA_ENABLED` | `--[no-]ssa` |
| `ssa_max_depth` | `100` | 10-500 | `BGSWARM_SSA_MAX_DEPTH` | `--ssa-max-depth` |
| `ssa_max_paths` | `20` | 5-100 | — | `--ssa-max-paths` |
| `ssa_conditional_confidence` | `0.6` | 0.3-0.9 | — | — |

---

## E4. Migration?

**Backward compatibility**: Full. SSA is additive. `find_taint_paths()` returns MORE paths after SSA, not fewer. Existing paths from substring matching are preserved but marked `source="substring"` vs `source="ssa"`. SSA paths have `confidence: 0.95`, substring paths retain `confidence: 0.5`.

**Data migration**: None. CPG is rebuilt from scratch on every index. No persisted graph format to migrate.

**Agent migration**: Agents receive both SSA taint paths and substring paths. They can distinguish by confidence. No agent code change needed.

---

## E5. Documentation?

| Document | Content |
|----------|---------|
| Code docs | Rust doc comments on `SsaVariable`, `UseDefEdge`, `build_ssa()`, `propagate_taint()` |
| User docs | New section in README: "SSA Data Flow Analysis" — what it tracks, how to interpret confidence scores |
| ADR | "ADR-017: SSA-Based Data Flow Analysis Replacing Substring Taint Detection" |
| Changelog | `feat: SSA-based data flow analysis with use-def chains, through-function taint, field/collection tracking` |

---

## Dependency Tree

```
Before: Phase 2 (CPG), Phase 16 (Sanitizers)
This:   Phase 17 (Data Flow Analysis)
After:  Phase 21 (Taint-Guided Fuzzing needs accurate danger maps)
        Phase 27 (Symbolic Execution needs accurate sink locations)
        Phase 28 (Concolic Execution needs path constraint targets)
```

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| SSA transform is too slow on large repos (>100K lines) | Medium | Medium | Per-function SSA is trivially parallelizable. Start single-threaded, add rayon if needed. |
| SSA false negatives (misses real taint paths) | Medium | High | Two-pass SSA: pass 1 collects all functions, pass 2 resolves cross-file edges. Test against golden dataset. |
| SSA increases false positives (flags safe code) | Low | Medium | Conditional taint paths have lower confidence. Agents prioritize high-confidence paths. |
| SSA can't handle dynamic Python features (eval, exec, getattr) | High | Medium | Accept limitation. Mark dynamic paths with `DYNAMIC_DISPATCH` annotation. Confidence reduced to 0.3. |
| SSA breaks existing call graph tests | Low | High | Run Phase 2 gate (20 tests) before/after. SSA is additive. If it breaks existing, fix before proceeding. |

---

## Decision Log

| Decision | Reasoning | Date |
|----------|-----------|------|
| Build SSA as a pass over existing AST, not a new IR | Existing CPG already has AST nodes. Adding a new IR would require migrating everything. Pass approach is incremental. | 2026-05-13 |
| Two-pass SSA: collect functions first, resolve cross-file second | Single-pass can't resolve forward references (function defined later in file, or in another file). Two-pass is standard SSA practice. | 2026-05-13 |
| Conditional taint paths get confidence 0.6 (not 0.95) | φ-nodes at if/else boundaries represent uncertainty. Conservative confidence reflects this. | 2026-05-13 |
| Heap objects (field store/load) get confidence 0.7 | Without points-to analysis, field-based taint is less certain than stack variable taint. Heap aliasing can cause false positives/negatives. | 2026-05-13 |
| Max SSA depth 100 per path | Real-world taint paths are rarely >20 hops. 100 is generous and prevents infinite recursion. | 2026-05-13 |

---

## Review Checklist

- [x] All 25 questions answered
- [x] Aggressive testing mandate met — 25 unit tests (8 aggressive), 7 integration tests (4 aggressive), 10-attack-vector Data Flow Crucible gate
- [ ] Every component individually stress-tested (SSA transform, taint propagation, cross-file, conditional, loop, closure, recursive)
- [ ] Dependency tree verified — Phase 2 + 16, unblocks 21, 27, 28
- [ ] Gate test (Data Flow Crucible) passes at 100% — all 10 attack vectors detected
- [ ] Existing Phase 2 gate (20 tests) still passes
- [ ] Golden dataset: >80% bug detection rate, <10% FP rate
- [ ] No downstream phase blocked
- [ ] Documentation updated

---

## Gate Receipt

```json
{
  "phase": 17,
  "name": "Data Flow Analysis",
  "gate": "data_flow_crucible",
  "timestamp": "TBD",
  "status": "PENDING",
  "total_tests": 10,
  "passed": 0,
  "failed": 0,
  "verdict": "PHASE 17 NOT YET EXECUTED"
}
```

---

## Changelog

| Date | Author | Change |
|------|--------|--------|
| 2026-05-13 | System | Initial plan created from `plan_standard.md` template |
