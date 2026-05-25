# ARIA: Adaptive Reasoning with Information Augmentation

**Version**: 1.0.0
**Date**: 2026-05-18
**Status**: Architecture Specification
**Integration Target**: BugSwarm Agent Loop (`agent/src/agent/loop.py`)
**Replaces**: IEP Engine (Investigate-Explain-Predict loop)

---

## 1.0 Core Insight

Information density and decision constraint are orthogonal axes. You can give a model maximum information without constraining its decision space at all. ARIA's job is to do expensive, deterministic, non-creative work BEFORE the model sees the prompt — static analysis, pattern matching, CVE similarity, capability mapping — so the model spends its compute on the parts that actually require intelligence.

- A **weak model** gets pre-computed leaps it couldn't make itself
- A **frontier model** gets rich data it would have had to gather manually
- **Neither is constrained**

---

## 2.0 Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         ARIA RUNTIME                                 │
│                                                                      │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────────────┐ │
│  │ PreCompEngine │──▶│  ICO Builder │──▶│ Layered Prompt Builder   │ │
│  │ (CPG, CVE,   │   │              │   │                          │ │
│  │  patterns,    │   │ Investigation │   │ Layer 0: Facts           │ │
│  │  waypoints,   │   │   Context     │   │ Layer 1: Patterns        │ │
│  │  embeddings)  │   │   Object      │   │ Layer 2: Waypoints       │ │
│  └──────────────┘   └──────────────┘   │ Capability Grammar       │ │
│                                         │ Open Decision            │ │
│                                         └──────────┬───────────────┘ │
│                                                    │                 │
│                                                    ▼                 │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────────────┐ │
│  │   LLM Call   │◀──│ Model Adapter│◀──│   (sends prompt)         │ │
│  │  (any model) │   │ (Gateway)    │   │                          │ │
│  └──────┬───────┘   └──────────────┘   └──────────────────────────┘ │
│         │                                                            │
│         ▼                                                            │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────────────┐ │
│  │Intent Parser │──▶│  Dispatch    │──▶│   Tool Registry          │ │
│  │(structured + │   │  (intent →   │   │   (execute capability)   │ │
│  │ semantic +   │   │   action)    │   │                          │ │
│  │ constrained) │   │              │   │                          │ │
│  └──────────────┘   └──────┬───────┘   └──────────────────────────┘ │
│                             │                                        │
│                             ▼                                        │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────────────┐ │
│  │ ICO Enricher │◀──│Tool Normalizer│◀──│   Tool Output            │ │
│  │(regenerate   │   │(normalize to  │   │                          │ │
│  │ waypoints,   │   │ common schema)│   │                          │ │
│  │ compress     │   │              │   │                          │ │
│  │ context)     │   │              │   │                          │ │
│  └──────┬───────┘   └──────────────┘   └──────────────────────────┘ │
│         │                                                            │
│         ▼                                                            │
│  ┌──────────────┐                                                    │
│  │ ISG Check    │──▶ Complete? ──▶ InvestigationReport               │
│  │(completion   │──▶ Not done? ──▶ Next cycle (build new prompt)     │
│  │ conditions)  │                                                    │
│  └──────────────┘                                                    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 3.0 The Investigation Context Object (ICO)

The ICO is the central data structure. Every cycle, the same ICO feeds the prompt builder. Every cycle, tool results enrich it. It never changes shape regardless of which model is driving.

### 3.1 ICO Schema

