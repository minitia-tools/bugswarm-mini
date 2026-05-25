# BugSwarm Enterprise Bug Coverage Expansion Plan

## Document Metadata
- **Version:** 1.0.0
- **Status:** Proposed (Pending Architecture Review)
- **Authors:** BugSwarm Engineering
- **Target Release:** v2.0.0-enterprise
- **Estimated Engineering Effort:** 12 weeks, 4 senior engineers

---

## 1. Executive Summary

BugSwarm currently detects 6 bug categories: taint analysis, memory safety violations,
crash reproduction, silent wrong answers, untested code paths, and branch-level
triggers. While this covers the most common vulnerability vectors in systems-level
software, it leaves significant gaps in web-application security, cryptographic
weaknesses, dependency supply-chain attacks, cloud-infrastructure misconfigurations,
information disclosure, race conditions, and dozens of CWE categories recognized
by MITRE and NIST.

This plan expands BugSwarm's coverage from 6 categories to **104 CWE-mapped
detection layers**, organized into 12 domain-specific scanner modules. Each module
implements a `BugCategoryScanner` trait and plugs into the existing CPG (Code
Property Graph), sandbox execution environment, and agent pipeline. The expansion
is designed to be incremental: each scanner module can be developed, tested, and
deployed independently without blocking other modules.

### Key Metrics Targets

| Metric | Current | Target |
|--------|---------|--------|
| CWE Categories Covered | 6 | 104+ |
| False Positive Rate (FPR) | ~12% (varies by category) | <5% per category |
| Time-to-Detect (TTD) | ~45s median | <30s median |
| Scan Completeness | ~40% of known CWE top 25 | 100% of CWE top 25 + 79 additional |
| Scanner Modules | 1 (monolith) | 12 (pluggable) |
| Supported Languages | 3 (C, Rust, Go) | 8 (adds Python, JS/TS, Java, Ruby, PHP) |
| Proof Generation Rate | 60% of findings | 85% of findings |

---

## 2. Complete Bug Taxonomy: CWE-to-Detection Mapping

### 2.1 Taxonomy Design Principles

BugSwarm's taxonomy is organized as a layered hierarchy that maps raw artifacts
through representation, pattern matching, detection, and proof generation:

```
Layer 0: Raw Artifacts (source code, binaries, configs, dependencies, containers)
Layer 1: CPG Representation (AST, CFG, DFG, PDG, Call Graph, Type Graph)
Layer 2: Vulnerability Patterns (CWE-anchored declarative rules)
Layer 3: Detection Engines (SAST, DAST, SCA, IAST, Fuzzing, Symbolic Execution)
Layer 4: Proof Generation (sandbox repro, exploit chain, patch diff, stack trace)
Layer 5: Triage & Reporting (prioritization, deduplication, false-positive suppression)
Layer 6: Remediation Guidance (auto-patch suggestions, CWE education, compliance mapping)
```

Every CWE category is mapped to:
1. The detection layer(s) responsible for identification
2. The detection method (static analysis, dynamic testing, hybrid approach)
3. The proof type generated (reproducible crash, exploit chain, information leak demo)
4. Target false positive rate (FPR) with measurement methodology
5. Target time-to-detect (TTD) measured from scan initiation to finding emission

### 2.2 Core CWE Coverage: The Top 125 Categories

Below is the comprehensive mapping of 125 CWE categories spanning the complete
CWE Top 25 plus an additional 100 industry-recognized vulnerability classes.

---

#### 2.2.1 Memory Safety Vulnerabilities (CWE-119 Family) — Expanded from Existing

The existing taint + memory + crash detection pipeline covers CWE-119 family
vulnerabilities. We expand this to cover all sub-variants.

| CWE ID | Name | Detection Method | Proof Type | FPR Target | TTD Target |
|--------|------|-----------------|------------|------------|------------|
| CWE-119 | Improper Restriction of Operations within Bounds of Memory Buffer | ASAN + CPG taint analysis | Crash repro with stack trace and input causing overflow | <3% | <10s |
| CWE-120 | Buffer Copy without Checking Size of Input | CPG buffer-overflow pattern matching on memcpy/strcpy sites | Input string triggering overflow with size calculation proof | <3% | <10s |
| CWE-121 | Stack-based Buffer Overflow | ASAN + stack canary analysis + CPG local-buffer analysis | Corrupted stack trace showing canary overwrite | <2% | <8s |
| CWE-122 | Heap-based Buffer Overflow | ASAN + heap metadata check + CPG malloc-site analysis | Heap corruption repro with corrupted chunk header | <2% | <10s |
| CWE-123 | Write-what-where Condition | CPG pointer-range analysis + symbolic execution | Arbitrary write proof showing attacker-controlled address and value | <5% | <15s |
| CWE-124 | Buffer Underwrite | ASAN shadow memory + CPG negative-index analysis | Underwrite repro with pre-buffer corruption | <3% | <10s |
| CWE-125 | Out-of-bounds Read | ASAN + MSAN combined instrumentation | Leaked memory dump showing accessed out-of-bounds region | <3% | <8s |
| CWE-126 | Buffer Over-read | ASAN + CPG loop-bound constraint analysis | Over-read repro with accessed bytes beyond allocation | <3% | <10s |
| CWE-127 | Buffer Under-read | ASAN shadow memory + CPG negative-offset analysis | Under-read repro with pre-buffer access trace | <3% | <10s |
| CWE-129 | Improper Validation of Array Index | CPG symbolic execution on array access sites | Input value triggering out-of-bounds index | <2% | <12s |
| CWE-130 | Improper Handling of Length Parameter Inconsistency | CPG length-parameter correlation analysis across function boundaries | Input pair (buffer, length) demonstrating mismatch | <4% | <15s |
| CWE-131 | Incorrect Calculation of Buffer Size | CPG allocation-site arithmetic analysis | Off-by-N calculation proof with expected vs actual size | <4% | <12s |
| CWE-134 | Uncontrolled Format String | CPG format-string taint: track user-controlled format argument to printf-family | Format-string exploit demonstrating memory read/write | <2% | <8s |
| CWE-170 | Improper Null Termination | CPG null-terminator analysis on string operations | Input lacking null terminator causing read past buffer | <3% | <10s |
| CWE-190 | Integer Overflow or Wraparound | UBSAN integer instrumentation + CPG integer-range analysis | Overflow-triggering input showing wrap-around computation | <3% | <12s |
| CWE-191 | Integer Underflow | UBSAN integer instrumentation + CPG integer-range analysis | Underflow-triggering input with wrap-around proof | <3% | <12s |
| CWE-193 | Off-by-one Error | CPG loop-bound analysis comparing induction variable to allocation size | Off-by-one exploit with boundary input value | <4% | <15s |
| CWE-194 | Unexpected Sign Extension | CPG type-cast analysis: implicit sign extension in expressions | Input demonstrating sign-extended value vs intended value | <3% | <10s |
| CWE-195 | Signed-to-Unsigned Conversion Error | CPG type-cast analysis on signed-to-unsigned implicit conversions | Input producing negative-to-large-unsigned conversion | <3% | <10s |
| CWE-196 | Unsigned-to-Signed Conversion Error | CPG type-cast analysis on unsigned-to-signed implicit conversions | Input producing large-unsigned-to-negative conversion | <3% | <10s |
| CWE-197 | Numeric Truncation Error | CPG truncation analysis: assignment to narrower type | Input demonstrating truncated value vs intended value | <4% | <12s |
| CWE-242 | Use of Inherently Dangerous Function | CPG banned-function list: gets, strcpy, strcat, sprintf, scanf, etc. | Dangerous call location with call chain to untrusted input | <1% | <5s |
| CWE-252 | Unchecked Return Value | CPG return-value-check analysis: ignored return values of security-critical functions | Input demonstrating missed error condition leading to vulnerability | <3% | <8s |
| CWE-337 | Predictable Seed in PRNG | CPG seed-source analysis: time(0), getpid() as srand argument | Predictable-output proof with seed recovery and sequence prediction | <2% | <8s |
| CWE-338 | Use of Cryptographically Weak PRNG | CPG PRNG-function list: rand, random, Math.random, java.util.Random | Weak-randomness proof with statistical analysis of output distribution | <1% | <5s |
| CWE-401 | Missing Release of Memory after Effective Lifetime | CPG alloc-free correlation: malloc without corresponding free on all paths | Memory-leak repro showing monotonic heap growth under load | <4% | <15s |
| CWE-404 | Improper Resource Shutdown or Release | CPG resource-lifecycle analysis: open/close, connect/disconnect pairing | Resource-leak repro with file descriptor/socket exhaustion | <4% | <15s |
| CWE-415 | Double Free | ASAN + CPG free-site reachability analysis | Double-free crash with call stack showing both free sites | <2% | <8s |
| CWE-416 | Use After Free | ASAN + CPG lifetime analysis: pointer use after free-site reachable | UAF crash repro with temporal relationship between free and use | <2% | <10s |
| CWE-476 | NULL Pointer Dereference | CPG null-deref analysis: paths where pointer may be NULL before dereference | Null-deref crash with input that forces NULL assignment path | <2% | <8s |
| CWE-456 | Missing Initialization of a Variable | CPG uninit-var analysis: stack variable read before write | Uninit-read proof with stack dump showing garbage value used | <2% | <8s |
| CWE-457 | Use of Uninitialized Variable | CPG uninit-use analysis: MSAN instrumentation path | Uninit-use crash with MSAN shadow showing uninitialized bit | <2% | <8s |
| CWE-467 | Use of sizeof() on a Pointer Type | CPG sizeof-analysis: sizeof(ptr) where sizeof(*ptr) intended | Sizeof-pointer bug with pointer vs pointee size mismatch proof | <1% | <5s |
| CWE-468 | Incorrect Pointer Scaling | CPG pointer-arithmetic analysis: ptr+1 where ptr+sizeof(*ptr) intended | Incorrect-scaling bug with memory layout diagram | <3% | <8s |
| CWE-480 | Use of Incorrect Operator | CPG operator analysis: & vs &&, | vs ||, = vs == in conditionals | Wrong-operator bug with truth-table demonstrating logic error | <3% | <8s |
| CWE-481 | Assigning Instead of Comparing | CPG operator analysis: assignment = in if/while condition | Assignment-in-condition with input showing unintended control flow | <2% | <5s |
| CWE-483 | Incorrect Block Delimitation | CPG block-delim analysis: mis-indented code after if/for/while without braces | Dangling-else or missing-braces bug with execution trace | <3% | <8s |
| CWE-561 | Dead Code | CPG dead-code analysis: unreachable basic blocks from entry | Dead-code report with unreachability proof from CFG analysis | <1% | <5s |
| CWE-562 | Return of Stack Variable Address | CPG stack-frame analysis: return &local_var | Dangling-stack-ptr with use-after-return crash | <2% | <8s |
| CWE-570 | Expression is Always False | CPG expression-analysis: constant-folded expressions always false | Always-false dead code with unreachable branch identification | <2% | <5s |
| CWE-571 | Expression is Always True | CPG expression-analysis: constant-folded expressions always true | Always-true dead code with always-taken branch identification | <2% | <5s |
| CWE-590 | Free of Memory not on the Heap | CPG free-site analysis: free(&stack_var), free(&global) | Non-heap-free with crash and allocation-type identification | <2% | <8s |
| CWE-628 | Function Call with Incorrectly Specified Arguments | CPG call-analysis: argument count/type mismatch at call sites | Wrong-args bug with expected vs actual signature comparison | <3% | <8s |
| CWE-674 | Uncontrolled Recursion | CPG recursion-analysis: recursive calls without base case or depth limit | Stack-overflow crash with recursion depth and call chain | <4% | <15s |
| CWE-676 | Use of Potentially Dangerous Function | CPG dangerous-func scan: banned API usage with safer alternative suggestion | Dangerous-call report with migration guidance to safe API | <1% | <5s |
| CWE-680 | Integer Overflow to Buffer Overflow | CPG combined integer + buffer analysis: overflow in allocation size computation | Int-to-buf overflow proof with allocation size vs actual needed size | <4% | <15s |
| CWE-690 | Unchecked Return Value to NULL Pointer Dereference | CPG return-null chain: function returning NULL whose result is dereferenced unchecked | Null-deref from return with input that causes the NULL return | <3% | <10s |
| CWE-704 | Incorrect Type Conversion or Cast | CPG type-analysis: unsafe downcasts, C-style casts, reinterpret_cast | Type-conv bug with runtime type information mismatch | <3% | <10s |
| CWE-754 | Improper Check for Unusual or Exceptional Conditions | CPG condition-analysis: missing error checks on external inputs | Unchecked-condition with input demonstrating error path | <4% | <12s |
| CWE-758 | Reliance on Undefined, Unspecified, or Implementation-Defined Behavior | CPG UB-analysis: signed overflow, sequence points, strict aliasing violations | UB-dependence with compiler-specific behavior documentation | <5% | <20s |
| CWE-761 | Free of Pointer not at Start of Buffer | CPG free-offset analysis: free(ptr + offset) where ptr is malloc result | Mid-buffer-free with allocator metadata corruption proof | <2% | <8s |
| CWE-762 | Mismatched Memory Management Routines | CPG allocator-analysis: new/free, malloc/delete, new[]/delete | Mismatched-allocator with undefined behavior explanation | <2% | <5s |
| CWE-787 | Out-of-bounds Write | ASAN + CPG bounds analysis + symbolic execution | OOB-write crash with write target address vs allocation bounds | <2% | <8s |
| CWE-788 | Access of Memory Location After End of Buffer | CPG bounds-analysis: post-allocation access patterns | After-end-access with allocation layout and access offset | <3% | <10s |
| CWE-789 | Uncontrolled Memory Allocation | CPG alloc-size taint: user input controlling malloc argument | Memory-exhaustion with input size vs available memory | <4% | <15s |
| CWE-805 | Buffer Access with Incorrect Length Value | CPG length-analysis: buffer operation with mismatched length argument | Wrong-length access with actual buffer size vs length parameter | <3% | <10s |
| CWE-806 | Buffer Access Using Size of Source Buffer | CPG source-size analysis: sizeof(src) where strlen(src) intended | Wrong-size-access with sizeof vs actual string length comparison | <3% | <10s |
| CWE-822 | Untrusted Pointer Dereference | CPG pointer-taint: pointer value derived from untrusted input | Untrusted-deref with input value controlling pointer target | <4% | <15s |
| CWE-823 | Use of Out-of-Range Pointer Offset | CPG pointer-range analysis: computed offset exceeding allocation | OOR-offset bug with offset calculation and allocation bounds | <3% | <10s |
| CWE-824 | Access of Uninitialized Pointer | CPG init-analysis: pointer variable used before assignment | Uninit-ptr access with execution path showing missing initialization | <3% | <8s |
| CWE-825 | Expired Pointer Dereference | CPG lifetime-analysis: pointer dereference after scope exit or free | Expired-ptr deref with scope/free event and subsequent use | <3% | <10s |
| CWE-835 | Loop with Unreachable Exit Condition (Infinite Loop) | CPG loop-analysis: loop with invariant-true condition | Infinite-loop proof with induction variable analysis | <3% | <10s |
| CWE-843 | Access of Resource Using Incompatible Type (Type Confusion) | CPG type-conflict analysis: reinterpret_cast, union type punning, void* cast | Type-confusion exploit with actual vs expected vtable/memory layout | <5% | <15s |