```
┌──────────────────────────────────────────────┐
│        INVESTIGATION CONTEXT OBJECT          │
├──────────────────────────────────────────────┤
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ LAYER 0: RAW FACTS                     │  │
│  │ Pre-computed, zero interpretation      │  │
│  │                                        │  │
│  │ • target: TargetProfile                │  │
│  │   - language, framework, size,         │  │
│  │     entry_points, memory_model         │  │
│  │ • static_facts: StaticAnalysis         │  │
│  │   - AST shape, CFG properties,         │  │
│  │     CPG graph reference                │  │
│  │ • tool_outputs: list[NormalizedOutput] │  │
│  │   - normalized outputs from prior      │  │
│  │     tool calls                         │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ LAYER 1: PATTERN ANNOTATIONS           │  │
│  │ Informational, never prescriptive      │  │
│  │                                        │  │
│  │ • domain_priors: list[DomainPattern]   │  │
│  │   - historical vuln distribution for   │  │
│  │     this language/framework profile    │  │
│  │ • similarity_hits: list[CVEMatch]      │  │
│  │   - pre-computed CVE DB similarity     │  │
│  │ • pattern_matches: list[PatternHit]    │  │
│  │   - pre-run SAST signatures against    │  │
│  │     CPG                                │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ LAYER 2: WAYPOINTS                     │  │
│  │ Optional checkpoints, not required     │  │
│  │                                        │  │
│  │ • waypoints: list[Waypoint]            │  │
│  │   - pre-generated investigation        │  │
│  │     suggestions with supporting data   │  │
│  │   - phrased as questions               │  │
│  │   - explicitly skippable               │  │
│  │   - regenerated after each ICO         │  │
│  │     enrichment                         │  │
│  │ • open_questions: list[Question]       │  │
│  │   - unanswered questions from prior    │  │
│  │     steps                              │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ INVESTIGATION STATE                    │  │
│  │                                        │  │
│  │ • isg: InvestigationStateGraph         │  │
│  │   - nodes = knowledge-states           │  │
│  │   - edges = reasoning-steps            │  │
│  │ • history: list[ModelAction]           │  │
│  │   - full audit trail                   │  │
│  │ • confidence_map: dict[str, float]     │  │
│  │   - per-hypothesis confidence scores   │  │
│  │ • reasoning_trace: list[str]           │  │
│  │   - raw model outputs for context      │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ CAPABILITY GRAMMAR                     │  │
│  │                                        │  │
│  │ • capabilities: CapabilityGrammar      │  │
│  │   - NOT a tool list                    │  │
│  │   - typed capability graph organized   │  │
│  │     by what they do to knowledge       │  │
│  │   - four permanent categories:         │  │
│  │     OBSERVE, TRANSFORM, SYNTHESIZE,    │  │
│  │     VERIFY                             │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
```

### 3.2 TargetProfile

```python
@dataclass
class TargetProfile:
    language: str              # "c", "python", "rust"
    framework: str             # "openssl", "django", "tokio"
    size_lines: int            # total lines of code
    size_functions: int        # total functions
    entry_points: list[str]    # function names reachable from external input
    memory_model: str          # "manual" (C/C++), "gc" (Java/Go), "ownership" (Rust)
    build_system: str          # "make", "cmake", "cargo", "pip"
    test_framework: str        # "pytest", "cargo test", "ctest"
    has_unsafe_blocks: bool    # relevant for Rust
    static_analysis_ready: bool # CPG index complete
```

### 3.3 NormalizedToolOutput

Every tool result is normalized to this schema before entering the ICO:

```python
@dataclass
class NormalizedToolOutput:
    capability_type: str         # OBSERVE, TRANSFORM, SYNTHESIZE, VERIFY
    tool_name: str               # original tool name for audit
    raw_output: Any              # preserved for debugging
    
    # Normalized representations
    code_locations: list[CodeLocation]      # file, line, column, function
    data_flow_paths: list[DataFlowPath]     # source → sink chains
    constraints: list[str]                  # SMT-compatible conditions
    artifacts: list[Artifact]              # files, dumps, traces — typed
    confidence: float                       # 0.0-1.0
    
    # Composition metadata
    consumable_by: list[str]   # which capability categories can consume this
    
    # Context for prompt
    summary: str               # one-line summary for compressed context
    relevance_score: float     # computed vs current investigation focus
```

---

## 4.0 The Capability Grammar

### 4.1 Why a Grammar, Not a Tool List

The model never sees a list of 14 (or 50, or 200) tool names. It sees 4 permanent categories. Tools are mapped into categories once. When 50 new tools are added, the model's reasoning surface stays 4 categories wide.

### 4.2 The Four Permanent Categories

| Category | What It Does to Knowledge | When to Use It |
|----------|--------------------------|----------------|
| **OBSERVE** | Produce new evidence from the target | Starting investigation, gathering data, exploring hypotheses |
| **TRANSFORM** | Reduce, isolate, or clarify existing evidence | Refining findings, minimizing inputs, testing assumptions |
| **SYNTHESIZE** | Create higher-order artifacts from evidence | Building chains, mining patterns, computing invariants |
| **VERIFY** | Check a hypothesis against ground truth | Confirming bugs, validating fixes, cross-referencing claims |

### 4.3 Tool-to-Category Mapping

```
CAPABILITY GRAMMAR

[OBSERVE] → produce new evidence from target
  ├── query_cpg(pattern, scope?)        → subgraphs, dataflow_paths, taint_chains
  ├── trace_dependency(function, depth?) → call_chains, dependency_graphs
  ├── exec_sandbox(poc_code, env?)      → execution_receipts, crash_reports, asan_output
  ├── fuzz_target(target, harness?)     → crashes, coverage_maps, edge_cases
  ├── read_file(path, start?, end?)     → source_text, annotated_code
  ├── list_dir(path)                    → file_listings, metadata
  ├── grep(pattern, scope?)             → search_results, pattern_locations
  ├── glob(pattern)                     → file_matches, relevance_ranked
  └── web_fetch(url)                    → external_knowledge, cve_data, docs

[TRANSFORM] → reduce/isolate existing evidence
  ├── delta_debug(crashing_input)       → minimal_reproducer, isolation_boundary
  ├── run_mutations(function, ops?)     → surviving_mutants, kill_matrix
  └── diff_execute(output_a, output_b)  → diff_reports, semantic_diffs

[SYNTHESIZE] → create higher-order artifacts from evidence
  ├── mine_invariants(function, count?) → inferred_invariants, violations
  ├── suggest_chain(bug_ids)            → chain_candidates, feasibility_scores
  ├── explore_paths(function, queries?) → branch_coverage, uncovered_paths
  └── solve_reachability(target, conds) → concrete_inputs, constraint_solutions

[VERIFY] → check hypothesis against ground truth
  ├── describe_trigger(bug_id, dim)     → trigger_conditions, documentation
  ├── get_trigger_matrix(bug_id)        → completeness_reports, verification_state
  └── predict_fix_impact(fix_proposal)  → caller_impacts, regression_tests, confidence
```

### 4.4 Grammar Self-Description

Every capability specifies its input and output types. The runtime enforces type compatibility:

```python
@dataclass
class CapabilityDef:
    name: str
    category: str              # OBSERVE, TRANSFORM, SYNTHESIZE, VERIFY
    description: str           # human-readable
    input_types: list[str]     # what types this capability accepts
    output_types: list[str]    # what types this capability produces
    args_schema: dict          # JSON Schema for arguments
    timeout_ms: int
    is_backgroundable: bool    # can run in background (BashOutput pattern)
```

The model sees this at initialization:
```
[OBSERVE] query_cpg(pattern: str, scope?: str) → subgraphs, dataflow_paths
[OBSERVE] fuzz_target(target: str, harness?: str) → crashes, coverage_maps
[TRANSFORM] delta_debug(input: bytes) → minimal_reproducer
[SYNTHESIZE] solve_reachability(target: str, conds: list) → concrete_inputs
[VERIFY] predict_fix_impact(fix: FixProposal) → caller_impacts
...
```

And reasons: "I need evidence about the taint paths → OBSERVE category. Among OBSERVE capabilities, `query_cpg` gives me dataflow_paths which is what I need."

---

## 5.0 The Waypoint System

### 5.1 Design Philosophy

Waypoints are the mechanism that helps weak models without constraining strong ones. They are:
- **Computed pre-run** and **regenerated after each ICO enrichment**
- **Phrased as questions**, not instructions
- **Explicitly skippable**, with an invitation to do something better
- **Pre-loaded with supporting data**, so models don't fetch separately

### 5.2 Waypoint Schema

```python
@dataclass
class Waypoint:
    id: str
    status: str               # OPEN, INVESTIGATED, SKIPPED, IRRELEVANT
    question: str              # phrased as a question, not a command
    supporting_data: list[Fact] # pre-fetched data to answer the question
    suggested_capability: str  # optional, not required
    skip_condition: str        # "skip if you have a different approach"
    priority_score: float      # 0.0-1.0, from domain priors
    depends_on: list[str]      # waypoint IDs this one depends on
    generated_from: str        # what triggered this waypoint (CPG, CVE, pattern, model)
    generated_at_step: int     # ICO step when created (0 = pre-computation)
```

### 5.3 Waypoint Generation

Waypoints are generated from static analysis at T=0, then **regenerated** after every ICO enrichment. The regeneration engine runs deterministically (no LLM call):