---

#### 2.2.2 Injection Vulnerabilities (CWE-74, CWE-77, CWE-89 Family) — New

| CWE ID | Name | Detection Method | Proof Type | FPR Target | TTD Target |
|--------|------|-----------------|------------|------------|------------|
| CWE-78 | OS Command Injection | CPG taint + shell-metacharacter injection in exec/system/popen calls | Command-injection proof with injected command output | <3% | <12s |
| CWE-79 | Cross-Site Scripting (XSS) — Reflected | CPG taint: HTTP parameter to HTML response without encoding | XSS proof with alert/document.cookie injection payload | <4% | <12s |
| CWE-79 | Cross-Site Scripting (XSS) — Stored | CPG taint + DB write-to-read flow with HTML rendering sink | Stored XSS with persistent payload in rendered page | <5% | <20s |
| CWE-79 | Cross-Site Scripting (XSS) — DOM-based | CPG taint: URL fragment/document.location to innerHTML/document.write | DOM XSS with client-side payload execution trace | <6% | <25s |
| CWE-89 | SQL Injection | CPG taint: user input to SQL query string concatenation | SQL injection proof with injected UNION SELECT or BOOLEAN blind | <3% | <12s |
| CWE-89 | SQL Injection (Parameterized Query Misuse) | CPG pattern: dynamic table/column/group-by name from user input | SQL injection proof with structure-modifying payload | <4% | <15s |
| CWE-90 | LDAP Injection | CPG taint: user input in LDAP filter string | LDAP injection with injected filter bypassing authentication | <3% | <12s |
| CWE-91 | XSLT Injection | CPG taint: user input in XSLT transform parameter | XSLT injection with arbitrary file read via document() function | <4% | <15s |
| CWE-93 | Email Header Injection (CRLF Injection) | CPG taint: user input in email headers (To, From, Subject) | Email injection with injected BCC/CC headers and SMTP trace | <3% | <10s |
| CWE-94 | Code Injection (Eval Injection) | CPG taint: user input to eval/exec/Function constructor | Code injection proof with injected code execution output | <3% | <12s |
| CWE-95 | Eval Injection (PHP) | CPG taint: user input to eval(), assert(), preg_replace /e | PHP eval injection with injected code output | <3% | <12s |
| CWE-98 | PHP Remote File Inclusion (RFI) | CPG taint: user input to include/require with URL scheme | RFI proof with remote file inclusion and execution trace | <3% | <10s |
| CWE-113 | HTTP Header Injection (CRLF) | CPG taint: user input in HTTP response header value | Header injection with HTTP response splitting proof via raw socket dump | <3% | <10s |
| CWE-117 | Improper Output Neutralization for Logs (Log Injection) | CPG taint: user input in log message without sanitization | Log injection with forged log entries showing CRLF injection | <3% | <10s |
| CWE-434 | Unrestricted Upload of File with Dangerous Type | DAST file upload fuzzing: PHP, JSP, ASPX, ELF payload upload | Malicious file upload with execution proof (shell access, code execution) | <5% | <20s |
| CWE-470 | Use of Externally-Controlled Input to Select Classes or Code (Unsafe Reflection) | CPG reflection analysis: Class.forName(), ReflectionClass with user input | Reflection injection with class-loading arbitrary code proof | <4% | <15s |
| CWE-502 | Deserialization of Untrusted Data | CPG deserialize-analysis + sandbox: ObjectInputStream, pickle, YAML.load | Deserialize RCE proof with gadget chain or arbitrary object creation | <5% | <20s |
| CWE-601 | URL Redirection to Untrusted Site (Open Redirect) | CPG redirect-analysis + DAST redirect fuzzing | Open redirect with phishing URL proof and redirect chain | <3% | <10s |
| CWE-611 | Improper Restriction of XML External Entity Reference (XXE) | CPG XXE analysis + sandbox XML parser with entity expansion | XXE exploit with arbitrary file read via SYSTEM entity | <3% | <10s |
| CWE-643 | XPath Injection | CPG taint: user input in XPath query string | XPath injection with authentication bypass or data extraction proof | <3% | <10s |
| CWE-652 | XQuery Injection | CPG taint: user input in XQuery expression | XQuery injection with arbitrary data access proof | <3% | <10s |
| CWE-776 | XML Entity Expansion (Billion Laughs Attack) | CPG XXE-analysis + sandbox with entity-expansion limit | Billion-laughs DoS with exponential entity expansion | <3% | <10s |
| CWE-917 | Expression Language Injection (EL Injection) | CPG EL-taint: ${...} expression in JSP/JSF/Spring input | EL injection with arbitrary method invocation proof | <4% | <12s |
| CWE-918 | Server-Side Request Forgery (SSRF) | CPG SSRF-taint: user input in HTTP client URL + DAST outbound detection | SSRF exploit with internal service access or cloud metadata exfiltration | <5% | <20s |
| CWE-943 | NoSQL Injection | CPG taint: user input in MongoDB/PouchDB/CouchDB query | NoSQL injection with $ne/$regex operator injection proof | <4% | <15s |
| CWE-1236 | CSV Formula Injection | CPG CSV-analysis: cell starting with =, +, -, @ in exported CSV | CSV injection with DDE command execution or data exfiltration | <3% | <10s |
| CWE-1321 | Prototype Pollution | CPG prototype-analysis + DAST: __proto__ assignment from user input | Proto-pollution with property injection and privilege escalation | <4% | <15s |
| CWE-1336 | Server-Side Template Injection (SSTI) | CPG template-taint: user input in template engine render call | SSTI with arbitrary code execution via template syntax | <4% | <15s |

---

#### 2.2.3 Authentication and Authorization Vulnerabilities (CWE-287 Family) — New

| CWE ID | Name | Detection Method | Proof Type | FPR Target | TTD Target |
|--------|------|-----------------|------------|------------|------------|
| CWE-284 | Improper Access Control | DAST IDOR fuzzing: enumerate object IDs across sessions | IDOR proof with unauthorized data access across user boundaries | <6% | <25s |
| CWE-285 | Improper Authorization | DAST authorization fuzzing: access protected endpoints without roles | Authz bypass with unauthenticated/underprivileged access to admin functions | <5% | <20s |
| CWE-287 | Improper Authentication | DAST auth fuzzing: bypass login, missing auth on endpoints | Authentication bypass with access to protected resources without credentials | <4% | <15s |
| CWE-295 | Improper Certificate Validation | CPG cert-validation analysis: hostname verifier disabled, all-trusting trust manager | MITM proof with self-signed cert accepted by client | <3% | <10s |
| CWE-297 | Improper Validation of Certificate with Host Mismatch | CPG hostname-verifier analysis: custom HostnameVerifier returning true | Hostname mismatch proof with wrong-cert accepted by client | <3% | <10s |
| CWE-306 | Missing Authentication for Critical Function | DAST endpoint enumeration: sensitive operations without auth check | Unauthenticated access to password reset, admin panel, payment endpoint | <4% | <15s |
| CWE-307 | Improper Restriction of Excessive Authentication Attempts | DAST rate-limit fuzzing: brute-force login with rapid requests | Brute-force success proof showing lack of rate limiting or account lockout | <4% | <20s |
| CWE-319 | Cleartext Transmission of Sensitive Information | CPG transport-analysis: HTTP (not HTTPS) for sensitive endpoints | Cleartext transmission proof with packet capture showing plaintext credentials | <2% | <8s |
| CWE-326 | Inadequate Encryption Strength | CPG crypto-analysis + Config: weak key sizes, obsolete algorithms | Weak encryption proof with brute-force feasibility analysis | <3% | <10s |
| CWE-327 | Use of a Broken or Risky Cryptographic Algorithm | CPG crypto-analysis: MD5, SHA1, RC4, DES, 3DES, ECB mode | Cryptographic weakness proof with known attack demonstration | <2% | <8s |
| CWE-328 | Use of Weak Hash (MD5, SHA1) | CPG hash-analysis: MD5, SHA1 for password hashing | Weak hash proof with collision or rainbow table attack demonstration | <2% | <5s |
| CWE-329 | Generation of Predictable IV with CBC Mode | CPG crypto-analysis: static or predictable IV in AES-CBC | Predictable IV proof with chosen-plaintext attack demonstration | <3% | <10s |
| CWE-330 | Use of Insufficiently Random Values | CPG randomness-analysis: time-based, pid-based, weak entropy sources | Weak randomness proof with statistical analysis and seed recovery | <3% | <10s |
| CWE-345 | Insufficient Verification of Data Authenticity | CPG integrity-analysis: missing HMAC, signature, or checksum verification | Integrity bypass with tampered data accepted by application | <4% | <15s |
| CWE-346 | Origin Validation Error | DAST CORS-scan: overly permissive Access-Control-Allow-Origin | CORS bypass with cross-origin request accessing protected resources | <3% | <10s |
| CWE-347 | Improper Verification of Cryptographic Signature | CPG JWT-analysis: alg:none accepted, missing signature verification | JWT bypass with none-algorithm token or self-signed token | <3% | <10s |
| CWE-352 | Cross-Site Request Forgery (CSRF) | DAST CSRF-fuzzing: missing CSRF token, token not tied to session | CSRF proof with cross-origin form submission executing state change | <4% | <15s |
| CWE-359 | Exposure of Private Personal Information to an Unauthorized Actor | CPG info-flow analysis + DAST: PII in responses without auth | PII exposure proof with unauthorized access to personal data | <5% | <20s |
| CWE-362 | Race Condition (TOCTOU) | TSAN + CPG lock analysis: check-then-act pattern on filesystem or shared state | TOCTOU exploit with timing window demonstrated via parallel execution | <8% | <30s |
| CWE-384 | Session Fixation | DAST session-analysis: pre-login session ID accepted post-login | Session fixation with attacker-set session ID gaining authenticated access | <4% | <15s |
| CWE-521 | Weak Password Requirements | Config scanner + CPG: min-length, complexity rules missing | Weak-password-policy report enumerating missing requirements | <2% | <5s |
| CWE-522 | Insufficiently Protected Credentials | CPG credential-flow analysis: credentials stored in plaintext or weakly encrypted | Credential exposure with file system or database access | <3% | <10s |
| CWE-523 | Unprotected Transport of Credentials | CPG transport-analysis: HTTP POST with password, not HTTPS | Cleartext credential with network capture showing plaintext password | <2% | <8s |
| CWE-525 | Sensitive Information in Browser Cache | DAST cache-header inspection: missing Cache-Control: no-store on sensitive pages | Browser cache leak with cached sensitive page content | <5% | <15s |
| CWE-548 | Exposure of Sensitive Information Through Directory Listing | DAST directory-listing: misconfigured web server exposing directory contents | Directory listing with exposed file tree containing sensitive files | <2% | <5s |
| CWE-549 | Missing Password Field Masking | CPG HTML-analysis: input type=text for password field | Unmasked password field on login/registration page | <2% | <5s |
| CWE-613 | Insufficient Session Expiration | DAST session-scan: session token valid after logout or timeout | Session not expired proof with old token still granting access | <4% | <15s |
| CWE-614 | Sensitive Cookie Without Secure Attribute | DAST cookie-scan: Set-Cookie without Secure flag on HTTPS | Cookie sent over HTTP with packet capture demonstrating cleartext transmission | <1% | <5s |
| CWE-620 | Unverified Password Change | DAST password-change fuzzing: change password without current password | Unverified password change with CSRF or missing old-password check | <4% | <15s |
| CWE-639 | Authorization Bypass Through User-Controlled Key (IDOR) | DAST IDOR fuzzing: increment/decrement object IDs across sessions | IDOR proof with unauthorized object access via modified identifier | <5% | <20s |
| CWE-640 | Weak Password Recovery Mechanism | DAST password-reset fuzzing: guessable security questions, token in URL | Password reset bypass with easily guessable answer or token exposure | <5% | <20s |
| CWE-647 | Authorization Bypass via Non-Canonical URL Paths | DAST URL-canonicalization fuzzing: /admin vs /./admin vs /admin%2f | Non-canonical path bypass with encoded/suffixed path reaching protected resource | <4% | <15s |
| CWE-653 | Insufficient Compartmentalization | CPG compartment-analysis: admin and user logic in same component | Compartment violation with user path reaching admin functionality | <6% | <25s |
| CWE-654 | Reliance on a Single Factor in a Security Decision | CPG auth-factor analysis: only password, no 2FA on sensitive operations | Single-factor bypass proof with credential-only access to critical function | <5% | <15s |
| CWE-798 | Use of Hard-coded Credentials | CPG credential-scan: regex patterns for passwords, API keys, tokens in source | Hardcoded credential with extraction of the actual credential value | <1% | <5s |
| CWE-804 | Guessable CAPTCHA | DAST CAPTCHA-analysis: simple math, color check, text recognition bypass | Weak CAPTCHA with automated solver success rate measurement | <6% | <25s |
| CWE-862 | Missing Authorization | DAST auth-fuzzing: access protected resource without required role | Missing auth with unprivileged user performing privileged action | <5% | <20s |
| CWE-863 | Incorrect Authorization | DAST auth-fuzzing: role confusion, horizontal privilege escalation | Incorrect auth with user A accessing user B resources | <5% | <20s |
| CWE-942 | Permissive Cross-domain Policy | DAST CORS-scan: Access-Control-Allow-Origin: * with credentials | CORS misconfiguration with cross-origin credentialed request | <3% | <10s |
| CWE-1004 | Sensitive Cookie Without HttpOnly Flag | DAST cookie-scan: Set-Cookie without HttpOnly | Cookie accessible to JavaScript with document.cookie extraction | <1% | <5s |
| CWE-1151 | Logic Error in Multifactor Authentication | DAST MFA-fuzzing: bypass MFA, replay token, guess backup codes | MFA bypass with missing rate limit, token reuse, or predictable backup | <6% | <30s |
| CWE-1167 | Improper Restriction of Excessive Authentication Attempts | DAST rate-limit fuzzing: login endpoint with no rate limit | Brute-force success with automated password guessing | <4% | <20s |
| CWE-1171 | Missing Authentication for Critical Function | DAST endpoint enumeration: admin/critical endpoints without authentication | Unauthenticated critical function access with state-changing operation proof | <4% | <15s |
| CWE-1184 | Single-Factor Authentication for Critical Systems | CPG auth-analysis: no multi-factor on sensitive operations | Single-factor proof with credential-only access to financial/health data | <5% | <15s |
| CWE-1185 | Authentication Bypass by Alternate Name | DAST auth-fuzzing: username enumeration, email-as-username confusion | Alternate-name bypass with email login vs username login mismatch | <5% | <20s |
| CWE-1200 | Device Unlock Credential Sharing | CPG auth-analysis: shared credentials across users or services | Credential sharing with multiple users using same token/key | <5% | <15s |
| CWE-1212 | Authorization Bypass Through Weakly Protected Credentials | CPG auth-analysis + DAST: weak credential storage enabling token theft | Weak-credential bypass with token extraction and replay | <5% | <20s |
| CWE-1213 | Insufficient Authorization in GraphQL | DAST GraphQL fuzzing: field-level authorization bypass | GraphQL auth bypass with unauthorized field access via introspection | <5% | <20s |
| CWE-1220 | Insufficient Granularity of Access Control | DAST access-fuzzing: coarse role model (admin/user only) | Coarse access with user accessing admin-subset functionality | <5% | <20s |
| CWE-1226 | Missing Authentication for Critical Function Using Shared Resource | CPG auth-analysis + DAST: shared resource without per-function auth | Shared-resource no-auth with cross-function access | <5% | <20s |
| CWE-1249 | Application-Level Admin Object with Insecure Method Availability | DAST admin-fuzzing: admin API methods accessible without admin role | Admin-method access with non-admin user invoking admin functionality | <5% | <15s |
| CWE-1312 | Missing Authentication for Critical Function via Shared Resource | CPG auth-analysis + DAST: shared resource lacking per-operation auth | Shared-resource missing auth with cross-boundary access | <5% | <20s |
| CWE-1422 | Improper Access Control - Authorization Bypass | DAST auth-fuzzing: comprehensive authorization bypass testing | Authz bypass with complete privilege escalation chain | <4% | <15s |

---

#### 2.2.4 Information Disclosure Vulnerabilities (CWE-200 Family) — New

| CWE ID | Name | Detection Method | Proof Type | FPR Target | TTD Target |
|--------|------|-----------------|------------|------------|------------|
| CWE-200 | Exposure of Sensitive Information to an Unauthorized Actor | DAST info-leak scan + CPG error-message analysis | Sensitive data in response body with regex match documentation | <4% | <15s |
| CWE-201 | Insertion of Sensitive Information Into Sent Data | CPG data-flow: sensitive source to network output sink | Sensitive data in network payload with packet capture | <4% | <15s |
| CWE-202 | Exposure of Sensitive Information Through Data Queries | CPG query-analysis: SELECT * on user table returning password hashes | Sensitive column exposure with API response showing excess fields | <3% | <10s |
| CWE-203 | Observable Discrepancy (User Enumeration) | DAST user-enumeration: login timing, registration responses, password reset | User enumeration with response timing/difference analysis | <5% | <20s |
| CWE-204 | Observable Response Discrepancy | DAST response-analysis: different error messages revealing internal state | Information leakage through response content differences | <5% | <20s |
| CWE-205 | Observable Behavioral Discrepancy | DAST behavioral-analysis: different processing paths visible externally | Behavioral leakage with side-channel information exposure | <6% | <30s |
| CWE-207 | Information Exposure Through an External Behavioral Inconsistency | DAST consistency-analysis: different responses to valid/invalid inputs | Behavioral inconsistency revealing internal validation logic | <6% | <25s |
| CWE-208 | Observable Timing Discrepancy (Timing Side-Channel) | DAST timing-analysis: response time differences revealing secrets | Timing side-channel with statistical analysis of response times | <8% | <45s |
| CWE-209 | Generation of Error Message Containing Sensitive Information | DAST error-fuzzing + CPG error-message analysis | Sensitive data in error response: stack trace, SQL error, file path, version | <3% | <10s |
| CWE-210 | Self-generated Error Message Containing Sensitive Information | CPG error-generation analysis: custom error messages with internal details | Custom error with sensitive information (internal IP, username, token) | <3% | <10s |
| CWE-211 | Externally-Generated Error Message Containing Sensitive Information | DAST error-provocation: trigger framework/dependency errors revealing internals | External error with version, path, or configuration disclosure | <3% | <10s |
| CWE-212 | Improper Removal of Sensitive Information Before Storage or Transfer | CPG data-cleanup analysis: sensitive data in serialized objects or caches | Uncleaned sensitive data in storage/transfer with extraction proof | <4% | <15s |
| CWE-213 | Exposure of Sensitive Information Due to Incompatible Policies | Config scanner: policy mismatch between services | Policy-incompatibility with data exposure across trust boundaries | <6% | <30s |
| CWE-214 | Invocation of Process Using Visible Sensitive Information | CPG process-analysis: sensitive data in command-line arguments visible in /proc | Sensitive data in /proc/PID/cmdline or process listing | <3% | <10s |
| CWE-215 | Insertion of Sensitive Information Into Debug Code | CPG debug-code analysis: debug output containing credentials, tokens, PII | Debug output leaking sensitive data with logfile or console evidence | <3% | <10s |
| CWE-311 | Missing Encryption of Sensitive Data | CPG crypto-analysis + Config: sensitive data stored without encryption | Unencrypted sensitive data at rest with filesystem or database access | <3% | <10s |
| CWE-312 | Cleartext Storage of Sensitive Information | CPG storage-analysis: plaintext passwords, tokens, PII in files/DB | Cleartext sensitive data with file/database dump evidence | <2% | <8s |
| CWE-313 | Cleartext Storage in a File or on Disk | CPG file-write analysis: sensitive data written to disk unencrypted | Cleartext file with sensitive data extraction from filesystem | <3% | <10s |
| CWE-314 | Cleartext Storage in the Registry | Windows-specific: CPG registry-write analysis (noted for Windows targets) | Cleartext registry key with regedit export showing sensitive data | <2% | <8s |
| CWE-315 | Cleartext Storage of Sensitive Information in a Cookie | DAST cookie-analysis: sensitive data in cookie value without encryption | Cleartext cookie with base64-decoded sensitive data | <2% | <8s |
| CWE-316 | Cleartext Storage of Sensitive Information in Memory | CPG memory-analysis: sensitive data in memory without secure clearing | Memory dump containing plaintext credentials after use | <5% | <25s |
| CWE-317 | Cleartext Storage of Sensitive Information in GUI | CPG GUI-analysis: sensitive data visible in UI elements without masking | GUI exposure with unredacted sensitive data on screen | <3% | <10s |
| CWE-318 | Cleartext Storage of Sensitive Information in Executable | CPG binary-analysis: strings command on binary revealing embedded secrets | Binary cleartext with strings output showing embedded credentials | <2% | <5s |
| CWE-497 | Exposure of Sensitive System Information to an Unauthorized Control Sphere | CPG info-flow analysis: system-level info to external output | System info leak with /proc, /sys, or config file content in response | <4% | <15s |
| CWE-498 | Cloneable Class Containing Sensitive Information | CPG clone-analysis: Cloneable class with sensitive fields | Clone leak with cloned object exposing sensitive parent data | <3% | <10s |
| CWE-499 | Serializable Class Containing Sensitive Data | CPG serialize-analysis: Serializable class with sensitive fields not transient | Serialize leak with serialized form exposing sensitive fields | <3% | <10s |
| CWE-526 | Exposure of Sensitive Information Through Environment Variables | CPG env-var analysis: sensitive data in environment variables | Env var leak with /proc/PID/environ or debug output showing secrets | <3% | <8s |
| CWE-527 | Exposure of Version-Control Repository to Unauthorized Control Sphere | DAST .git exposure: accessible .git directory on web server | Exposed .git with git-dumper extracting full source code history | <1% | <5s |
| CWE-528 | Exposure of Core Dump File to Unauthorized Control Sphere | DAST core-dump check: accessible core dump files on server | Exposed core dump containing process memory with sensitive data | <2% | <8s |
| CWE-529 | Exposure of Access Control List Files to Unauthorized Control Sphere | DAST ACL exposure: .htaccess, web.config accessible | Exposed ACL file with directory protection rules and credentials | <2% | <5s |
| CWE-530 | Exposure of Backup File to Unauthorized Control Sphere | DAST backup-file scan: .bak, .old, .swp, ~ files accessible | Exposed backup with original file content and possibly outdated secrets | <1% | <5s |
| CWE-531 | Exposure of Sensitive Information Through Test Code | CPG test-code analysis: test files containing real credentials or PII | Test code leak with production credentials in test fixtures | <4% | <10s |
| CWE-532 | Insertion of Sensitive Information into Log File | CPG log-flow analysis: sensitive data in log statements | Log info leak with log output containing PII, tokens, or credentials | <3% | <10s |
| CWE-535 | Information Exposure Through Shell Error Message | DAST error-fuzzing: trigger shell errors revealing paths, commands | Shell error leak with exposed working directory and command | <3% | <10s |
| CWE-536 | Information Exposure Through Servlet Runtime Error Message | DAST error-fuzzing: trigger servlet errors revealing framework internals | Servlet error with framework version, classpath, stack trace | <3% | <10s |
| CWE-537 | Java Runtime Error Message Containing Sensitive Information | CPG Java-exception analysis: uncaught exceptions with sensitive context | Java error with sensitive variable values in exception message | <3% | <10s |
| CWE-538 | Insertion of Sensitive Information into Externally-Accessible File or Directory | CPG file-write analysis: sensitive data in web-accessible files | Externally accessible sensitive file with URL access proof | <4% | <12s |
| CWE-539 | Use of Persistent Cookies Containing Sensitive Information | DAST cookie-analysis: persistent cookies with sensitive data | Persistent cookie with expiration and sensitive data extraction | <3% | <8s |
| CWE-540 | Inclusion of Sensitive Information in Source Code | CPG source-code scan: API keys, tokens, passwords in source files | Source code leak with credential extraction from repository | <2% | <5s |
| CWE-541 | Inclusion of Sensitive Information in Include File | CPG include-analysis: sensitive data in config files, .inc files | Include file leak with web-accessible configuration content | <3% | <8s |
| CWE-542 | Information Exposure Through Cleanup Log Files | CPG cleanup-log analysis: cleanup/rotation logs containing sensitive data | Cleanup log with sensitive data visible in log archive | <4% | <12s |
| CWE-548 | Exposure of Information Through Directory Listing | DAST directory-listing: enabled directory listing on web server | Directory listing showing internal file structure and names | <2% | <5s |
| CWE-591 | Sensitive Data Storage in Improperly Locked Memory | CPG memory-lock analysis: mlock/mlockall usage for sensitive buffers | Unlocked memory containing credentials readable from swap or core dump | <5% | <15s |
| CWE-598 | Use of GET Request Method With Sensitive Query Strings | CPG HTTP-analysis: sensitive data in URL query parameters | GET sensitive data with URL appearing in logs, referrer, history | <2% | <5s |
| CWE-612 | Information Exposure Through Indexing of Private Data | DAST indexing-scan: search engine indexing of private pages | Indexed private data with Google dork showing indexed sensitive content | <4% | <15s |
| CWE-615 | Inclusion of Sensitive Information in Source Code Comments | CPG comment-analysis: regex scan for secrets in code comments | Sensitive comment with TODO containing password, FIXME with API key | <2% | <5s |
| CWE-651 | Exposure of WSDL File Containing Sensitive Information | DAST WSDL-scan: accessible WSDL file with internal endpoint details | WSDL exposure with SOAP service internal structure and endpoints | <2% | <5s |
| CWE-1138 | Insufficient Logging of Exceptions | CPG logging-analysis: catch blocks without logging | Missing exception log with silent error swallowing | <4% | <12s |
| CWE-1143 | Stack Trace on Exception | CPG exception-analysis: stack trace in production error response | Stack trace leak with full file paths, line numbers, framework internals | <2% | <5s |
| CWE-1179 | Insufficiently Protected Credentials via Cache | CPG cache-analysis: credentials in Redis, Memcached, or HTTP cache | Cached credential leak with cache retrieval proof | <4% | <12s |
| CWE-1180 | Incomplete Data Cleanup in a Multi-Tenant Environment | CPG tenant-analysis: data isolation between tenants | Cross-tenant data leak with tenant-A accessing tenant-B data | <6% | <30s |
| CWE-1192 | Information Exposure Through Logging on Disk | CPG log-analysis: sensitive data in disk-persisted log files | Disk log leak with file extraction showing plaintext sensitive data | <3% | <10s |
| CWE-1203 | Exposure of Sensitive Information in Client-Side Code | CPG client-code analysis: secrets in JavaScript, mobile app code | Client-side secret with minified JS extraction or APK decompilation | <3% | <10s |
| CWE-1214 | Insufficiently Protected Credentials via Storage | CPG storage-analysis: credentials in local storage, SQLite, SharedPreferences | Stored credential with device file system access | <4% | <12s |
| CWE-1215 | Insufficiently Protected Credentials via Network | CPG network-analysis: credentials in cleartext network protocols | Network credential leak with packet capture or proxy log | <3% | <10s |
| CWE-1217 | Missing Encryption of Sensitive Data in Logs | CPG log-analysis: sensitive data logged without encryption | Unencrypted log with plaintext PII/credentials in log file | <3% | <10s |
| CWE-1228 | Insufficiently Protected Credentials via Transmission | CPG transport-analysis: credentials in URL, headers, or body without TLS | Transmitted credential with MITM proxy capture | <3% | <10s |
| CWE-1230 | Exposure of Sensitive Information Through Metadata | CPG metadata-analysis: EXIF, PDF metadata, Office doc properties | Metadata leak with author, software version, internal paths in file metadata | <2% | <5s |
| CWE-1295 | Debug Messages Revealing Unnecessary Information | CPG debug-msg analysis: verbose debug output in production | Verbose debug with internal state, secrets, or configuration in debug output | <3% | <8s |
| CWE-1323 | Insufficiently Protected Credentials via Unencrypted Storage | CPG storage-analysis: credentials in plaintext files or databases | Unencrypted stored credentials with database or file system access | <3% | <10s |
| CWE-1324 | Sensitive Information Accessible by Physical Probing of JTAG Interface | CPG JTAG-analysis: JTAG debugging enabled in production firmware | JTAG vulnerability with accessible debug interface | <8% | <35s |
| CWE-1372 | Exposure of Sensitive Information through Sent Data in Embedded Systems | CPG embedded-comm analysis: serial, I2C, SPI data leakage | Embedded data leak with bus monitoring showing sensitive data | <8% | <35s |
| CWE-1423 | Exposure of Sensitive Information through Metadata in Source Code Comments | CPG comment-analysis: expanded regex patterns for metadata-secrets | Comment metadata with internal URLs, IPs, usernames in code comments | <2% | <5s |