```python
class WaypointEngine:
    def generate(self, ico: ICO) -> list[Waypoint]:
        waypoints = []
        
        # Source 1: CPG pattern matches
        for hit in ico.pattern_matches:
            waypoints.append(self.pattern_to_waypoint(hit, ico))
        
        # Source 2: CVE similarity
        for cve in ico.similarity_hits:
            waypoints.append(self.cve_to_waypoint(cve, ico))
        
        # Source 3: Open hypotheses from ISG
        for hypothesis in ico.isg.open_hypotheses():
            waypoints.append(self.hypothesis_to_waypoint(hypothesis, ico))
        
        # Source 4: Confirmed findings that need deeper analysis
        for finding in ico.isg.confirmed_findings():
            waypoints.extend(self.finding_to_waypoints(finding, ico))
        
        return sorted(waypoints, key=lambda w: w.priority_score, reverse=True)
    
    def regenerate(self, ico: ICO) -> list[Waypoint]:
        """Called after every ICO enrichment. Merges old + new waypoints."""
        existing = {w.id: w for w in ico.waypoints}
        new_waypoints = self.generate(ico)
        
        merged = []
        for w in new_waypoints:
            if w.id in existing and existing[w.id].status != WaypointStatus.OPEN:
                w.status = existing[w.id].status  # preserve INVESTIGATED/SKIPPED
            merged.append(w)
        
        return merged
```

---

## 6.0 The Universal Prompt Architecture

### 6.1 Prompt Structure

Every prompt has the same structure. Every model receives the same prompt. What differs is which layers the model actually uses:

```
╔═══════════════════════════════════════════════════════════════════╗
║  INVESTIGATION CONTEXT                                             ║
║  Session: {session_id}  │  Step: {n}/{max}  │  Target: {name}     ║
╠═══════════════════════════════════════════════════════════════════╣
║                                                                   ║
║  ━━━ LAYER 0 — FACTS ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ║
║  (what is known, zero interpretation)                             ║
║                                                                   ║
║  Target: {language}, {framework}, {lines} lines, {functions} fn   ║
║  Entry points: {entry_points}                                      ║
║  Memory model: {memory_model}                                     ║
║  CPG: {nodes} nodes, {edges} edges [reference: {cpg_ref}]         ║
║                                                                   ║
║  Prior tool outputs (this session):                               ║
║  {for each output: summary, relevance — only top 3 shown}        ║
║  {older outputs summarized in compressed section}                  ║
║                                                                   ║
╠═══════════════════════════════════════════════════════════════════╣
║                                                                   ║
║  ━━━ LAYER 1 — PATTERNS ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ║
║  (domain knowledge — read as data, not instructions)              ║
║                                                                   ║
║  Pre-run CWE signatures against CPG:                              ║
║  {for each CWE match: type, count, locations, severity}           ║
║                                                                   ║
║  CVE similarity (cosine, profile-based):                          ║
║  {for each CVE match: id, score, shared_pattern}                  ║
║                                                                   ║
║  Historical: {language} codebases of this profile typically show  ║
║  {top_vuln_classes} as the dominant vulnerability classes.        ║
║                                                                   ║
╠═══════════════════════════════════════════════════════════════════╣
║                                                                   ║
║  ━━━ LAYER 2 — WAYPOINTS ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ║
║  (optional — skip freely, add your own)                           ║
║                                                                   ║
║  {for each waypoint, ordered by priority}:                        ║
║    [{status}] [{priority}] {question}                             ║
║    Supporting data: {pre-fetched facts}                           ║
║    Suggested: {capability} (optional)                              ║
║    Skip if: {condition}                                           ║
║                                                                   ║
║  These waypoints are not exhaustive. They are not required.       ║
║  If you see a better path, take it. You can add waypoints too.    ║
║                                                                   ║
╠═══════════════════════════════════════════════════════════════════╣
║                                                                   ║
║  ━━━ CAPABILITY GRAMMAR ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ║
║                                                                   ║
║  [OBSERVE]  produce new evidence from target                      ║
║    {list of OBSERVE capabilities with input/output types}         ║
║                                                                   ║
║  [TRANSFORM] reduce or isolate existing evidence                  ║
║    {list of TRANSFORM capabilities with input/output types}       ║
║                                                                   ║
║  [SYNTHESIZE] create higher-order artifacts from evidence         ║
║    {list of SYNTHESIZE capabilities with input/output types}      ║
║                                                                   ║
║  [VERIFY] check a hypothesis against ground truth                 ║
║    {list of VERIFY capabilities with input/output types}          ║
║                                                                   ║
║  Outputs of any capability can be inputs to any other.            ║
║                                                                   ║
╠═══════════════════════════════════════════════════════════════════╣
║                                                                   ║
║  ━━━ OPEN DECISION ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ║
║                                                                   ║
║  What is your next investigation step?                            ║
║                                                                   ║
║  You may:                                                         ║
║  • Call a capability: <category>.<name>(<args>)                   ║
║  • Form a hypothesis: HYPOTHESIS: <text>                          ║
║  • Ask a question: QUESTION: <text>                               ║
║  • Skip waypoints and pursue your own direction                   ║
║  • Propose a new waypoint: WAYPOINT: <text>                       ║
║  • Conclude the investigation: CONCLUDE: <finding>                ║
║                                                                   ║
╚═══════════════════════════════════════════════════════════════════╝
```