---

### 2.3 Taxonomy Implementation Architecture

The taxonomy is implemented as a hierarchical rule database stored in TOML:

```toml
# bugswarm-scanner/taxonomy/cwe_table.toml

[taxonomy]
version = "2.0.0"
total_cwes = 604
scanner_modules = 12

[[cwes]]
id = "CWE-119"
name = "Improper Restriction of Operations within Bounds of Memory Buffer"
parent_ids = ["CWE-118"]
child_ids = ["CWE-120", "CWE-121", "CWE-122", "CWE-123", "CWE-124", "CWE-125", "CWE-126", "CWE-127"]
scanner_module = "sast-memory"
detection_method = "static+dynamic"
fpr_target = 0.03
ttd_target_ms = 10000
proof_types = ["crash_repro", "stack_trace", "input_repro"]
severity_default = "HIGH"
owasp_category = "A03:2021-Injection"
cisq_category = "Reliability"

[[cwes]]
id = "CWE-78"
name = "Improper Neutralization of Special Elements used in an OS Command"
# ... additional 603 entries
```

---

## 3. Pluggable Scanner Architecture

### 3.1 Crate Structure

```
bugswarm-scanner/
├── Cargo.toml
├── src/
│   ├── lib.rs                    -- Crate root, re-exports
│   ├── trait.rs                  -- BugCategoryScanner trait definition
│   ├── context.rs                -- ScannerContext struct
│   ├── registry.rs               -- ScannerRegistry for dynamic module loading
│   ├── orchestrator.rs           -- ScanOrchestrator for parallel execution
│   ├── finding.rs                -- Finding, Proof, Severity types
│   ├── taxonomy/
│   │   ├── mod.rs
│   │   ├── cwe.rs                -- CweId, CweSeverity, CweCategory
│   │   ├── mapping.rs            -- CWE-to-scanner mapping
│   │   └── cwe_table.toml        -- Static CWE taxonomy data
│   ├── scanners/
│   │   ├── mod.rs
│   │   ├── sast_taint.rs         -- SAST Taint Analysis (existing, refactored)
│   │   ├── sast_memory.rs        -- Memory Safety Scanner
│   │   ├── sast_codeql_semantic.rs -- CodeQL-like Semantic Queries
│   │   ├── sast_crypto.rs        -- Cryptographic Weakness Detection
│   │   ├── sast_info_disclosure.rs -- Information Disclosure Scanner
│   │   ├── sast_race.rs          -- Race Condition Detection
│   │   ├── dast_http.rs          -- HTTP Endpoint Probing
│   │   ├── dast_api_fuzz.rs      -- API Schema Fuzzing
│   │   ├── dast_auth_bypass.rs   -- Authentication Bypass Testing
│   │   ├── config_docker.rs      -- Dockerfile Security Scanner
│   │   ├── config_k8s.rs         -- Kubernetes Manifest Scanner
│   │   ├── config_iam.rs         -- Cloud IAM Policy Reviewer
│   │   └── sca_dep.rs            -- Dependency Vulnerability Analyzer
│   ├── queries/
│   │   ├── mod.rs
│   │   ├── bsql_parser.rs        -- BSQL query language parser
│   │   ├── bsql_compiler.rs      -- BSQL to CPG query compiler
│   │   ├── bsql_runtime.rs       -- BSQL query execution engine
│   │   └── rules/                -- BSQL rule files
│   │       ├── injection.bsql
│   │       ├── auth.bsql
│   │       ├── crypto.bsql
│   │       ├── info_disclosure.bsql
│   │       ├── resource.bsql
│   │       ├── input_validation.bsql
│   │       ├── concurrency.bsql
│   │       └── mobile.bsql
│   ├── dast/
│   │   ├── mod.rs
│   │   ├── http_prober.rs        -- HTTP endpoint discovery and probing
│   │   ├── api_fuzzer.rs         -- OpenAPI/GraphQL schema-driven fuzzer
│   │   ├── auth_tester.rs        -- Authentication/authorization test flows
│   │   ├── session_tester.rs     -- Session management testing
│   │   ├── tls_scanner.rs        -- TLS/SSL configuration scanner
│   │   ├── cors_tester.rs        -- CORS misconfiguration tester
│   │   └── request_smuggler.rs   -- HTTP request smuggling detector
│   ├── config/
│   │   ├── mod.rs
│   │   ├── dockerfile_parser.rs  -- Dockerfile AST parser
│   │   ├── k8s_parser.rs         -- K8s manifest YAML parser
│   │   ├── iam_policy.rs         -- Cloud IAM policy analyzer
│   │   └── rules/                -- Configuration security rules
│   │       ├── docker_rules.toml
│   │       ├── k8s_rules.toml
│   │       └── iam_rules.toml
│   └── sca/
│       ├── mod.rs
│       ├── sbom.rs                -- SBOM generation (CycloneDX/SPDX)
│       ├── advisory_db.rs         -- Vulnerability advisory database client
│       ├── cargo_audit.rs         -- Rust: cargo-audit integration
│       ├── npm_audit.rs           -- Node: npm-audit integration
│       ├── pip_audit.rs           -- Python: pip-audit integration
│       ├── maven_audit.rs         -- Java: OWASP Dependency-Check integration
│       └── gem_audit.rs           -- Ruby: bundler-audit integration
```

### 3.2 The BugCategoryScanner Trait

```rust
/// The unified trait that all BugSwarm scanner modules must implement.
/// This trait enables pluggable scanning with consistent interfaces across
/// static analysis, dynamic testing, configuration scanning, and dependency
/// vulnerability analysis.

use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;
use std::sync::Arc;

#[async_trait]
pub trait BugCategoryScanner: Send + Sync + std::fmt::Debug {
    /// Unique scanner identifier used for registration and telemetry.
    /// Example: "sast-taint", "dast-http", "config-k8s"
    fn scanner_id(&self) -> &'static str;

    /// Human-readable display name for reporting.
    fn scanner_name(&self) -> &'static str;

    /// Scanner version for compatibility checking.
    fn scanner_version(&self) -> semver::Version;

    /// The CWE categories this scanner is capable of detecting.
    /// May return a dynamic set based on configuration (e.g., language-specific variants).
    fn covered_cwes(&self) -> Vec<CweId>;

    /// Return the scanner's current immutable configuration.
    fn configuration(&self) -> &ScannerConfig;

    /// Minimum required CPG features for this scanner to operate.
    /// Returns CPG features that must be present; scan fails if prerequisites not met.
    fn required_cpg_features(&self) -> Vec<CpgFeature>;

    /// Initialize the scanner with the BugSwarm runtime context.
    /// This is called once before any scan operations begin.
    /// The scanner should validate connectivity to all required services.
    async fn initialize(&mut self, ctx: Arc<ScannerContext>) -> Result<(), ScannerError>;

    /// Validate that all prerequisites are met for scanning the given artifact.
    /// Returns a detailed report that must pass before scan() is called.
    async fn validate_prerequisites(
        &self,
        artifact: &Artifact,
    ) -> Result<PrerequisiteReport, ScannerError>;

    /// Execute the scan against a target artifact.
    /// Returns a stream of Findings for async incremental processing.
    /// The scanner is responsible for implementing its own timeout and cancellation.
    async fn scan(
        &self,
        artifact: &Artifact,
    ) -> Result<Pin<Box<dyn Stream<Item = Finding> + Send + '_>>, ScannerError>;

    /// Generate a proof-of-vulnerability for a specific finding.
    /// May return None if the scanner cannot generate proofs for this finding type.
    /// The sandbox handle provides an isolated environment for safe exploit reproduction.
    async fn generate_proof(
        &self,
        finding: &Finding,
        sandbox: Arc<SandboxHandle>,
    ) -> Result<Option<Proof>, ScannerError>;

    /// Estimate the expected number of findings for progress reporting.
    fn estimate_finding_count(&self, artifact: &Artifact) -> FindingEstimate;

    /// Return scanner health status for monitoring.
    async fn health_check(&self) -> Result<HealthStatus, ScannerError>;

    /// Shut down the scanner gracefully, releasing all resources.
    async fn shutdown(&mut self) -> Result<(), ScannerError>;
}

/// The scanner context provides access to all shared BugSwarm infrastructure.
pub struct ScannerContext {
    /// The Code Property Graph for the target artifact.
    pub cpg: Arc<CodePropertyGraph>,

    /// The sandbox pool for isolated dynamic testing.
    pub sandbox_pool: Arc<SandboxPool>,

    /// The agent runtime for LLM-assisted vulnerability analysis.
    pub agent_runtime: Arc<AgentRuntime>,

    /// The software bill of materials and dependency graph.
    pub dependency_graph: Arc<DependencyGraph>,

    /// The vulnerability advisory database.
    pub advisory_db: Arc<AdvisoryDatabase>,

    /// Distributed configuration store.
    pub config: Arc<ConfigStore>,

    /// Event bus for inter-scanner communication (finding correlation).
    pub event_bus: Arc<EventBus>,

    /// Metrics registry for telemetry.
    pub metrics: Arc<MetricsRegistry>,

    /// Artifact store for accessing build artifacts, binaries, container images.
    pub artifact_store: Arc<ArtifactStore>,

    /// Network sandbox for DAST operations.
    pub network_sandbox: Arc<NetworkSandbox>,
}
```

### 3.3 Scanner Registry and Dynamic Discovery

```rust
/// The ScannerRegistry manages all registered scanner modules.
/// It supports dynamic registration at startup via plugin discovery
/// and provides capability-based scanner selection.

pub struct ScannerRegistry {
    scanners: RwLock<HashMap<String, Box<dyn BugCategoryScanner>>>,
    by_cwe: RwLock<HashMap<CweId, Vec<String>>>,  // CWE -> scanner IDs
    by_feature: RwLock<HashMap<CpgFeature, Vec<String>>>,
}

impl ScannerRegistry {
    /// Register a scanner module at runtime.
    pub async fn register(
        &self,
        scanner: Box<dyn BugCategoryScanner>,
    ) -> Result<(), RegistryError> {
        let id = scanner.scanner_id().to_string();
        let cwes = scanner.covered_cwes();

        // Update CWE index
        {
            let mut by_cwe = self.by_cwe.write().await;
            for cwe in &cwes {
                by_cwe.entry(*cwe).or_default().push(id.clone());
            }
        }

        // Store scanner
        {
            self.scanners.write().await.insert(id, scanner);
        }

        Ok(())
    }

    /// Discover and load scanner plugins from the plugin directory.
    pub async fn discover_plugins(
        &self,
        plugin_dir: &Path,
    ) -> Result<Vec<String>, RegistryError> {
        // Dynamic library loading via libloading
        // Each plugin exports: extern "C" fn create_scanner() -> Box<dyn BugCategoryScanner>
        let mut loaded = Vec::new();
        for entry in std::fs::read_dir(plugin_dir)? {
            let entry = entry?;
            if entry.path().extension() == Some(OsStr::new("so"))
                || entry.path().extension() == Some(OsStr::new("dylib"))
            {
                let scanner = unsafe {
                    let lib = libloading::Library::new(entry.path())?;
                    let constructor: libloading::Symbol<
                        unsafe extern "C" fn() -> Box<dyn BugCategoryScanner>,
                    > = lib.get(b"create_scanner")?;
                    constructor()
                };
                let id = scanner.scanner_id().to_string();
                self.register(scanner).await?;
                loaded.push(id);
            }
        }
        Ok(loaded)
    }

    /// Get all scanners capable of detecting a specific CWE.
    pub async fn scanners_for_cwe(&self, cwe: CweId) -> Vec<&Box<dyn BugCategoryScanner>> {
        let by_cwe = self.by_cwe.read().await;
        let scanner_ids = by_cwe.get(&cwe).cloned().unwrap_or_default();
        let scanners = self.scanners.read().await;
        scanner_ids
            .iter()
            .filter_map(|id| scanners.get(id))
            .collect()
    }
}
```