---

## 7.0 Intent Parsing

### 7.1 The Three-Layer Parser

```
┌─────────────────────────────────────────────────────────────┐
│                    INTENT PARSER                             │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ LAYER A: STRUCTURED EXTRACTION                        │   │
│  │ Parse explicit capability call syntax first:          │   │
│  │   <category>.<name>(<args>)                           │   │
│  │   HYPOTHESIS: <text>                                  │   │
│  │   QUESTION: <text>                                    │   │
│  │   CONCLUDE: <finding>                                  │   │
│  │ If found: high-confidence intent, skip Layer B        │   │
│  └──────────────────────┬───────────────────────────────┘   │
│                         │ (no structured match)             │
│                         ▼                                    │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ LAYER B: SEMANTIC CLASSIFICATION                      │   │
│  │ Classify free text into intent types using:           │   │
│  │   - Pattern matching (keywords: "run", "execute")     │   │
│  │   - Small fast model if available                     │   │
│  │   - Rule-based fallback                               │   │
│  │                                                       │   │
│  │ Intent types: TOOL_CALL, NOVEL_HYPOTHESIS,            │   │
│  │ QUESTION, CONCLUSION, REASONING, STUCK                │   │
│  └──────────────────────┬───────────────────────────────┘   │
│                         │ (confidence < threshold)           │
│                         ▼                                    │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ LAYER C: CONSTRAINED FALLBACK                         │   │
│  │ On repeated STUCK: force structured output format     │   │
│  │ "Please state exactly one of:                         │   │
│  │  [CALL: category.name(args)]                          │   │
│  │  [HYPOTHESIS: <text>]                                  │   │
│  │  [CONCLUDE: <finding>]                                 │   │
│  │  [QUESTION: <text>]"                                   │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 7.2 Intent Types

```python
class IntentType(Enum):
    TOOL_CALL = "tool_call"              # model calls a capability
    PARALLEL_CALLS = "parallel_calls"    # model requests simultaneous tool calls
    NOVEL_HYPOTHESIS = "novel_hypothesis" # model proposes a new hypothesis
    QUESTION = "question"                # model asks for more information
    CONCLUSION = "conclusion"            # model declares investigation complete
    REASONING = "reasoning"              # model is thinking (no action needed)
    STUCK = "stuck"                      # model produced no useful intent
    UNCLEAR = "unclear"                  # output cannot be classified
```

---

## 8.0 The Dispatch & Runtime Loop

### 8.1 Main Loop

```python
class ARIARuntime:
    def __init__(self, tools: ToolRegistry, pre_computer: PreComputationEngine):
        self.tools = tools
        self.pre_computer = pre_computer
        self.intent_parser = UniversalIntentParser()
        self.prompt_builder = LayeredPromptBuilder()
        self.context_manager = ContextManager()
        self.isg = InvestigationStateGraph()
    
    def investigate(self, target: Target, model: ModelInterface) -> InvestigationReport:
        # Phase 0: Pre-computation (no model involved)
        ico = self.pre_computer.analyze(target)
        
        steps = 0
        while not self.is_complete(ico):
            steps += 1
            
            # Phase 1: Build prompt (identical for all models)
            prompt = self.prompt_builder.build(ico)
            
            # Phase 2: Model call (model decides what to do)
            raw_output = model.complete(prompt)
            ico.reasoning_trace.append(raw_output)
            
            # Phase 3: Parse intent
            intent = self.intent_parser.parse(raw_output)
            
            # Phase 4: Dispatch
            ico = self.dispatch(intent, ico)
        
        return self.isg.compile_report(ico)
    
    def dispatch(self, intent: Intent, ico: ICO) -> ICO:
        match intent.type:
            
            case IntentType.TOOL_CALL:
                result = self.tools.execute(intent.capability, intent.args)
                return self.enrich_ico(ico, self.normalize_output(result))
            
            case IntentType.PARALLEL_CALLS:
                results = self.tools.execute_parallel(intent.calls)
                for r in results:
                    ico = self.enrich_ico(ico, self.normalize_output(r))
                return ico
            
            case IntentType.NOVEL_HYPOTHESIS:
                self.isg.add_hypothesis(intent.hypothesis)
                related = self.pre_computer.knowledge_base.fetch_related(intent.hypothesis)
                return self.inject_related(ico, related)
            
            case IntentType.QUESTION:
                answer = self.pre_computer.answer(intent.question, ico)
                return self.inject_answer(ico, answer)
            
            case IntentType.REASONING:
                # Model is thinking — give it another turn
                if len(ico.reasoning_trace) > self.MAX_REASONING_STEPS:
                    waypoint = self.waypoint_engine.next_waypoint(ico)
                    return self.inject_waypoint(ico, waypoint)
                return ico  # unchanged — model gets another cycle
            
            case IntentType.STUCK | IntentType.UNCLEAR:
                # Model didn't produce useful output — inject guidance
                if ico.consecutive_stuck > self.MAX_STUCK_CYCLES:
                    # Force constrained prompt format
                    ico.use_constrained_format = True
                waypoint = self.waypoint_engine.next_waypoint(ico)
                return self.inject_waypoint(ico, waypoint)
            
            case IntentType.CONCLUSION:
                verified = self.verify_finding(intent.finding, ico)
                self.isg.record_finding(verified)
                return self.advance_ico(ico)
            
            case _:
                return ico
    
    def enrich_ico(self, ico: ICO, output: NormalizedToolOutput) -> ICO:
        # Add output to Layer 0
        ico.tool_outputs.append(output)
        self.isg.integrate(output)
        
        # Regenerate waypoints
        ico.waypoints = self.waypoint_engine.regenerate(ico)
        
        # Compress context
        ico = self.context_manager.compress(ico)
        
        return ico
```

### 8.2 Completion Conditions

```python
def is_complete(self, ico: ICO) -> bool:
    conditions = {
        "all_high_priority_waypoints_resolved": all(
            w.status != WaypointStatus.OPEN 
            for w in ico.waypoints 
            if w.priority_score > 0.8
        ),
        "no_open_high_confidence_hypotheses": all(
            h.confidence < 0.5 or h.status != HypothesisStatus.OPEN
            for h in self.isg.hypotheses
        ),
        "diminishing_returns": self.isg.finding_rate(last_n=3) < 0.1,
        "budget_exhausted": ico.step_count >= ico.max_steps,
        "model_declared_complete": ico.model_declared_complete,
    }
    
    return (
        conditions["model_declared_complete"] or
        (conditions["all_high_priority_waypoints_resolved"] and
         conditions["no_open_high_confidence_hypotheses"]) or
        (conditions["diminishing_returns"] and conditions["budget_exhausted"])
    )