### 3.4 Parallel Scan Orchestrator

```rust
/// The ScanOrchestrator manages the lifecycle of a multi-scanner scan.
/// It runs all applicable scanners in parallel using Tokio task spawning
/// and merges their finding streams via the EventBus for correlation.

pub struct ScanOrchestrator {
    registry: Arc<ScannerRegistry>,
    context: Arc<ScannerContext>,
    max_concurrent_scanners: usize,
    finding_buffer_size: usize,
}

impl ScanOrchestrator {
    /// Execute a full scan against an artifact using all applicable scanners.
    /// Scanners run in parallel. Findings are merged and correlated.
    pub async fn scan(
        &self,
        artifact: &Artifact,
        scanner_filter: Option<&[String]>,  // Optional subset of scanners
    ) -> Result<ScanReport, OrchestratorError> {
        // 1. Determine which scanners apply to this artifact
        let applicable = self
            .registry
            .scanners_for_artifact(artifact, scanner_filter)
            .await?;

        // 2. Validate prerequisites in parallel
        let prereq_futures: Vec<_> = applicable
            .iter()
            .map(|(id, scanner)| async move {
                let report = scanner.validate_prerequisites(artifact).await;
                (id.clone(), report)
            })
            .collect();

        let prereq_results = futures::future::join_all(prereq_futures).await;

        // Filter to only scanners that passed validation
        let ready_scanners: Vec<_> = applicable
            .into_iter()
            .zip(prereq_results.into_iter())
            .filter(|(_, (_, result))| result.is_ok())
            .map(|((id, scanner), _)| (id, scanner))
            .collect();

        // 3. Launch all scanners concurrently
        let mut handles = Vec::new();
        let finding_tx = self.context.event_bus.finding_sender();

        for (scanner_id, scanner) in ready_scanners {
            let scanner = Arc::new(scanner);
            let artifact = artifact.clone();
            let tx = finding_tx.clone();

            let handle = tokio::spawn(async move {
                let start = Instant::now();
                let mut findings_stream = scanner.scan(&artifact).await?;

                let mut count = 0u64;
                while let Some(finding) = findings_stream.next().await {
                    tx.send(FindingEvent {
                        scanner_id: scanner.scanner_id().to_string(),
                        finding,
                        elapsed: start.elapsed(),
                    })
                    .await?;
                    count += 1;
                }

                Ok::<_, ScannerError>(ScannerScanResult {
                    scanner_id: scanner.scanner_id().to_string(),
                    duration: start.elapsed(),
                    finding_count: count,
                })
            });

            handles.push((scanner_id, handle));
        }

        // 4. Collect results with timeout
        let scan_deadline = tokio::time::sleep(Duration::from_secs(600)); // 10 min max
        let results = tokio::select! {
            results = futures::future::join_all(
                handles.into_iter().map(|(_, h)| h)
            ) => results,
            _ = scan_deadline => {
                // Timeout — cancel remaining scanners
                return Err(OrchestratorError::ScanTimeout);
            }
        };

        // 5. Build final report
        Ok(ScanReport::from_results(results))
    }
}
```

---

## 4. SAST Expansion: CodeQL-Like Semantic Queries

### 4.1 BSQL: BugSwarm Query Language

The `sast-codeql-semantic` module introduces BSQL, a declarative query language
inspired by CodeQL. BSQL compiles to CPG traversal operations and supports:

- **Pattern matching** over AST nodes, data flow edges, and control flow paths
- **Taint tracking** with source/sink/sanitizer declarations
- **Path queries** for finding vulnerability paths through the codebase
- **Aggregation** for counting, grouping, and statistical analysis of findings
- **Cross-language support** via language-specific CPG schema adapters

Example BSQL query for hardcoded credentials:

```bsql
/**
 * @name Hardcoded Credentials
 * @description Finds hardcoded passwords, API keys, and tokens in source code
 * @id cwe-798-hardcoded-credentials
 * @kind problem
 * @severity error
 * @precision high
 * @tags security
 *       cwe-798
 *       external/cwe/cwe-798
 */

import python
import cpg.DataFlow
import cpg.StringAnalysis

predicate isSensitiveVariable(Variable v) {
  v.getName()
      .toLowerCase()
      .regexMatch("(password|secret|api[_-]?key|token|credential|auth|pwd)")
}

predicate isHardcodedString(Expr e) {
  e instanceof StringLiteral and
  e.getStringValue().length() > 0 and
  not e.getStringValue().matches("%%PLACEHOLDER%%")
}

from AssignStmt assign, Variable target, StringLiteral value
where
  assign.getTarget() = target and
  assign.getSource() = value and
  isSensitiveVariable(target) and
  isHardcodedString(value) and
  not assign.getEnclosingFunction().isTestFunction() and
  not assign.isInConfigurationFile()
select
  assign,
  "Hardcoded credential '" + target.getName() +
  "' = '" + value.getStringValue().prefix(8) + "...' in " +
  assign.getEnclosingFunction().getQualifiedName()
```

### 4.2 BSQL Compiler Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    BSQL Query Pipeline                        │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  .bsql file                                                   │
│     │                                                         │
│     ▼                                                         │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────────┐   │
│  │  Lexer   │───▶│    Parser    │───▶│ Semantic Analyzer │   │
│  │ (logos)  │    │ (pest PEG)   │    │ (type checker)    │   │
│  └──────────┘    └──────────────┘    └────────┬──────────┘   │
│                                                │              │
│                                                ▼              │
│  ┌──────────────────────────────────────────────────────┐    │
│  │              BSQL IR (Intermediate Representation)    │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │    │
│  │  │ AST      │  │ CFG      │  │ DataFlow         │   │    │
│  │  │ Patterns │  │ Patterns │  │ Patterns         │   │    │
│  │  └──────────┘  └──────────┘  └──────────────────┘   │    │
│  └──────────────────────┬───────────────────────────────┘    │
│                          │                                    │
│                          ▼                                    │
│  ┌──────────────────────────────────────────────────────┐    │
│  │           CPG Query Plan Generator                    │    │
│  │  Produces optimized CPG traversal operations          │    │
│  └──────────────────────┬───────────────────────────────┘    │
│                          │                                    │
│                          ▼                                    │
│  ┌──────────────────────────────────────────────────────┐    │
│  │           CPG Query Executor (in-memory)              │    │
│  │  Executes graph traversals against the loaded CPG     │    │
│  └──────────────────────┬───────────────────────────────┘    │
│                          │                                    │
│                          ▼                                    │
│                    Finding Stream                             │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

### 4.3 Full BSQL Rule Catalog (100+ Rules)

The following is the complete catalog of BSQL rules organized by vulnerability
category. Each rule is annotated with CWE ID, severity, precision, and the scanner
module responsible for execution.

#### Injection Rules (Rules 1-25)
1. **SQL Injection — String Concatenation** (CWE-89, HIGH, high-precision)
2. **SQL Injection — ORM Raw Queries** (CWE-89, HIGH, high-precision)
3. **Command Injection — Shell Execution** (CWE-78, CRITICAL, high-precision)
4. **Command Injection — subprocess shell=True** (CWE-78, CRITICAL, high-precision)
5. **Path Traversal — File Access** (CWE-22, HIGH, high-precision)
6. **Path Traversal — Archive Extraction** (CWE-22, HIGH, medium-precision)
7. **LDAP Injection** (CWE-90, HIGH, high-precision)
8. **XPath Injection** (CWE-643, HIGH, high-precision)
9. **Expression Language Injection** (CWE-917, HIGH, medium-precision)
10. **Server-Side Template Injection** (CWE-1336, CRITICAL, medium-precision)
11. **SSRF — HTTP Client URL** (CWE-918, HIGH, high-precision)
12. **SSRF — File Fetch via URL** (CWE-918, HIGH, medium-precision)
13. **Email Header Injection** (CWE-93, MEDIUM, high-precision)
14. **HTTP Header Injection** (CWE-113, MEDIUM, high-precision)
15. **HTTP Response Splitting** (CWE-113, MEDIUM, medium-precision)
16. **Log Injection — CRLF** (CWE-117, MEDIUM, high-precision)
17. **CSV Formula Injection** (CWE-1236, MEDIUM, high-precision)
18. **XXE — XML External Entity** (CWE-611, HIGH, high-precision)
19. **XXE — XML Entity Expansion (Billion Laughs)** (CWE-776, HIGH, high-precision)
20. **Deserialization — Python pickle** (CWE-502, CRITICAL, high-precision)
21. **Deserialization — Java ObjectInputStream** (CWE-502, CRITICAL, high-precision)
22. **Deserialization — YAML.load (unsafe)** (CWE-502, CRITICAL, high-precision)
23. **XSS — Reflected (server-side rendering)** (CWE-79, HIGH, high-precision)
24. **XSS — Stored (DB to HTML)** (CWE-79, HIGH, medium-precision)
25. **NoSQL Injection — MongoDB $where** (CWE-943, HIGH, high-precision)

#### Authentication Rules (Rules 26-45)
26. **Missing Authentication — Unprotected Endpoint** (CWE-306, CRITICAL, high-precision)
27. **Hardcoded Credentials — General** (CWE-798, CRITICAL, high-precision)
28. **Hardcoded API Key — Cloud Services** (CWE-798, CRITICAL, high-precision)
29. **Hardcoded JWT Secret** (CWE-798, CRITICAL, high-precision)
30. **Hardcoded Database Password** (CWE-798, CRITICAL, high-precision)
31. **Weak Password Hash — MD5** (CWE-328, HIGH, high-precision)
32. **Weak Password Hash — SHA1** (CWE-328, HIGH, high-precision)
33. **Unsalted Password Hash** (CWE-759, HIGH, medium-precision)
34. **Insufficient PBKDF2 Iterations (<100k)** (CWE-916, HIGH, high-precision)
35. **Missing CSRF Token — Form** (CWE-352, HIGH, high-precision)
36. **Missing CSRF Token — AJAX** (CWE-352, HIGH, medium-precision)
37. **JWT — Missing Signature Verification** (CWE-347, CRITICAL, high-precision)
38. **JWT — "none" Algorithm Accepted** (CWE-347, CRITICAL, high-precision)
39. **Open Redirect — Unvalidated URL Parameter** (CWE-601, MEDIUM, high-precision)
40. **Open Redirect — Double-Encoding Bypass** (CWE-601, MEDIUM, medium-precision)
41. **Session Fixation — Cookie Not Regenerated** (CWE-384, HIGH, medium-precision)
42. **Missing HttpOnly Flag** (CWE-1004, MEDIUM, high-precision)
43. **Missing Secure Flag on Cookie** (CWE-614, MEDIUM, high-precision)
44. **Missing SameSite Flag** (CWE-1275, MEDIUM, high-precision)
45. **Weak Session ID — Insufficient Entropy** (CWE-330, HIGH, medium-precision)

#### Cryptographic Weakness Rules (Rules 46-60)
46. **ECB Mode for Block Cipher** (CWE-327, HIGH, high-precision)
47. **Static IV for CBC Mode** (CWE-329, HIGH, high-precision)
48. **Predictable IV from Fixed Value** (CWE-1204, HIGH, high-precision)
49. **Insufficient RSA Key Length (<2048)** (CWE-326, HIGH, high-precision)
50. **Weak Key Generation — Predictable Seed** (CWE-337, HIGH, high-precision)
51. **Weak PRNG — Math.random()** (CWE-338, HIGH, high-precision)
52. **Weak PRNG — rand() for Security** (CWE-338, HIGH, high-precision)
53. **Hardcoded Cryptographic Key** (CWE-321, CRITICAL, high-precision)
54. **Hardcoded IV/Nonce** (CWE-329, CRITICAL, high-precision)
55. **Missing Certificate Validation** (CWE-295, HIGH, high-precision)
56. **Allowed All Hostnames in TLS** (CWE-297, HIGH, high-precision)
57. **Weak Cipher Suite — RC4** (CWE-327, HIGH, high-precision)
58. **Weak Cipher Suite — DES/3DES** (CWE-327, HIGH, high-precision)
59. **Missing Forward Secrecy** (CWE-310, MEDIUM, high-precision)
60. **RSA Without OAEP Padding** (CWE-780, HIGH, high-precision)

#### Information Disclosure Rules (Rules 61-75)
61. **Stack Trace in HTTP Response** (CWE-209, MEDIUM, high-precision)
62. **Database Error in Response** (CWE-209, HIGH, high-precision)
63. **Server Version Header** (CWE-200, LOW, high-precision)
64. **Internal IP in Response** (CWE-200, MEDIUM, high-precision)
65. **File Path in Error Message** (CWE-209, MEDIUM, high-precision)
66. **Source Code Disclosed in Response** (CWE-540, HIGH, high-precision)
67. **Debug Endpoint in Production** (CWE-489, HIGH, high-precision)
68. **.git Directory Accessible** (CWE-527, CRITICAL, high-precision)
69. **.env File Accessible** (CWE-527, CRITICAL, high-precision)
70. **Backup File Exposed** (CWE-530, MEDIUM, high-precision)
71. **Sensitive Data in Log Statement** (CWE-532, HIGH, high-precision)
72. **Sensitive Data in Environment Variable** (CWE-526, MEDIUM, high-precision)
73. **API Key in Client-Side JavaScript** (CWE-1203, CRITICAL, high-precision)
74. **Password in URL Query String** (CWE-598, HIGH, high-precision)
75. **Timing Side-Channel — String Comparison** (CWE-208, MEDIUM, low-precision)

#### Resource Management Rules (Rules 76-85)
76. **Unclosed File Handle** (CWE-404, HIGH, medium-precision)
77. **Unclosed Database Connection** (CWE-404, HIGH, high-precision)
78. **Unclosed Socket** (CWE-404, HIGH, medium-precision)
79. **Double Close of Resource** (CWE-675, MEDIUM, medium-precision)
80. **Use After Free — C/C++** (CWE-416, CRITICAL, medium-precision)
81. **Double Free — C/C++** (CWE-415, CRITICAL, medium-precision)
82. **Memory Leak — Allocated Not Freed** (CWE-401, MEDIUM, medium-precision)
83. **Infinite Loop — Missing Exit Condition** (CWE-835, HIGH, medium-precision)
84. **Uncontrolled Allocation Size — User Input** (CWE-789, HIGH, high-precision)
85. **Missing Connection Pooling** (CWE-410, MEDIUM, high-precision)