```

---

## 9.0 Context Management

Prevents unbounded prompt growth across investigation steps.

### 9.1 Three-Tier Memory

```
┌─────────────────────────────────────────────────────────────┐
│  ACTIVE CONTEXT (always in prompt)                          │
│                                                              │
│  • Current step findings (last 2 tool outputs)              │
│  • 3 most relevant prior findings (cosine to current focus) │
│  • All open high-priority waypoints                         │
│  • Current hypothesis tree (root + immediate children)      │
│  • Capability grammar                                        │
│                                                              │
│  Size: ≤ 8,000 tokens                                        │
├─────────────────────────────────────────────────────────────┤
│  COMPRESSED CONTEXT (summarized in prompt)                  │
│                                                              │
│  • "Prior steps: [summary of completed investigations]"     │
│  • 2-3 sentence summaries of completed waypoints            │
│  • Finding summaries (confirmed, severity, location)        │
│                                                              │
│  Size: ≤ 2,000 tokens                                        │
├─────────────────────────────────────────────────────────────┤
│  ARCHIVE (retrievable on request, not in prompt)             │
│                                                              │
│  • Full raw tool outputs from all prior steps               │
│  • Completed waypoints with supporting data                 │
│  • Rejected hypotheses with reasons                         │
│  • Full reasoning trace                                      │
│                                                              │
│  Retrieved via: QUESTION intent from model                   │
└─────────────────────────────────────────────────────────────┘
```

### 9.2 Relevance Scoring

```python
class ContextManager:
    def get_active_context(self, ico: ICO) -> list[NormalizedToolOutput]:
        current_focus = self.detect_current_focus(ico)
        # current_focus derived from: most recent tool call args,
        # open waypoints, and ISG active path
        
        scored = [
            (output, self.cosine_similarity(output, current_focus))
            for output in ico.tool_outputs
        ]
        scored.sort(key=lambda x: x[1], reverse=True)
        
        # 2 most recent + top 3 by relevance (dedup)
        active = ico.tool_outputs[-2:]
        for output, score in scored:
            if output not in active and score > 0.4:
                active.append(output)
            if len(active) >= 5:
                break
        
        return active
```

---

## 10.0 Integration Points

### 10.1 Replaces: `bugswarm-agent/src/agent/loop.py`

The existing 354-line IEP Engine is replaced by the ARIA runtime. The new file structure:

```
bugswarm-agent/src/agent/
├── aria/
│   ├── __init__.py
│   ├── runtime.py           # ARIARuntime — main loop
│   ├── ico.py               # InvestigationContextObject + related types
│   ├── pre_computer.py      # PreComputationEngine
│   ├── prompt_builder.py    # LayeredPromptBuilder
│   ├── intent_parser.py     # UniversalIntentParser (3-layer)
│   ├── waypoint_engine.py   # WaypointEngine (generate + regenerate)
│   ├── capability_grammar.py # CapabilityGrammar + CapabilityDef
│   ├── context_manager.py   # ContextManager (3-tier memory)
│   ├── dispatch.py          # Intent dispatch logic
│   └── completion.py        # Completion condition logic
├── tools.py                 # ToolRegistry (unchanged, tools register into grammar)
├── loop.py                  # DEPRECATED — kept for backward compat
└── ...
```

### 10.2 Integration with Gateway

```
Agent (ARIA Runtime)
    │
    ▼
Prompt Builder → builds ICO-based prompt
    │
    ▼
Gateway (bugswarm-gateway)
    │ adapter: DeepSeek, Anthropic, OpenAI, etc.
    ▼
LLM (any model)
    │
    ▼
Intent Parser ← parses model output
    │
    ▼
Dispatch → ToolRegistry.execute()
```

### 10.3 Configuration

```yaml
# In /etc/bugswarm/config.yaml
aria:
  max_steps: 50
  max_reasoning_steps: 5
  max_stuck_cycles: 3
  diminishing_returns_threshold: 0.1
  active_context_max_tokens: 8000
  compressed_context_max_tokens: 2000
  waypoint_max_count: 10
  relevance_threshold: 0.4
  enable_constrained_fallback: true
```

### 10.4 Model Independence

The ARIA runtime never queries model identity. It processes intent types. Any model that can produce structured output or free text works. The only per-model configuration is in the Gateway (adapter selection), not in ARIA.

---

## 11.0 Pre-Computation Engine

### 11.1 What Runs Pre-Model

```
PreComputationEngine.analyze(target) → ICO

1. CPG INDEXING (~2-5 seconds)
   parse target → AST → CFG → call graph → taint paths → danger map
   
2. PATTERN PRE-RUN (~1-2 seconds)
   for each registered pattern signature:
       run against CPG → collect matches
   registered patterns: CWE-190, CWE-125, CWE-416, CWE-122, CWE-787,
                        CWE-89, CWE-79, CWE-20, CWE-200, CWE-287
   
3. CVE SIMILARITY (~1 second)
   classify target profile → search CVE DB → cosine similarity → top 20
   
4. PROFILE CLASSIFICATION (~0.5 seconds)
   language + framework + memory_model + size → vulnerability class priors
   
5. WAYPOINT GENERATION (~0.5 seconds)
   for each pattern hit + CVE match + profile prior:
       generate waypoint with pre-fetched supporting data
       
6. VECTOR INDEX BUILD (~1 second)
   embed all static facts → build persistent queryable index
   for runtime fetch_related() calls on novel hypotheses
   
Total pre-computation: ~6-10 seconds
```

### 11.2 Persistent Knowledge Base

The knowledge base persists across investigation steps and across sessions:

```python
class PreComputedKnowledgeBase:
    def __init__(self):
        self.cpg = None                    # Code Property Graph (queryable)
        self.vector_index = None            # FAISS/annoy index over all facts
        self.cve_db = None                  # CVE database (pre-loaded)
        self.pattern_signatures = []        # registered SAST patterns
        self.profile_priors = {}            # language→vulnerability distribution
    
    def fetch_related(self, hypothesis: str) -> list[Fact]:
        """Called when model proposes a novel hypothesis."""
        h_embedding = self.embed(hypothesis)
        related = self.vector_index.search(h_embedding, k=10)
        cpg_results = self.cpg.semantic_query(hypothesis)
        return deduplicate(related + cpg_results)
```

---

## Appendix A: Investigation State Graph (ISG)

The ISG tracks the evolving understanding of the investigation. It is a directed graph where:

- **Nodes** = knowledge states (hypotheses, confirmed findings, open questions)
- **Edges** = reasoning steps (supported_by, contradicted_by, refined_by, chained_from)

```python
class InvestigationStateGraph:
    def __init__(self):
        self.nodes: dict[str, ISGNode] = {}
        self.edges: list[ISGEdge] = []
        self.confirmed_findings: list[Finding] = []
        self.open_hypotheses: list[Hypothesis] = []
    
    def add_hypothesis(self, hypothesis: Hypothesis):
        node = ISGNode(id=hypothesis.id, type=NodeType.HYPOTHESIS, data=hypothesis)
        self.nodes[node.id] = node
        self.open_hypotheses.append(hypothesis)
    
    def integrate(self, tool_output: NormalizedToolOutput):
        """Integrate tool output into the graph — link to relevant hypotheses."""
        for hyp in self.open_hypotheses:
            if tool_output.confidence > 0.7:
                edge = ISGEdge(source=hyp.id, target=tool_output.tool_name,
                               type=EdgeType.SUPPORTED_BY)
                self.edges.append(edge)
    
    def compile_report(self) -> InvestigationReport:
        return InvestigationReport(
            findings=self.confirmed_findings,
            hypotheses=self.open_hypotheses,
            steps=len(self.edges),
            graph=self
        )
```

## Appendix B: Tool Output Normalizers

Every tool must implement a normalizer. Examples:

```python
class CPGQueryNormalizer:
    def normalize(self, raw: dict) -> NormalizedToolOutput:
        return NormalizedToolOutput(
            capability_type="OBSERVE",
            tool_name="query_cpg",
            raw_output=raw,
            code_locations=[CodeLocation(f, l) for f, l in raw.get("taint_paths", [])],
            data_flow_paths=raw.get("dataflow_paths", []),
            constraints=[],
            artifacts=[],
            confidence=raw.get("confidence", 0.0),
            consumable_by=["TRANSFORM", "SYNTHESIZE", "VERIFY"],
            summary=f"CPG query returned {len(raw.get('taint_paths',[]))} taint paths",
            relevance_score=0.0
        )

class FuzzerNormalizer:
    def normalize(self, raw: dict) -> NormalizedToolOutput:
        return NormalizedToolOutput(
            capability_type="OBSERVE",
            tool_name="fuzz_target",
            raw_output=raw,
            code_locations=[CodeLocation(c["file"], c["line"]) for c in raw.get("crashes", [])],
            data_flow_paths=[],
            constraints=[],
            artifacts=[Artifact(type="crash_dump", data=c) for c in raw.get("crashes", [])],
            confidence=0.9 if raw.get("unique_crashes", 0) > 0 else 0.0,
            consumable_by=["TRANSFORM", "SYNTHESIZE"],
            summary=f"Fuzzer: {raw.get('unique_crashes',0)} unique crashes, {raw.get('execs_per_sec',0)} execs/sec",
            relevance_score=0.0
        )
```

---

## Changelog

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0.0 | 2026-05-18 | System | Initial ARIA specification — ICO, 4-category capability grammar, 3-layer intent parser, waypoint engine with regeneration, 3-tier context manager, completion conditions, pre-computation engine with persistent knowledge base |