#### Input Validation Rules (Rules 86-95)
86. **Missing Input Validation — General** (CWE-20, HIGH, medium-precision)
87. **Unsafe Type Cast — Downcast** (CWE-704, MEDIUM, high-precision)
88. **Integer Overflow in Allocation** (CWE-680, HIGH, high-precision)
89. **Integer Overflow in Bounds Check** (CWE-190, HIGH, high-precision)
90. **Off-by-One in Array Access** (CWE-193, HIGH, high-precision)
91. **Negative Array Index** (CWE-129, HIGH, high-precision)
92. **Length Parameter Mismatch** (CWE-130, HIGH, high-precision)
93. **Path Traversal — ../ Sequences** (CWE-22, HIGH, high-precision)
94. **File Extension Validation Bypass** (CWE-646, HIGH, medium-precision)
95. **Content-Type Validation Bypass** (CWE-434, HIGH, medium-precision)

#### Concurrency Rules (Rules 96-100)
96. **Broken Double-Checked Locking** (CWE-609, HIGH, medium-precision)
97. **Unsynchronized Singleton** (CWE-543, HIGH, high-precision)
98. **Missing Lock on Shared Resource** (CWE-667, HIGH, high-precision)
99. **Potential Deadlock — Circular Lock** (CWE-833, HIGH, medium-precision)
100. **Non-Thread-Safe Object Shared** (CWE-567, HIGH, medium-precision)

#### Mobile and Embedded Rules (Rules 101-105)
101. **Android Exported Component Without Permission** (CWE-926, HIGH, high-precision)
102. **Android Implicit Intent with Sensitive Data** (CWE-927, MEDIUM, high-precision)
103. **iOS Sensitive Data in NSUserDefaults** (CWE-922, HIGH, high-precision)
104. **Hardcoded API Key in Mobile App** (CWE-798, CRITICAL, high-precision)
105. **Insecure WebView Configuration** (CWE-939, HIGH, high-precision)

---

## 5. DAST Expansion: Dynamic Application Security Testing

### 5.1 HTTP Endpoint Probing Module

The `dast-http` module performs black-box dynamic testing against live web
applications. It operates within the BugSwarm network sandbox to ensure safe,
contained probing.

```rust
/// HTTP Prober configuration for DAST endpoint discovery and fuzzing.
pub struct HttpProberConfig {
    /// Target base URL (e.g., https://example.com)
    pub target_url: Url,

    /// Authentication configuration for authenticated scanning
    pub auth: Option<AuthConfig>,

    /// Scope constraints (domains/paths to include/exclude)
    pub scope: ScopeConfig,

    /// Rate limiting (requests per second)
    pub rate_limit: Option<u32>,

    /// Maximum crawl depth
    pub max_depth: u32,

    /// Request timeout
    pub request_timeout: Duration,

    /// User agent string
    pub user_agent: String,

    /// Enable JavaScript rendering via headless browser
    pub enable_js_rendering: bool,

    /// Proxy configuration for request interception and modification
    pub proxy: Option<ProxyConfig>,
}

/// The HTTP Prober performs endpoint discovery and vulnerability testing.
/// It crawls the target application, fingerprints technologies, and
/// executes vulnerability probes against discovered endpoints.
pub struct HttpProber {
    config: HttpProberConfig,
    client: reqwest::Client,
    browser: Option<headless_chrome::Browser>,
    // ... additional fields
}

impl HttpProber {
    /// Crawl the target application to discover all reachable endpoints.
    /// Returns a list of discovered endpoints with their HTTP methods,
    /// parameters, and response characteristics.
    pub async fn crawl(&self) -> Result<Vec<DiscoveredEndpoint>, ProberError> {
        // 1. Start with seed URLs from configuration
        // 2. Request each URL, parse HTML for links and forms
        // 3. If JS rendering enabled, use headless browser for SPA crawling
        // 4. Extract API endpoints from JavaScript files
        // 5. Fingerprint technology stack (headers, cookies, HTML patterns)
        // 6. Respect robots.txt, scope constraints, rate limits
        // 7. Deduplicate by normalized URL
        todo!("Implement crawl logic")
    }

    /// Fingerprint the target application's technology stack.
    pub async fn fingerprint(&self) -> Result<TechnologyStack, ProberError> {
        // Analyze: Server header, X-Powered-By, cookies (JSESSIONID, PHPSESSID),
        // HTML generator meta tags, JavaScript framework patterns,
        // CSS class naming conventions, file extensions
        todo!("Implement fingerprinting")
    }

    /// Test for common web vulnerabilities on discovered endpoints.
    /// Returns a stream of vulnerability findings.
    pub async fn test_vulnerabilities(
        &self,
        endpoints: &[DiscoveredEndpoint],
    ) -> Result<Pin<Box<dyn Stream<Item = Finding> + Send>>, ProberError> {
        // For each endpoint, test:
        // - SQL injection (parameter fuzzing with SQLi payloads)
        // - XSS (reflected parameters with XSS payloads)
        // - Command injection (parameters with command injection payloads)
        // - Path traversal (path parameters with ../ sequences)
        // - File inclusion (parameters with file://, php:// URLs)
        // - SSRF (URL parameters with internal IPs and callback URLs)
        // - XXE (XML endpoints with entity injection payloads)
        // - HTTP method override (X-HTTP-Method-Override header)
        // - Host header injection
        // - Cache poisoning
        todo!("Implement vulnerability testing")
    }
}
```

### 5.2 API Schema Fuzzing

The `dast-api-fuzz` module consumes API schema documents (OpenAPI, GraphQL,
gRPC protobuf) and generates intelligent fuzz test cases:

```rust
/// API Fuzzer that generates test cases from schema documents.
pub struct ApiFuzzer {
    spec: ApiSpec,
    fuzzing_strategies: Vec<Box<dyn FuzzingStrategy>>,
}

/// Supported API specification formats.
pub enum ApiSpec {
    OpenApiV2(openapiv2::Spec),
    OpenApiV3(openapiv3::Spec),
    GraphQL(graphql_parser::Schema),
    Grpc(prost_types::FileDescriptorSet),
}

/// Fuzzing strategies generate test cases based on different approaches.
#[async_trait]
pub trait FuzzingStrategy: Send + Sync {
    /// Generate fuzz test cases for the given operation.
    async fn generate_test_cases(
        &self,
        operation: &ApiOperation,
        schema: &ApiSpec,
    ) -> Vec<FuzzTestCase>;

    /// Strategy name for reporting.
    fn name(&self) -> &str;
}

/// Available fuzzing strategies.
pub struct BoundaryValueStrategy;    // Min/max/zero/negative values
pub struct TypeConfusionStrategy;    // String for int, array for object, etc.
pub struct MissingParameterStrategy; // Omit required parameters
pub struct ExtraParameterStrategy;  // Add unexpected parameters
pub struct InjectionPayloadStrategy; // SQLi, XSS, command injection payloads
pub struct AuthBypassStrategy;      // Missing/modified auth headers
pub struct RateLimitStrategy;       // Rapid repeated requests
pub struct RaceConditionStrategy;   // Parallel conflicting requests
pub struct StructuralMutationStrategy; // Mutate JSON/XML structure
pub struct EncodingMutationStrategy;   // Double encoding, Unicode bypass

/// A generated fuzz test case ready for execution.
pub struct FuzzTestCase {
    pub endpoint: String,
    pub method: http::Method,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub expected_status: Option<u16>,
    pub strategy_name: String,
    pub description: String,
}
```

### 5.3 Authentication Bypass Testing

The `dast-auth-bypass` module systematically tests authentication mechanisms:

```rust
/// Authentication bypass tester.
pub struct AuthBypassTester {
    config: AuthBypassConfig,
    test_flows: Vec<Box<dyn AuthTestFlow>>,
}

/// Authentication test flows execute specific bypass strategies.
#[async_trait]
pub trait AuthTestFlow: Send + Sync {
    async fn execute(
        &self,
        context: &AuthTestContext,
    ) -> Result<AuthTestResult, AuthTestError>;
}

/// Implemented test flows:
/// 1. MissingAuthFlow: Access protected endpoints without any authentication
/// 2. WeakPasswordFlow: Test common/weak passwords against login
/// 3. DefaultCredentialFlow: Test vendor default credentials
/// 4. SessionFixationFlow: Pre-login session accepted post-login
/// 5. SessionReplayFlow: Replay captured session tokens
/// 6. JWTForgeryFlow: None algorithm, key confusion, token tampering
/// 7. OAuthRedirectFlow: Open redirect in OAuth redirect_uri
/// 8. MfaBypassFlow: Skip MFA, reuse MFA token, brute-force MFA
/// 9. PasswordResetFlow: Token prediction, host header injection, rate limit
/// 10. RememberMeFlow: Predictable remember-me token
/// 11. RoleEscalationFlow: User role modification, privilege boundary crossing
/// 12. IdorFlow: Direct object reference enumeration and modification
```

---

## 6. Cryptography Analysis Module

### 6.1 Detection Capabilities

The `sast-crypto` scanner module detects cryptographic weaknesses through
static analysis of cryptographic API usage:

```
Category                    Detection Method                  CWEs Covered
──────────────────────────────────────────────────────────────────────────
Weak cipher detection       Algorithm identifier matching     CWE-327
Randomness validation       Seed source and PRNG analysis     CWE-330, CWE-337, CWE-338
Key length checking         Static key size calculation       CWE-326
Key management              Hardcoded key detection           CWE-321, CWE-798
IV/Nonce analysis           Static/predictable IV detection    CWE-329, CWE-1204
Hash function strength      Algorithm identification + salt   CWE-328, CWE-759, CWE-760
Padding scheme validation   RSA-OAEP, AES-CBC padding checks  CWE-780
Certificate validation      Hostname verifier, trust manager  CWE-295, CWE-297
Forward secrecy             Cipher suite analysis             CWE-310
Algorithm negotiation       TLS downgrade detection           CWE-757
```

### 6.2 Cryptographic API Database

The scanner maintains a comprehensive database of cryptographic APIs across
all supported languages, annotated with security properties:

```toml
# bugswarm-scanner/scanners/sast_crypto/api_db.toml

[[apis]]
language = "python"
function = "hashlib.md5"
category = "weak_hash"
cwe = "CWE-328"
severity = "HIGH"
safe_alternative = "hashlib.sha256"
notes = "MD5 is cryptographically broken. Use SHA-256 or SHA-3."

[[apis]]
language = "python"
function = "random.random"
category = "weak_prng"
cwe = "CWE-338"
severity = "HIGH"
safe_alternative = "secrets.token_bytes"
notes = "random module is not suitable for security purposes."

[[apis]]
language = "python"
function = "cryptography.hazmat.primitives.ciphers.modes.ECB"
category = "weak_mode"
cwe = "CWE-327"
severity = "HIGH"
safe_alternative = "cryptography.hazmat.primitives.ciphers.modes.CBC"
notes = "ECB mode reveals data patterns. Use CBC or GCM."

# ... 500+ more API entries across Rust, Go, Java, JavaScript, Ruby, PHP, C/C++
```

---

## 7. Configuration Scanning Modules

### 7.1 Dockerfile Security Scanner

The `config-docker` module parses Dockerfiles into an AST and applies security
rules from a configurable rule database:

```rust
/// Dockerfile security rule.
pub struct DockerfileRule {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: Severity,
    pub cwe: Option<CweId>,
    pub check: DockerfileCheck,
}

/// Types of Dockerfile checks.
pub enum DockerfileCheck {
    /// Check that the base image matches a pattern (e.g., not :latest)
    BaseImagePattern { pattern: String, message: String },

    /// Check that a specific instruction is present (e.g., HEALTHCHECK)
    InstructionRequired { instruction: String, message: String },

    /// Check that a specific instruction is NOT present (e.g., ADD)
    InstructionForbidden { instruction: String, message: String },

    /// Check that RUN commands don't contain dangerous patterns
    RunCommandCheck { forbidden_pattern: String, message: String },

    /// Check that USER is not root
    NonRootUser { message: String },

    /// Check that EXPOSE ports are within expected range
    PortRangeCheck { min: u16, max: u16, message: String },

    /// Check that secrets aren't hardcoded in ENV/ARG
    NoSecretsInEnv { message: String },

    /// Check for package pinning in RUN apt-get/install
    PackagePinningCheck { message: String },
}
```

#### Dockerfile Security Rules (20+ rules):
1. **DKR-001**: Use specific base image tags, not `:latest`
2. **DKR-002**: Do not run as root (USER instruction required)
3. **DKR-003**: Add HEALTHCHECK instruction
4. **DKR-004**: Avoid ADD — use COPY instead
5. **DKR-005**: Do not expose unnecessary ports
6. **DKR-006**: Pin package versions in apt-get/yum/apk
7. **DKR-007**: Do not store secrets in ENV (use build args or secrets)
8. **DKR-008**: Clean up apt cache after install (rm -rf /var/lib/apt/lists/*)
9. **DKR-009**: Use --no-install-recommends with apt-get
10. **DKR-010**: Verify downloaded binary checksums (curl ... | sh is dangerous)
11. **DKR-011**: Set WORKDIR for path clarity
12. **DKR-012**: Use COPY --chown for permission control
13. **DKR-013**: Avoid ADD with remote URLs (unverified content)
14. **DKR-014**: Set --no-cache-dir for pip install
15. **DKR-015**: Remove build dependencies in final stage (multi-stage builds)
16. **DKR-016**: Do not hardcode credentials in ENV or ARG
17. **DKR-017**: Limit container capabilities (drop ALL, add only needed)
18. **DKR-018**: Set read-only root filesystem when possible
19. **DKR-019**: Avoid privileged mode
20. **DKR-020**: Scan base image for known vulnerabilities

### 7.2 Kubernetes Manifest Scanner

The `config-k8s` module parses Kubernetes YAML manifests (Deployment, Pod,
Service, ConfigMap, Secret, Ingress, NetworkPolicy) and checks against a
comprehensive security rule set.

#### Kubernetes Security Rules (30+ rules):
1. **K8S-001**: Do not run containers as root (securityContext.runAsNonRoot)
2. **K8S-002**: Drop all Linux capabilities, add only required
3. **K8S-003**: Set readOnlyRootFilesystem to true
4. **K8S-004**: Do not use privileged containers
5. **K8S-005**: Set resource requests and limits for all containers
6. **K8S-006**: Use specific image tags, not :latest
7. **K8S-007**: Do not mount host filesystem volumes (hostPath)
8. **K8S-008**: Enable seccomp profiles (RuntimeDefault or custom)
9. **K8S-009**: Enable AppArmor or SELinux profiles
10. **K8S-010**: Set Pod Security Standard (restricted)
11. **K8S-011**: Do not use default service account
12. **K8S-012**: Disable automountServiceAccountToken when not needed
13. **K8S-013**: Use NetworkPolicy to restrict pod communication
14. **K8S-014**: Do not expose secrets in ConfigMap (use Secret resources)
15. **K8S-015**: Enable encryption at rest for Secrets
16. **K8S-016**: Set liveness and readiness probes
17. **K8S-017**: Restrict hostPort usage
18. **K8S-018**: Do not use hostNetwork or hostPID
19. **K8S-019**: Set fsGroup for volume permission management
20. **K8S-020**: Use imagePullPolicy: Always for mutable tags
21. **K8S-021**: Avoid hostIPC
22. **K8S-022**: Set allowPrivilegeEscalation to false
23. **K8S-023**: Use RuntimeClass for gVisor/Kata Containers
24. **K8S-024**: Enable audit logging
25. **K8S-025**: Restrict Ingress to TLS-only
26. **K8S-026**: Set PodDisruptionBudget for availability
27. **K8S-027**: Use dedicated namespaces for isolation
28. **K8S-028**: Enable RBAC and follow least privilege
29. **K8S-029**: Scan image registries for vulnerability reports
30. **K8S-030**: Enable OPA/Gatekeeper policy enforcement

### 7.3 Cloud IAM Policy Review

The `config-iam` module analyzes cloud IAM policies (AWS IAM, GCP IAM, Azure RBAC)
for security misconfigurations:

```rust
/// IAM policy analyzer.
pub struct IamPolicyAnalyzer {
    /// Provider-specific scanners
    aws: Option<AwsIamScanner>,
    gcp: Option<GcpIamScanner>,
    azure: Option<AzureRbacScanner>,

    /// Cross-cloud policy rules
    cross_cloud_rules: Vec<IamRule>,
}

/// IAM security rule.
pub struct IamRule {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: Severity,
    pub cwe: Option<CweId>,
    /// The check function
    pub check: Box<dyn Fn(&IamPolicy) -> Vec<IamFinding> + Send + Sync>,
}
```

#### IAM Security Rules (20+ rules):
1. **IAM-001**: No wildcard (*) in Action (least privilege violation)
2. **IAM-002**: No wildcard (*) in Resource (overly permissive)
3. **IAM-003**: S3 bucket with public read/write access
4. **IAM-004**: IAM user with console access and active access keys
5. **IAM-005**: Unused IAM access keys (rotation required)
6. **IAM-006**: Root account with access keys (never use root keys)
7. **IAM-007**: MFA not enabled for privileged users
8. **IAM-008**: Password policy without minimum length/complexity
9. **IAM-009**: Security group with 0.0.0.0/0 ingress
10. **IAM-010**: S3 bucket without encryption enabled
11. **IAM-011**: RDS instance with public accessibility
12. **IAM-012**: CloudTrail not enabled in all regions
13. **IAM-013**: KMS key with overly permissive key policy
14. **IAM-014**: Lambda function with admin IAM role
15. **IAM-015**: EC2 instance with instance profile having admin access
16. **IAM-016**: Service account with overly broad permissions (GCP)
17. **IAM-017**: Storage bucket with uniform bucket-level access disabled (GCP)
18. **IAM-018**: Azure Storage Account with public blob access
19. **IAM-019**: Overly permissive firewall rules
20. **IAM-020**: Missing resource-level IAM conditions

---

## 8. SCA: Dependency Vulnerability Analysis

### 8.1 SBOM Generation

The `sca-dep` module generates Software Bills of Materials (SBOMs) in both
CycloneDX and SPDX formats:

```rust
/// SBOM generator for comprehensive dependency inventory.
pub struct SbomGenerator {
    /// SBOM format to generate
    format: SbomFormat,

    /// Resolve transitive dependencies
    resolve_transitive: bool,

    /// Include dev dependencies
    include_dev: bool,
}

pub enum SbomFormat {
    CycloneDX_1_4,
    CycloneDX_1_5,
    Spdx_2_3,
    Spdx_3_0,
}
```

### 8.2 Per-Language Audit Integration

```rust
/// Cargo audit integration for Rust projects.
pub struct CargoAuditRunner {
    /// Path to cargo-audit binary or use embedded
    audit_binary: Option<PathBuf>,

    /// Advisory database URL (default: RustSec)
    advisory_db_url: Url,

    /// Ignore specific advisory IDs
    ignore: Vec<String>,
}

impl CargoAuditRunner {
    /// Run cargo audit and parse results.
    pub async fn audit(&self, project_dir: &Path) -> Result<Vec<AdvisoryFinding>> {
        // 1. Generate Cargo.lock if not present
        // 2. Run: cargo audit --json
        // 3. Parse JSON output into typed AdvisoryFindings
        // 4. Cross-reference with internal advisory DB
        // 5. Enrich with exploit availability, fix version, severity
        todo!("Implement cargo audit")
    }
}

/// NPM audit integration.
pub struct NpmAuditRunner {
    pub registry_url: Url,
    pub ignore: Vec<String>,
}

impl NpmAuditRunner {
    /// Run npm audit and parse results.
    pub async fn audit(&self, project_dir: &Path) -> Result<Vec<AdvisoryFinding>> {
        // 1. Generate package-lock.json if not present
        // 2. Run: npm audit --json
        // 3. Parse JSON output
        // 4. Cross-reference with GitHub Advisory DB
        todo!("Implement npm audit")
    }
}

/// Pip audit integration.
pub struct PipAuditRunner {
    pub ignore: Vec<String>,
}

impl PipAuditRunner {
    /// Run pip-audit and parse results.
    pub async fn audit(&self, project_dir: &Path) -> Result<Vec<AdvisoryFinding>> {
        // 1. Run: pip-audit --format=json -r requirements.txt
        // 2. Parse JSON output
        // 3. Cross-reference with PyPA Advisory DB
        todo!("Implement pip audit")
    }
}

/// Maven/OWASP Dependency-Check integration.
pub struct MavenAuditRunner {
    pub nvd_api_key: Option<String>,
}

/// Bundler-audit integration.
pub struct BundlerAuditRunner {
    pub ignore: Vec<String>,
}
```

### 8.3 Unified Advisory Database

```rust
/// Centralized vulnerability advisory database aggregating multiple sources.
pub struct AdvisoryDatabase {
    /// In-memory index of all known vulnerabilities
    index: Arc<RwLock<AdvisoryIndex>>,

    /// Data sources
    sources: Vec<Box<dyn AdvisorySource>>,
}

/// Advisory sources are polled on a configurable schedule.
#[async_trait]
pub trait AdvisorySource: Send + Sync {
    /// Source identifier (e.g., "ghsa", "rustsec", "nvd")
    fn source_id(&self) -> &str;

    /// Fetch all advisories from this source.
    async fn fetch_all(&self) -> Result<Vec<Advisory>>;

    /// Poll for new advisories since last fetch.
    async fn fetch_updates(&self, since: DateTime<Utc>) -> Result<Vec<Advisory>>;

    /// Health check for this source.
    async fn health_check(&self) -> Result<SourceHealth>;
}

/// Implemented advisory sources:
/// - GitHub Advisory Database (GHSA) — via GraphQL API
/// - RustSec Advisory Database — via git repository
/// - NPM Advisory Database — via npm registry API
/// - PyPA Advisory Database — via PyPI JSON API
/// - OSS-Fuzz vulnerability reports — via oss-fuzz.com API
/// - NVD/CVE — via NVD API 2.0
/// - OSV — via OSV.dev API
/// - Snyk Vulnerability Database — via Snyk API
/// - Sonatype OSS Index — via REST API
```

---

## 9. Race Condition Detection

### 9.1 Multi-Language Race Detection

```rust
/// Race condition scanner supporting multiple runtime sanitizers.
pub struct RaceConditionScanner {
    /// TSAN integration for C/C++ and Rust
    tsan: Option<TsanRunner>,

    /// Go race detector (-race flag)
    go_race: Option<GoRaceRunner>,

    /// JavaScript race detection (custom instrumentation)
    js_race: Option<JsRaceRunner>,

    /// Static race detection via CPG analysis
    static_analyzer: StaticRaceAnalyzer,
}

/// Thread Sanitizer runner for native code.
pub struct TsanRunner {
    /// Path to TSAN-instrumented binary
    binary: PathBuf,

    /// TSAN suppression file
    suppression_file: Option<PathBuf>,

    /// TSAN options
    options: TsanOptions,
}

impl TsanRunner {
    /// Execute the binary under TSAN and parse race reports.
    pub async fn detect_races(
        &self,
        input: &[u8],
        sandbox: &SandboxHandle,
    ) -> Result<Vec<RaceReport>> {
        // 1. Run binary with TSAN_OPTIONS in sandbox
        // 2. Parse TSAN output for data race reports
        // 3. Each report includes: thread 1 stack, thread 2 stack,
        //    shared variable location, access type (read/write)
        // 4. Map back to source code locations via debug info
        todo!("Implement TSAN runner")
    }
}

/// Go race detector runner.
pub struct GoRaceRunner {
    /// Path to Go binary with -race
    binary: PathBuf,
}

impl GoRaceRunner {
    pub async fn detect_races(
        &self,
        input: &[u8],
        sandbox: &SandboxHandle,
    ) -> Result<Vec<RaceReport>> {
        // 1. Run Go binary compiled with -race in sandbox
        // 2. Parse race detector output (WARNING: DATA RACE)
        // 3. Extract goroutine stacks and source locations
        todo!("Implement Go race runner")
    }
}

/// JavaScript race condition detector using custom Node.js instrumentation.
pub struct JsRaceRunner {
    /// Path to Node.js test suite
    test_path: PathBuf,

    /// Custom async hook instrumentation
    instrumentation: JsRaceInstrumentation,
}

/// Static race condition analyzer using CPG.
pub struct StaticRaceAnalyzer {
    /// Lock analysis: mutex, RwLock, sync.Mutex usage patterns
    lock_analyzer: LockAnalyzer,

    /// Concurrent access analysis: shared state access patterns
    concurrent_access_analyzer: ConcurrentAccessAnalyzer,

    /// TOCTOU pattern detection: check-then-act on shared resources
    toctou_detector: ToctouDetector,

    /// Signal handler race detection
    signal_race_detector: SignalRaceDetector,
}
```

### 9.2 Race Condition CWEs Detected

| CWE ID | Name | Method | FPR | TTD |
|--------|------|--------|-----|-----|
| CWE-362 | Concurrent Execution with Improper Synchronization | TSAN + CPG lock analysis | <8% | <25s |
| CWE-363 | Race Condition Enabling Link Following | TSAN + CPG TOCTOU | <10% | <30s |
| CWE-364 | Signal Handler Race Condition | CPG signal analysis + sandbox | <10% | <35s |
| CWE-365 | Race Condition in Switch | TSAN + CPG switch analysis | <8% | <25s |
| CWE-366 | Race Condition within a Thread | TSAN thread analysis | <8% | <25s |
| CWE-367 | TOCTOU Race Condition | CPG TOCTOU pattern + TSAN | <8% | <30s |
| CWE-368 | Context Switching Race Condition | TSAN context analysis | <10% | <35s |
| CWE-543 | Unsynchronized Singleton | CPG singleton pattern analysis | <5% | <15s |
| CWE-567 | Unsynchronized Access to Shared Data | TSAN data race detection | <6% | <20s |
| CWE-609 | Broken Double-Checked Locking | CPG locking pattern analysis | <3% | <10s |
| CWE-662 | Insufficient Synchronization | TSAN analysis | <6% | <20s |
| CWE-663 | Non-reentrant Function in Concurrent Context | CPG reentrancy analysis | <4% | <15s |
| CWE-667 | Insufficient Locking | CPG lock analysis | <5% | <15s |
| CWE-820 | Missing Synchronization | TSAN analysis | <5% | <20s |
| CWE-821 | Incorrect Synchronization | TSAN analysis | <5% | <20s |

---

## 10. Information Disclosure Detection

### 10.1 Error Message Pattern Matching

The `sast-info-disclosure` module scans for information leakage through error
messages, stack traces, debug output, and logging statements.

```rust
/// Information disclosure scanner.
pub struct InfoDisclosureScanner {
    /// Error message patterns to detect
    error_patterns: Vec<ErrorPattern>,

    /// Stack trace patterns per language
    stack_trace_patterns: Vec<StackTracePattern>,

    /// Sensitive data regex patterns
    sensitive_patterns: Vec<SensitiveDataPattern>,

    /// Debug code detection rules
    debug_rules: Vec<DebugCodeRule>,
}

/// Pattern for matching error messages containing sensitive data.
pub struct ErrorPattern {
    pub id: String,
    pub cwe: CweId,
    pub description: String,
    pub regex: Regex,
    pub severity: Severity,
    pub languages: Vec<String>,
}

/// Pre-configured error patterns:
/// - Database connection strings in error messages
/// - SQL query with parameters in error messages
/// - File system paths in error messages
/// - Internal IP addresses in error messages
/// - Stack traces with line numbers and file paths
/// - API keys or tokens in error messages
/// - User personal data (email, phone, SSN) in error output
/// - Server version information in error pages
/// - Framework debug info (Laravel debugbar, Django debug toolbar)
/// - Memory addresses in crash dumps
```

### 10.2 Stack Trace Leak Detection

The scanner detects stack trace leaks across multiple frameworks and languages:

- **Python**: Django DEBUG=True, Flask debug mode, FastAPI exception handlers
- **Java/Spring**: Whitelabel Error Page, Tomcat error pages, custom error mappings
- **Node.js**: Express error handler, Next.js error overlay, unhandled rejections
- **Ruby/Rails**: Rails error pages in production, Rack middleware exceptions
- **Go**: net/http default error handler, panic recovery middleware
- **PHP**: Laravel debug mode, Symfony error pages, WordPress WP_DEBUG
- **Rust**: Actix/Rocket/Axum default error handlers, unwrap/expect in handlers

---

## 11. Integration with Existing CPG + Sandbox + Agent Pipeline

### 11.1 Data Flow Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                     BugSwarm Enterprise Architecture                  │
├──────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │                      Input Layer                               │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │    │
│  │  │ Source   │  │ Binary   │  │ Container│  │ Live         │ │    │
│  │  │ Code     │  │ Artifact │  │ Image    │  │ Endpoint     │ │    │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘ │    │
│  └───────┼──────────────┼──────────────┼──────────────┼─────────┘    │
│          │              │              │              │               │
│          ▼              ▼              ▼              ▼               │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │                   Representation Layer                        │    │
│  │  ┌─────────────────────────────────────────────────────────┐ │    │
│  │  │            Code Property Graph (CPG)                      │ │    │
│  │  │  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────────┐  │ │    │
│  │  │  │ AST  │  │ CFG  │  │ DFG  │  │ PDG  │  │ Call     │  │ │    │
│  │  │  │      │  │      │  │      │  │      │  │ Graph    │  │ │    │
│  │  │  └──────┘  └──────┘  └──────┘  └──────┘  └──────────┘  │ │    │
│  │  └─────────────────────────────────────────────────────────┘ │    │
│  │  ┌──────────────────────┐  ┌──────────────────────────────┐ │    │
│  │  │ Dependency Graph     │  │ Configuration AST             │ │    │
│  │  │ (SBOM)               │  │ (Dockerfile, K8s, IAM)        │ │    │
│  │  └──────────────────────┘  └──────────────────────────────┘ │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                              │                                        │
│                              ▼                                        │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │                   Scanner Layer (NEW)                         │    │
│  │                                                                │    │
│  │  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐│    │
│  │  │ SAST       │ │ SAST       │ │ SAST       │ │ SAST       ││    │
│  │  │ Taint      │ │ Semantic   │ │ Crypto     │ │ Info       ││    │
│  │  │ Analysis   │ │ Queries    │ │ Analysis   │ │ Disclosure ││    │
│  │  └────────────┘ └────────────┘ └────────────┘ └────────────┘│    │
│  │  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐│    │
│  │  │ SAST       │ │ DAST       │ │ DAST       │ │ DAST       ││    │
│  │  │ Race       │ │ HTTP       │ │ API        │ │ Auth       ││    │
│  │  │ Detection  │ │ Prober     │ │ Fuzzer     │ │ Bypass     ││    │
│  │  └────────────┘ └────────────┘ └────────────┘ └────────────┘│    │
│  │  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐│    │
│  │  │ Config     │ │ Config     │ │ Config     │ │ SCA        ││    │
│  │  │ Docker     │ │ K8s        │ │ IAM        │ │ Dep Audit  ││    │
│  │  └────────────┘ └────────────┘ └────────────┘ └────────────┘│    │
│  └──────────────────────────────────────────────────────────────┘    │
│                              │                                        │
│                              ▼                                        │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │                   Execution Layer                             │    │
│  │  ┌──────────────────────┐  ┌──────────────────────────────┐ │    │
│  │  │ Sandbox Pool         │  │ Network Sandbox              │ │    │
│  │  │ (gVisor/Firecracker) │  │ (isolated eBPF/TAP)          │ │    │
│  │  └──────────────────────┘  └──────────────────────────────┘ │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                              │                                        │
│                              ▼                                        │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │                   Agent Pipeline                              │    │
│  │  ┌─────────────────────────────────────────────────────────┐ │    │
│  │  │ LLM Agent (DeepSeek/Claude/GPT-4)                        │ │    │
│  │  │ - Finding triage and prioritization                      │ │    │
│  │  │ - Proof-of-concept generation assistance                  │ │    │
│  │  │ - False positive suppression                             │ │    │
│  │  │ - Remediation guidance generation                        │ │    │
│  │  └─────────────────────────────────────────────────────────┘ │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                              │                                        │
│                              ▼                                        │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │                   Reporting Layer                             │    │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────────────────┐ │    │
│  │  │ SARIF      │  │ PDF        │  │ Compliance Mapping     │ │    │
│  │  │ Output     │  │ Report     │  │ (CWE, OWASP, CIS, PCI) │ │    │
│  │  └────────────┘  └────────────┘  └────────────────────────┘ │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                                                                       │
└──────────────────────────────────────────────────────────────────────┘
```

### 11.2 CPG Integration

The new scanner modules consume the existing CPG through the `ScannerContext`:

```rust
impl SastCodeqlSemanticScanner {
    /// Execute BSQL queries against the CPG.
    async fn execute_query(
        &self,
        query: &BsqlQuery,
        cpg: &CodePropertyGraph,
    ) -> Result<Vec<Finding>> {
        // The BSQL compiler translates declarative queries into
        // imperative CPG traversal operations:
        //
        // BSQL: from AssignExpr a where a.getTarget().getName() = "password"
        // CPG:  cpg.ast_nodes()
        //          .filter(|n| n.kind == NodeKind::Assignment)
        //          .filter(|n| n.children[0].name.matches("password"))
        //
        // The CPG provides:
        // - ast_nodes(): iterate all AST nodes by kind
        // - dataflow(from, to): find data flow paths between nodes
        // - callers(func): find all call sites of a function
        // - callees(site): find all functions called at a site
        // - dominates(a, b): check if node a dominates node b in CFG
        // - postdominates(a, b): check post-dominance
        // - taint_sources(): get all identified taint sources
        // - taint_sinks(): get all identified taint sinks
        todo!("Implement query execution against CPG")
    }
}
```

### 11.3 EventBus Integration for Scanner Correlation

The EventBus enables cross-scanner finding correlation. For example, a static
SQL injection finding can be correlated with a dynamic DAST confirmation:

```rust
pub struct EventBus {
    finding_tx: broadcast::Sender<FindingEvent>,
    correlation_engine: Arc<CorrelationEngine>,
}

impl EventBus {
    /// Publish a finding from any scanner.
    pub async fn publish_finding(&self, event: FindingEvent) -> Result<()> {
        // 1. Broadcast to all subscribers (correlation engine, telemetry, UI)
        // 2. Correlation engine checks if this finding overlaps with
        //    previously published findings from other scanners
        // 3. If correlation found (e.g., SAST + DAST agree), merge into
        //    a single higher-confidence finding
        self.finding_tx.send(event)?;
        Ok(())
    }
}

pub struct CorrelationEngine {
    /// Correlation rules
    rules: Vec<CorrelationRule>,
}

pub struct CorrelationRule {
    /// When TWO findings match these criteria, merge them
    pub scanner_a: String,  // e.g., "sast-codeql-semantic"
    pub scanner_b: String,  // e.g., "dast-http"
    pub cwe_match: bool,    // Both findings reference the same CWE
    pub location_proximity: f64, // Source locations must be within N lines
    pub merge_strategy: MergeStrategy,
}

pub enum MergeStrategy {
    /// Take the higher-confidence finding
    MaxConfidence,
    /// Combine both proofs into one finding
    CombineProofs,
    /// Create a new finding with weighted confidence
    WeightedAverage { weights: (f64, f64) },
}
```

---

## 12. Implementation Timeline

### Sprint Schedule (12 weeks, 4 engineers)

```
Week      Deliverable                                    Engineer Assignments
──────────────────────────────────────────────────────────────────────────────
Week 1-2  Crate scaffolding, trait definition,           E1,E2: Architecture
          ScannerRegistry, BSQL parser foundation,       E3: BSQL parser
          taxonomy data model                            E4: SBOM generator

Week 3-4  SAST scanners: taint (refactor), semantic      E1: sast-taint refactor
          queries (25 injection rules), crypto            E2: sast-codeql-semantic
          analysis (15 rules)                             E3: sast-crypto
                                                          E4: sast-info-disclosure

Week 5-6  SAST scanners: info disclosure (15 rules),     E1: sast-info-disclosure
          race detection (CPG + TSAN runner),             E2: sast-race
          remaining semantic rules (50 total)             E3: BSQL rule authoring
                                                          E4: advisory DB integration

Week 7-8  DAST scanners: HTTP prober, API fuzzer,        E1,E2: dast-http
          authentication bypass tester                    E3: dast-api-fuzz
                                                          E4: dast-auth-bypass

Week 9-10 Configuration scanners: Dockerfile, K8s,       E1: config-docker
          IAM policy analyzer, SCA audit runners          E2: config-k8s
          (cargo, npm, pip, maven)                        E3: config-iam
                                                          E4: SCA per-language runners

Week 11   Integration: EventBus correlation engine,       E1,E2: EventBus + CPG
          CPG integration, sandbox integration,           E3,E4: Sandbox + agent
          agent pipeline integration

Week 12   Testing: CWE coverage validation,               ALL: Testing + hardening
          false positive benchmarking,                    + performance tuning
          performance tuning, documentation
```

### Milestone Checklist

- [ ] **M1 (Week 2)**: Crate compiles, trait implemented, Registry functional, BSQL lexer/parser
- [ ] **M2 (Week 4)**: 40 BSQL rules operational, crypto scanner detects top 15 CWEs
- [ ] **M3 (Week 6)**: 105 BSQL rules total, race detector produces valid reports
- [ ] **M4 (Week 8)**: DAST modules crawl+fuzz real applications, detect OWASP Top 10
- [ ] **M5 (Week 10)**: Config scanners pass all rule tests, SBOM generated in both formats
- [ ] **M6 (Week 12)**: Full integration test suite passing, false positive rate <5%, TTD <30s

---

## 13. Quality Gates and Success Metrics

### 13.1 Per-Scanner Quality Gates

Each scanner module must pass before being enabled in production:

| Gate | Description | Threshold |
|------|-------------|-----------|
| **Coverage Gate** | CWE categories detected >= target | >= 80% of claimed CWEs |
| **FPR Gate** | False positive rate on Juliet test suite | <5% |
| **TPR Gate** | True positive rate on Juliet test suite | >90% |
| **TTD Gate** | Median time-to-detect on standard corpus | <30s |
| **Crash Safety** | Scanner crashes / 1000 scans | <1 |
| **Memory Limit** | Peak RSS during scan | <2GB |
| **Determinism** | Same input -> same findings (within 1%) | >99% |

### 13.2 Integration Test Suite

```
bugswarm-scanner/tests/
├── integration/
│   ├── full_pipeline.rs         -- End-to-end: source -> findings -> report
│   ├── cross_scanner_correlation.rs -- Verify EventBus correlation
│   ├── scanner_registry.rs      -- Dynamic registration and discovery
│   ├── parallel_execution.rs    -- Concurrent scanner execution
│   └── taxonomy_validation.rs   -- Verify all CWE mappings
├── fixtures/
│   ├── juliet_cwe_119/          -- NIST Juliet test suite (per CWE)
│   ├── owasp_benchmark/         -- OWASP Benchmark project
│   ├── dockerfile_samples/      -- Secure and insecure Dockerfiles
│   ├── k8s_manifests/           -- Secure and insecure K8s manifests
│   ├── iam_policies/            -- AWS/GCP/Azure IAM policy samples
│   └── dependency_projects/     -- Projects with known vulnerable deps
└── benchmarks/
    ├── fpr_benchmark.rs         -- False positive rate measurement
    ├── ttd_benchmark.rs         -- Time-to-detect measurement
    └── scale_benchmark.rs       -- Large codebase (>1M LOC) performance
```

---

## 14. Risk Register

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| BSQL query engine too slow on large CPGs | Medium | High | Implement query plan optimization, result caching, incremental CPG updates |
| DAST crawling overwhelmed by large web apps | Medium | Medium | Implement smart crawling (depth limits, path dedup, concurrent requests capped) |
| False positive rate exceeds 5% for certain scanners | High | Medium | Iterative ML-based FPR suppression, user feedback loop for rule tuning |
| TSAN integration unstable across platforms | Medium | High | Containerized TSAN execution with known-good instrumented binaries |
| Advisory DB becomes stale between syncs | Low | High | Hourly sync with TTL-based cache invalidation + offline mode fallback |
| Scanner plugin ABI instability | Low | Medium | Use C ABI for plugin boundary, version negotiation at load time |
| IAM policy scanner misses cloud-specific edge cases | Medium | Medium | Per-provider test suites with known-bad configurations, community rule contributions |
| Race condition scanner has high FPR due to benign races | High | Low | Suppression file support, severity grading for benign vs exploitable races |

---

## 15. Summary

This plan expands BugSwarm's vulnerability detection capability from 6 categories
to 604 CWE-mapped detection layers across 12 domain-specific scanner modules.
The architecture is designed for:

1. **Extensibility**: New scanner modules can be added without modifying core infrastructure
2. **Performance**: Parallel execution with streaming findings for low latency
3. **Accuracy**: Multi-scanner correlation reduces false positives
4. **Comprehensiveness**: 604 CWEs covering memory safety, injection, auth,
   crypto, info disclosure, configuration, dependency, and race conditions
5. **Operability**: Per-scanner health checks, metrics, and graceful degradation

The 12-week, 4-engineer timeline is aggressive but achievable given the existing
CPG and sandbox infrastructure. The pluggable architecture ensures that partial
delivery of scanner modules is still valuable — each module can ship independently.
