# BugSwarm CPG: Multi-Language Code Property Graph at Scale

## Executive Summary

This plan details the architecture, implementation strategy, and rollout timeline for extending BugSwarm's Code Property Graph (CPG) engine from its current multi-language support to **100+ programming languages** via tree-sitter grammars. The approach leverages tree-sitter's uniform AST representation to build a language-agnostic CPG pipeline that scales from syntactic analysis (Tier 3) through full semantic modeling (Tier 1).

---

## 1. Tree-Sitter Grammar Registry — 100+ Languages

Each entry below specifies the tree-sitter crate name, primary parser entrypoint, and the top-level `program` or `source_file` node type used as the CPG root. Languages are ordered alphabetically.

### 1.1 Tier 1 — Full Support (AST + CFG + Call Graph + Taint + SSA): 11 Languages

| # | Language | Crate | Root Node Type | File Extensions |
|---|----------|-------|----------------|-----------------|
| 1 | Python | `tree-sitter-python` | `module` | `.py`, `.pyi`, `.pyx`, `.pxd` |
| 2 | JavaScript | `tree-sitter-javascript` | `program` | `.js`, `.mjs`, `.cjs`, `.jsx` |
| 3 | TypeScript | `tree-sitter-typescript` | `program` | `.ts`, `.tsx`, `.mts`, `.cts` |
| 4 | C | `tree-sitter-c` | `translation_unit` | `.c`, `.h` |
| 5 | C++ | `tree-sitter-cpp` | `translation_unit` | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx` |
| 6 | Rust | `tree-sitter-rust` | `source_file` | `.rs` |
| 7 | Go | `tree-sitter-go` | `source_file` | `.go` |
| 8 | Java | `tree-sitter-java` | `program` | `.java` |
| 9 | C# | `tree-sitter-c-sharp` | `compilation_unit` | `.cs` |
| 10 | Ruby | `tree-sitter-ruby` | `program` | `.rb` |
| 11 | PHP | `tree-sitter-php` | `program` | `.php`, `.phtml`, `.php3`, `.php4`, `.php5` |

### 1.2 Tier 2 — AST + Call Graph + Basic Taint: 15 Languages

| # | Language | Crate | Root Node Type | File Extensions |
|---|----------|-------|----------------|-----------------|
| 12 | Kotlin | `tree-sitter-kotlin` | `source_file` | `.kt`, `.kts` |
| 13 | Swift | `tree-sitter-swift` | `source_file` | `.swift` |
| 14 | Scala | `tree-sitter-scala` | `compilation_unit` | `.scala`, `.sc` |
| 15 | Dart | `tree-sitter-dart` | `compilation_unit` | `.dart` |
| 16 | Lua | `tree-sitter-lua` | `chunk` | `.lua` |
| 17 | Perl | `tree-sitter-perl` | `source_file` | `.pl`, `.pm`, `.t` |
| 18 | R | `tree-sitter-r` | `program` | `.r`, `.R`, `.Rprofile` |
| 19 | Julia | `tree-sitter-julia` | `source_file` | `.jl` |
| 20 | Haskell | `tree-sitter-haskell` | `haskell` | `.hs`, `.lhs` |
| 21 | Elixir | `tree-sitter-elixir` | `source_file` | `.ex`, `.exs` |
| 22 | Erlang | `tree-sitter-erlang` | `source_file` | `.erl`, `.hrl` |
| 23 | Clojure | `tree-sitter-clojure` | `source` | `.clj`, `.cljs`, `.cljc`, `.edn` |
| 24 | OCaml | `tree-sitter-ocaml` | `compilation_unit` | `.ml`, `.mli` |
| 25 | Groovy | `tree-sitter-groovy` | `compilation_unit` | `.groovy`, `.gvy`, `.gy`, `.gsh` |
| 26 | Zig | `tree-sitter-zig` | `source_file` | `.zig`, `.zir` |

### 1.3 Tier 3 — AST + Call Graph Only: 80+ Languages

| # | Language | Crate | Root Node Type | File Extensions |
|---|----------|-------|----------------|-----------------|
| 27 | Bash | `tree-sitter-bash` | `program` | `.sh`, `.bash`, `.zsh`, `.ksh` |
| 28 | Make | `tree-sitter-make` | `build_file` | `Makefile`, `.mk` |
| 29 | CMake | `tree-sitter-cmake` | `source_file` | `CMakeLists.txt`, `.cmake` |
| 30 | Dockerfile | `tree-sitter-dockerfile` | `source_file` | `Dockerfile`, `*.dockerfile` |
| 31 | YAML | `tree-sitter-yaml` | `stream` | `.yaml`, `.yml` |
| 32 | JSON | `tree-sitter-json` | `document` | `.json` |
| 33 | TOML | `tree-sitter-toml` | `document` | `.toml` |
| 34 | XML | `tree-sitter-xml` | `document` | `.xml`, `.xsd`, `.xsl`, `.svg` |
| 35 | HTML | `tree-sitter-html` | `document` | `.html`, `.htm` |
| 36 | CSS | `tree-sitter-css` | `stylesheet` | `.css` |
| 37 | SCSS | `tree-sitter-scss` | `stylesheet` | `.scss` |
| 38 | Svelte | `tree-sitter-svelte` | `document` | `.svelte` |
| 39 | Vue | `tree-sitter-vue` | `component` | `.vue` |
| 40 | GraphQL | `tree-sitter-graphql` | `document` | `.graphql`, `.gql` |
| 41 | SQL | `tree-sitter-sql` | `program` | `.sql` |
| 42 | Verilog | `tree-sitter-verilog` | `source_text` | `.v`, `.vh`, `.sv` |
| 43 | VHDL | `tree-sitter-vhdl` | `design_file` | `.vhd`, `.vhdl` |
| 44 | SystemVerilog | `tree-sitter-verilog` | `source_text` | `.sv`, `.svh` |
| 45 | Ada | `tree-sitter-ada` | `compilation_unit` | `.adb`, `.ads` |
| 46 | Agda | `tree-sitter-agda` | `program` | `.agda` |
| 47 | Assembly (x86) | `tree-sitter-asm` | `program` | `.s`, `.S`, `.asm` |
| 48 | AWK | `tree-sitter-awk` | `program` | `.awk` |
| 49 | Bazel / Starlark | `tree-sitter-starlark` | `source_file` | `BUILD`, `.bzl`, `.sky` |
| 50 | BibTeX | `tree-sitter-bibtex` | `bibtex` | `.bib` |
| 51 | Bicep | `tree-sitter-bicep` | `program` | `.bicep` |
| 52 | BitBake | `tree-sitter-bitbake` | `source_file` | `.bb`, `.bbappend`, `.bbclass` |
| 53 | Cairo | `tree-sitter-cairo` | `source_file` | `.cairo` |
| 54 | Cap'n Proto | `tree-sitter-capnp` | `source_file` | `.capnp` |
| 55 | Chapel | `tree-sitter-chapel` | `source_file` | `.chpl` |
| 56 | Cooklang | `tree-sitter-cooklang` | `document` | `.cook` |
| 57 | Cpon | `tree-sitter-cpon` | `document` | `.cpon` |
| 58 | CUE | `tree-sitter-cue` | `source_file` | `.cue` |
| 59 | D | `tree-sitter-d` | `source_file` | `.d`, `.di` |
| 60 | Diff | `tree-sitter-diff` | `diff` | `.diff`, `.patch` |
| 61 | DOT | `tree-sitter-dot` | `graph` | `.dot`, `.gv` |
| 62 | Earthfile | `tree-sitter-earthfile` | `earthfile` | `Earthfile` |
| 63 | EBNF | `tree-sitter-ebnf` | `syntax` | `.ebnf` |
| 64 | EDS | `tree-sitter-eds` | `source_file` | `.eds` |
| 65 | Elm | `tree-sitter-elm` | `module` | `.elm` |
| 66 | Elvish | `tree-sitter-elvish` | `chunk` | `.elv` |
| 67 | Emacs Lisp | `tree-sitter-elisp` | `program` | `.el` |
| 68 | Fennel | `tree-sitter-fennel` | `source_file` | `.fnl` |
| 69 | Fish | `tree-sitter-fish` | `program` | `.fish` |
| 70 | Fortran | `tree-sitter-fortran` | `program` | `.f90`, `.f95`, `.f03`, `.f08`, `.f`, `.for` |
| 71 | Func | `tree-sitter-func` | `source_file` | `.fc` |
| 72 | GDScript | `tree-sitter-gdscript` | `source_file` | `.gd` |
| 73 | GLSL | `tree-sitter-glsl` | `translation_unit` | `.vert`, `.frag`, `.geom`, `.comp`, `.tesc`, `.tese` |
| 74 | GN (Generate Ninja) | `tree-sitter-gn` | `source_file` | `.gn`, `.gni` |
| 75 | Go Template | `tree-sitter-go-template` | `template` | `.gotmpl`, `.gotpl`, `.gohtml` |
| 76 | Godot Resource | `tree-sitter-godot-resource` | `document` | `.tscn`, `.tres` |
| 77 | Go Mod | `tree-sitter-go-mod` | `source_file` | `go.mod` |
| 78 | Go Sum | `tree-sitter-go-sum` | `source_file` | `go.sum` |
| 79 | Gowork | `tree-sitter-gowork` | `source_file` | `go.work` |
| 80 | Gradle | `tree-sitter-groovy` | `compilation_unit` | `.gradle` |
| 81 | Hack | `tree-sitter-hack` | `source_file` | `.hack`, `.hck`, `.hh` |
| 82 | Hare | `tree-sitter-hare` | `source_file` | `.ha` |
| 83 | HCL (Terraform) | `tree-sitter-hcl` | `config_file` | `.hcl`, `.tf`, `.tfvars` |
| 84 | HLSL | `tree-sitter-hlsl` | `translation_unit` | `.hlsl`, `.fx`, `.fxh`, `.hlsli` |
| 85 | HOCON | `tree-sitter-hocon` | `document` | `.conf`, `.hocon` |
| 86 | Hoon | `tree-sitter-hoon` | `source_file` | `.hoon` |
| 87 | Hy | `tree-sitter-hy` | `program` | `.hy` |
| 88 | INI | `tree-sitter-ini` | `document` | `.ini`, `.cfg`, `.conf` |
| 89 | JQ | `tree-sitter-jq` | `program` | `.jq` |
| 90 | Kconfig | `tree-sitter-kconfig` | `source` | `Kconfig`, `Kconfig.*` |
| 91 | LaTeX | `tree-sitter-latex` | `document` | `.tex`, `.latex`, `.sty`, `.cls` |
| 92 | LLVM | `tree-sitter-llvm` | `module` | `.ll` |
| 93 | M4 | `tree-sitter-m4` | `document` | `.m4` |
| 94 | Markdown | `tree-sitter-markdown` | `document` | `.md`, `.mdx`, `.markdown` |
| 95 | Meson | `tree-sitter-meson` | `build` | `meson.build` |
| 96 | MLIR | `tree-sitter-mlir` | `module` | `.mlir` |
| 97 | Nix | `tree-sitter-nix` | `expression` | `.nix` |
| 98 | Nim | `tree-sitter-nim` | `source_file` | `.nim` |
| 99 | Norg | `tree-sitter-norg` | `document` | `.norg` |
| 100 | Nu (Nushell) | `tree-sitter-nu` | `source_file` | `.nu` |
| 101 | Odin | `tree-sitter-odin` | `source_file` | `.odin` |
| 102 | Org | `tree-sitter-org` | `document` | `.org` |
| 103 | Pascal / Delphi | `tree-sitter-pascal` | `program` | `.pas`, `.pp`, `.dpr` |
| 104 | PioASM | `tree-sitter-pioasm` | `program` | `.pio` |
| 105 | Pony | `tree-sitter-pony` | `source_file` | `.pony` |
| 106 | PowerShell | `tree-sitter-powershell` | `program` | `.ps1`, `.psm1`, `.psd1` |
| 107 | PRQL | `tree-sitter-prql` | `query` | `.prql` |
| 108 | Prolog | `tree-sitter-prolog` | `program` | `.pl`, `.pro` |
| 109 | Protobuf | `tree-sitter-proto` | `source_file` | `.proto` |
| 110 | Purescript | `tree-sitter-purescript` | `module` | `.purs` |
| 111 | QL (CodeQL) | `tree-sitter-ql` | `query` | `.ql`, `.qll` |
| 112 | QML | `tree-sitter-qmljs` | `program` | `.qml` |
| 113 | Racket | `tree-sitter-racket` | `program` | `.rkt` |
| 114 | Rasi | `tree-sitter-rasi` | `document` | `.rasi` |
| 115 | Rego (OPA) | `tree-sitter-rego` | `module` | `.rego` |
| 116 | ReScript | `tree-sitter-rescript` | `source_file` | `.res`, `.resi` |
| 117 | RMarkdown | `tree-sitter-rmarkdown` | `document` | `.Rmd` |
| 118 | RON | `tree-sitter-ron` | `value` | `.ron` |
| 119 | Scheme | `tree-sitter-scheme` | `program` | `.scm`, `.ss` |
| 120 | proto | `tree-sitter-sproto` | `source_file` | `.sproto` |
| 121 | Solidity | `tree-sitter-solidity` | `source_unit` | `.sol` |
| 122 | Squirrel | `tree-sitter-squirrel` | `program` | `.nut`, `.sq` |
| 123 | SuperCollider | `tree-sitter-supercollider` | `program` | `.scd`, `.sc` |
| 124 | SWIFT (Apple) | `tree-sitter-swift` | `source_file` | `.swift` |
| 125 | Tcl | `tree-sitter-tcl` | `script` | `.tcl`, `.tk` |
| 126 | Thrift | `tree-sitter-thrift` | `document` | `.thrift` |
| 127 | Tiger | `tree-sitter-tiger` | `exp` | `.tig` |
| 128 | TLA+ | `tree-sitter-tlaplus` | `module` | `.tla` |
| 129 | Turtle / RDF | `tree-sitter-turtle` | `document` | `.ttl` |
| 130 | Twig | `tree-sitter-twig` | `document` | `.twig` |
| 131 | TypeSpec | `tree-sitter-typespec` | `source_file` | `.tsp` |
| 132 | Typst | `tree-sitter-typst` | `markup` | `.typ` |
| 133 | Unison | `tree-sitter-unison` | `source_file` | `.u` |
| 134 | Uxntal | `tree-sitter-uxntal` | `program` | `.tal` |
| 135 | Vala | `tree-sitter-vala` | `source_file` | `.vala` |
| 136 | Vim Script | `tree-sitter-vim` | `source_file` | `.vim`, `.vimrc` |
| 137 | Vimdoc | `tree-sitter-vimdoc` | `document` | `.txt` |
| 138 | WGSL | `tree-sitter-wgsl` | `translation_unit` | `.wgsl` |
| 139 | Wing | `tree-sitter-wing` | `source` | `.w` |
| 140 | WIT | `tree-sitter-wit` | `source_file` | `.wit` |
| 141 | YANG | `tree-sitter-yang` | `module` | `.yang` |

---

## 2. Architecture: `bugswarm-cpg/src/parsers/`

### 2.1 Directory Layout

```
bugswarm-cpg/src/
├── parsers/
│   ├── mod.rs                          # Parser registry + dispatch
│   ├── common.rs                       # Shared AST traversal, type inference traits
│   ├── identifiers.rs                  # Cross-language identifier resolution
│   ├── python/
│   │   ├── mod.rs                      # Python parser module
│   │   ├── ast.rs                      # AST extraction (~50 lines)
│   │   ├── call_graph.rs               # Call graph extraction (~100 lines)
│   │   ├── cfg.rs                      # Control-flow graph (~200 lines)
│   │   ├── taint.rs                    # Taint source/sink definitions (~50 lines)
│   │   ├── ssa.rs                      # Static Single Assignment (~150 lines)
│   │   ├── types.rs                    # Type inference (~80 lines)
│   │   └── mutation.rs                 # Mutation operators (~60 lines)
│   ├── javascript/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs
│   │   └── mutation.rs
│   ├── typescript/                     # Shares JS AST, adds type-aware analysis
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs                    # Leverages tsc type information
│   │   └── mutation.rs
│   ├── c/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs
│   │   ├── mutation.rs
│   │   └── preprocessor.rs             # C preprocessor directive handling
│   ├── cpp/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs
│   │   ├── mutation.rs
│   │   └── template.rs                 # C++ template instantiation tracking
│   ├── rust/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs                    # Leverages cargo check output
│   │   └── mutation.rs
│   ├── go/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs
│   │   └── mutation.rs
│   ├── java/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs
│   │   └── mutation.rs
│   ├── csharp/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs
│   │   └── mutation.rs
│   ├── ruby/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs
│   │   └── mutation.rs
│   ├── php/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── cfg.rs
│   │   ├── taint.rs
│   │   ├── ssa.rs
│   │   ├── types.rs
│   │   └── mutation.rs
│   ├── kotlin/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs                    # Basic taint only
│   │   └── mutation.rs
│   ├── swift/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── scala/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── dart/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── lua/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── perl/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── r/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── julia/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── haskell/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── elixir/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── erlang/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── clojure/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── ocaml/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── groovy/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   ├── zig/
│   │   ├── mod.rs
│   │   ├── ast.rs
│   │   ├── call_graph.rs
│   │   ├── taint.rs
│   │   └── mutation.rs
│   └── t3_parsers/                     # Tier 3: 80+ languages, AST + call graph only
│       ├── mod.rs
│       ├── bash.rs                     # Shell script parser
│       ├── sql.rs                      # SQL parser
│       ├── solidity.rs                 # Smart contract parser
│       ├── nix.rs                      # Nix expression parser
│       ├── hcl.rs                      # Terraform HCL parser
│       ├── dockerfile.rs               # Containerfile parser
│       ├── verilog.rs                  # HDL parser
│       ├── vhdl.rs                     # HDL parser
│       ├── fortran.rs                  # Scientific computing parser
│       ├── ada.rs                      # Safety-critical systems parser
│       ├── elm.rs                      # Functional frontend parser
│       ├── purescript.rs               # Pure functional parser
│       ├── nim.rs                      # Systems language parser
│       ├── reason.rs                   # OCaml-for-JS parser
│       ├── rescript.rs                 # Fast typed JS parser
│       ├── fsharp.rs                   # .NET functional parser
│       ├── vala.rs                     # GNOME language parser
│       ├── crystal.rs                  # Ruby-like compiled parser
│       ├── odin.rs                     # Data-oriented parser
│       ├── pony.rs                     # Actor model parser
│       ├── chapel.rs                   # Productivity language parser
│       ├── cupl.rs                     # CUPL logic parser
│       ├── systemverilog.rs            # Advanced HDL parser
│       ├── glsl.rs                     # GPU shader parser
│       ├── hlsl.rs                     # DirectX shader parser
│       ├── wgsl.rs                     # WebGPU shader parser
│       ├── powershell.rs               # Shell/script parser
│       ├── fish.rs                     # Friendly shell parser
│       ├── awk.rs                      # Text processing parser
│       ├── make.rs                     # Build system parser
│       ├── cmake.rs                    # Cross-platform build parser
│       ├── meson.rs                    # Modern build parser
│       ├── bazel.rs                    # Google build parser
│       ├── gradle.rs                   # JVM build parser
│       ├── scheme.rs                   # Lisp-dialect parser
│       ├── racket.rs                   # Language-oriented parser
│       ├── common_lisp.rs              # ANSI CL parser
│       ├── emacs_lisp.rs               # Editor extension parser
│       ├── hy.rs                       # Lisp-on-Python parser
│       ├── prolog.rs                   # Logic programming parser
│       ├── tlaplus.rs                  # Formal verification parser
│       ├── alloy.rs                    # Software modeling parser
│       ├── idris.rs                    # Dependent types parser
│       ├── agda.rs                     # Proof assistant parser
│       ├── coq.rs                      # Theorem prover parser
│       ├── isabelle.rs                 # Proof assistant parser
│       ├── forth.rs                    # Stack language parser
│       ├── factor.rs                   # Concatenative parser
│       ├── io.rs                       # Prototype language parser
│       ├── self.rs                     # Prototype OO parser
│       ├── gherkin.rs                  # BDD language parser
│       ├── cue.rs                      # Configuration language parser
│       ├── dhall.rs                    # Safe config parser
│       ├── jsonnet.rs                  # Templating config parser
│       ├── kcl.rs                      # Policy language parser
│       ├── rego.rs                     # Policy language parser
│       ├── sml.rs                      # Standard ML parser
│       ├── tex.rs                      # LaTeX document parser
│       ├── markdown.rs                 # Documentation parser
│       ├── rmarkdown.rs                # R documentation parser
│       ├── org.rs                      # Org mode parser
│       ├── norg.rs                     # Neovim org parser
│       ├── bibtex.rs                   # Bibliography parser
│       ├── dot.rs                      # Graphviz parser
│       ├── tikz.rs                     # Drawing parser
│       ├── gnuplot.rs                  # Plotting parser
│       ├── matlab.rs                   # Math/engineering parser
│       ├── octave.rs                   # Open-source MATLAB parser
│       ├── stata.rs                    # Statistics parser
│       ├── sas.rs                      # Analytics parser
│       ├── unreal_script.rs            # Game engine parser
│       ├── blueprint.rs                # Visual script parser
│       ├── maxscript.rs                # 3D modeling parser
│       └── mel.rs                      # Maya scripting parser
```

---

## 3. Parser Implementation Templates

### 3.1 AST Extraction Pattern (ALL tiers, ~50 lines)

Every language parser implements the `AstExtractor` trait:

```rust
/// Trait that every language parser must implement for AST extraction.
/// Applies to all 140+ languages in the registry.
pub trait AstExtractor: Send + Sync {
    /// Parse source code and return a VNode tree (language-agnostic IR).
    fn parse(
        &self,
        source: &str,
        file_path: &Path,
        config: &ParseConfig,
    ) -> Result<VNodeTree, ParseError>;

    /// Return the tree-sitter Language definition for this parser.
    fn language(&self) -> tree_sitter::Language;

    /// Return the top-level node kind (e.g., "program", "module").
    fn root_node_kind(&self) -> &'static str;

    /// Walk the CST and convert tree-sitter nodes to VNode IR.
    /// This is the core AST extraction — ~50 lines per language.
    fn walk_node(&self, node: tree_sitter::Node, source: &str) -> VNode {
        let kind = node.kind();
        let range = node.range();
        let mut children = Vec::with_capacity(node.child_count() as usize);

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if self.should_descend_into(child) {
                    children.push(self.walk_node(child, source));
                }
            }
        }

        let text = node.utf8_text(source.as_bytes())
            .unwrap_or("")
            .to_string();

        VNode {
            kind: kind.to_string(),
            range: SourceRange {
                start_byte: range.start_byte,
                end_byte: range.end_byte,
                start_point: Point::new(range.start_point.row, range.start_point.column),
                end_point: Point::new(range.end_point.row, range.end_point.column),
            },
            text: if self.should_store_text(kind) { Some(text) } else { None },
            children,
            properties: self.extract_properties(node, source),
        }
    }

    /// Filter out punctuation, delimiters, and whitespace nodes.
    /// Most languages override this to skip: ",", ";", "{", "}", "(", ")", etc.
    fn should_descend_into(&self, node: tree_sitter::Node) -> bool {
        !matches!(node.kind(),
            "," | ";" | "{" | "}" | "(" | ")" | "[" | "]" |
            ":" | "::" | "." | ".." | "..." | "=>" | "->" |
            "comment" | "block_comment" | "line_comment"
        )
    }

    fn should_store_text(&self, _kind: &str) -> bool { false }
    fn extract_properties(&self, _node: tree_sitter::Node, _source: &str) -> HashMap<String, String> {
        HashMap::new()
    }
}
```

### 3.2 Call Graph Extraction Pattern (ALL tiers, ~100 lines)

```rust
/// Builds a directed call graph per compilation unit.
/// Used across all 140+ languages.
pub trait CallGraphExtractor: AstExtractor {
    /// Walk the AST and emit call edges: (caller_site, callee_name, argument_count, type_hint).
    fn extract_call_graph(
        &self,
        vnodes: &VNodeTree,
        symbol_table: &SymbolTable,
    ) -> Result<CallGraph, ParseError> {
        let mut graph = CallGraph::new();
        self.visit_calls(&vnodes.root, None, &mut graph, symbol_table)?;
        self.resolve_symbols(&mut graph, symbol_table)?;
        self.build_transitive_closure(&mut graph);
        Ok(graph)
    }

    /// Language-specific logic: iterate children to find call-like nodes.
    /// Returns call subgraphs per function scope.
    fn visit_calls(
        &self,
        node: &VNode,
        current_function: Option<&str>,
        graph: &mut CallGraph,
        symbols: &SymbolTable,
    ) -> Result<(), ParseError> {
        let func = self.enter_function_scope(node).or(current_function);

        if self.is_call_node(node) {
            let callee = self.extract_callee_name(node);
            let args = self.extract_call_arguments(node);
            let call_site = CallSite {
                id: CallSiteId::new(),
                file: self.file_path().to_path_buf(),
                location: node.range.clone(),
                caller: func.map(|s| s.to_string()),
                callee: callee.clone(),
                callee_original: callee.clone(),
                argument_count: args.len() as u32,
                argument_types: args.iter().map(|a| self.infer_argument_type(a, symbols)).collect(),
                resolved: false,
                is_virtual: self.is_virtual_call(node),
                is_tail_call: self.is_tail_call(node),
                is_closure: self.is_closure_call(node),
                dispatch_targets: self.resolve_dispatch_targets(node, symbols)?,
            };
            graph.add_call_site(call_site);

            for arg in &args {
                if self.is_call_node(arg) {
                    self.visit_calls(arg, func, graph, symbols)?;
                }
            }
        }

        for child in &node.children {
            self.visit_calls(child, func, graph, symbols)?;
        }
        Ok(())
    }

    fn is_call_node(&self, node: &VNode) -> bool;
    fn extract_callee_name(&self, node: &VNode) -> String;
    fn extract_call_arguments(&self, node: &VNode) -> Vec<VNode>;
    fn is_virtual_call(&self, node: &VNode) -> bool { false }
    fn is_tail_call(&self, node: &VNode) -> bool { false }
    fn is_closure_call(&self, node: &VNode) -> bool { false }
    fn resolve_dispatch_targets(&self, _node: &VNode, _symbols: &SymbolTable) -> Result<Vec<String>, ParseError> {
        Ok(vec![])
    }
    fn enter_function_scope<'a>(&self, node: &'a VNode) -> Option<&'a str>;
    fn infer_argument_type(&self, _arg: &VNode, _symbols: &SymbolTable) -> TypeHint {
        TypeHint::Unknown
    }
}
```

### 3.3 CFG Construction Pattern (Tier 1 ONLY, ~200 lines)

Control-flow graph construction is the most complex per-language component, required only for Tier 1 languages. This builds a graph where nodes are basic blocks and edges represent control flow.

```rust
pub trait CfgExtractor: AstExtractor + CallGraphExtractor {
    /// Build the intra-procedural control-flow graph.
    fn construct_cfg(
        &self,
        vnodes: &VNodeTree,
        functions: &FunctionIndex,
    ) -> Result<ControlFlowGraph, ParseError> {
        let mut cfg = ControlFlowGraph::new();

        for func in &functions.list {
            let func_body = self.locate_function_body(vnodes, func)?;
            let mut builder = CfgBuilder::new(func.id, func.name.clone());

            let (entry_id, exit_id) = builder.create_entry_exit_blocks();
            let mut pending = vec![func_body];
            let mut block_map = FxHashMap::default();

            while let Some(stmt) = pending.pop() {
                match self.classify_statement(&stmt) {
                    StatementKind::Sequential => {
                        let bb_id = builder.push_sequential(&stmt);
                        block_map.insert(stmt.id, bb_id);
                        pending.extend(stmt.children.iter().rev());
                    }
                    StatementKind::Conditional { condition, then_branch, else_branch } => {
                        let cond_bb = builder.push_conditional(condition);
                        let then_bb = self.build_branch(&mut builder, then_branch, &mut pending)?;
                        let else_bb = if let Some(eb) = else_branch {
                            self.build_branch(&mut builder, eb, &mut pending)?
                        } else {
                            builder.push_empty_block()
                        };
                        let merge_bb = builder.push_empty_block();
                        builder.add_edge(cond_bb, then_bb, EdgeKind::TrueBranch);
                        builder.add_edge(cond_bb, else_bb, EdgeKind::FalseBranch);
                        builder.add_edge(then_bb, merge_bb, EdgeKind::Fallthrough);
                        builder.add_edge(else_bb, merge_bb, EdgeKind::Fallthrough);
                    }
                    StatementKind::Loop { condition, body, loop_type } => {
                        let header_bb = builder.push_loop_header(condition);
                        let body_bb = self.build_branch(&mut builder, body, &mut pending)?;
                        let exit_bb = builder.push_empty_block();
                        builder.add_edge(header_bb, body_bb, EdgeKind::LoopBody);
                        builder.add_edge(body_bb, header_bb, EdgeKind::LoopBack);
                        builder.add_edge(header_bb, exit_bb, EdgeKind::LoopExit);
                    }
                    StatementKind::Switch { discriminant, cases, default } => {
                        let disc_bb = builder.push_switch_discriminant(discriminant);
                        for (case_val, case_body) in cases {
                            let case_bb = self.build_branch(&mut builder, case_body, &mut pending)?;
                            builder.add_edge(disc_bb, case_bb, EdgeKind::SwitchCase(case_val));
                            builder.add_edge(case_bb, merge_bb, EdgeKind::Fallthrough);
                        }
                        if let Some(def_body) = default {
                            let def_bb = self.build_branch(&mut builder, def_body, &mut pending)?;
                            builder.add_edge(disc_bb, def_bb, EdgeKind::SwitchDefault);
                            builder.add_edge(def_bb, merge_bb, EdgeKind::Fallthrough);
                        }
                    }
                    StatementKind::Jump { target, jump_type } => {
                        let bb_id = builder.push_jump(&stmt, jump_type);
                        match jump_type {
                            JumpKind::Return => { builder.set_exit(bb_id); }
                            JumpKind::Break => { builder.set_break_target(bb_id, target); }
                            JumpKind::Continue => { builder.set_continue_target(bb_id, target); }
                            JumpKind::Goto => { builder.set_goto_target(bb_id, target); }
                            JumpKind::Throw => { builder.set_exception_edge(bb_id); }
                        }
                    }
                    StatementKind::TryCatch { try_body, catch_clauses, finally } => {
                        let try_bb = self.build_branch(&mut builder, try_body, &mut pending)?;
                        for (_, catch_body) in catch_clauses {
                            let catch_bb = self.build_branch(&mut builder, catch_body, &mut pending)?;
                            builder.add_edge(try_bb, catch_bb, EdgeKind::Exception);
                        }
                        if let Some(fin_body) = finally {
                            let fin_bb = self.build_branch(&mut builder, fin_body, &mut pending)?;
                            for catch_bb in &catch_blocks { builder.add_edge(*catch_bb, fin_bb, EdgeKind::Fallthrough); }
                            builder.add_edge(fin_bb, merge_bb, EdgeKind::Fallthrough);
                        }
                    }
                    // Language-specific statement kinds (Python generators, C++ coroutines, Go defer, etc.)
                    StatementKind::LanguageSpecific(ls) => self.handle_language_specific_stmt(
                        &mut builder, ls, &stmt, &mut pending
                    )?,
                }
            }

            builder.link_all_blocks();
            let proc_cfg = builder.finalize();
            cfg.add_procedure(func.id, proc_cfg);
        }

        Ok(cfg)
    }

    /// Core classification: each Tier 1 language implements this to map
    /// VNode kinds to standard statement classes.
    fn classify_statement(&self, node: &VNode) -> StatementKind;

    fn handle_language_specific_stmt(
        &self, builder: &mut CfgBuilder, kind: LanguageSpecificKind, node: &VNode, pending: &mut Vec<VNode>
    ) -> Result<(), ParseError> { Ok(()) }
}
```

### 3.4 Taint Source/Sink Definitions (ALL tiers, ~50 lines)

Each language must define its taint propagation model. Tier 1 and Tier 2 languages provide full definitions; Tier 3 languages provide at least I/O-based sources and sinks.

#### 3.4.1 Python Taint Model

```rust
impl TaintModel for PythonTaintModel {
    fn sources(&self) -> Vec<TaintSource> {
        vec![
            // Network
            TaintSource::new("socket.recv", TaintKind::Network)
                .with_pattern("socket|sock|connection|conn").with_method("recv|recvfrom|recv_into"),
            TaintSource::new("urllib.request.urlopen", TaintKind::Network)
                .with_pattern("urllib.request").with_method("urlopen|urlretrieve"),
            TaintSource::new("requests.get", TaintKind::Network)
                .with_module("requests").with_method("get|post|put|delete|patch|head|options"),
            TaintSource::new("httpx.get", TaintKind::Network)
                .with_module("httpx").with_method("get|post|put|delete|stream"),
            TaintSource::new("aiohttp.get", TaintKind::Network)
                .with_module("aiohttp").with_method("ClientSession.get|post|request"),
            TaintSource::new("flask.request", TaintKind::HTTPRequest)
                .with_pattern("request.args|request.form|request.json|request.data|request.values|request.cookies|request.headers"),
            TaintSource::new("django.request", TaintKind::HTTPRequest)
                .with_pattern("request.GET|request.POST|request.body|request.META"),
            TaintSource::new("fastapi.request", TaintKind::HTTPRequest)
                .with_pattern("Request|request.body|request.query_params"),
            TaintSource::new("starlette.request", TaintKind::HTTPRequest)
                .with_pattern("request.query_params|request.path_params|request.headers"),
            // File I/O
            TaintSource::new("file.read", TaintKind::FileIO)
                .with_method("read|readline|readlines|readinto|readinto1"),
            TaintSource::new("pathlib.read", TaintKind::FileIO)
                .with_method("read_bytes|read_text"),
            TaintSource::new("pickle.load", TaintKind::Deserialization)
                .with_module("pickle|_pickle|cPickle|dill|cloudpickle").with_method("load|loads"),
            TaintSource::new("yaml.load", TaintKind::Deserialization)
                .with_module("yaml").with_method("load|full_load|unsafe_load"),
            TaintSource::new("json.loads", TaintKind::Deserialization)
                .with_module("json|ujson|orjson").with_method("loads"),
            // Environment / CLI
            TaintSource::new("os.environ", TaintKind::Environment)
                .with_pattern("os.environ|os.getenv|os.get_exec_path"),
            TaintSource::new("sys.argv", TaintKind::CLI)
                .with_pattern("sys.argv"),
            TaintSource::new("argparse.args", TaintKind::CLI)
                .with_module("argparse").with_pattern("parse_args|.*parse_args"),
            TaintSource::new("click args", TaintKind::CLI)
                .with_decorator("click.argument|click.option"),
            TaintSource::new("subprocess output", TaintKind::Process)
                .with_module("subprocess").with_method("check_output|getoutput|run"),
            TaintSource::new("multiprocessing.Queue.get", TaintKind::Process)
                .with_method("get|get_nowait"),
            // Database
            TaintSource::new("cursor.fetch", TaintKind::Database)
                .with_method("fetchone|fetchmany|fetchall|execute"),
            TaintSource::new("sqlalchemy results", TaintKind::Database)
                .with_method("fetchall|scalars|all|first|one|one_or_none"),
            TaintSource::new("django.orm.queryset", TaintKind::Database)
                .with_pattern("objects.filter|objects.get|objects.all|objects.values"),
            // Message queue
            TaintSource::new("kafka consumer", TaintKind::MessageQueue)
                .with_module("confluent_kafka|kafka").with_method("Consumer.poll|Consumer.consume"),
            TaintSource::new("rabbitmq consumer", TaintKind::MessageQueue)
                .with_module("pika").with_method("basic_get"),
            TaintSource::new("redis.get", TaintKind::Cache)
                .with_module("redis").with_method("get|hget|hgetall|lrange|smembers|zrange"),
            // WebSockets
            TaintSource::new("websocket.recv", TaintKind::Network)
                .with_method("recv|receive|recv_text|recv_data|recv_json"),
            // Cloud / S3
            TaintSource::new("boto3.get_object", TaintKind::CloudStorage)
                .with_module("boto3|botocore").with_method("get_object|download_file|download_fileobj"),
            // User input in frameworks
            TaintSource::new("pyramid request", TaintKind::HTTPRequest)
                .with_pattern("request.params|request.GET|request.POST|request.json_body"),
            TaintSource::new("bottle request", TaintKind::HTTPRequest)
                .with_pattern("request.query|request.forms|request.body|request.json"),
        ]
    }

    fn sinks(&self) -> Vec<TaintSink> {
        vec![
            // Command injection
            TaintSink::new("os.system", TaintKind::CommandInjection)
                .with_module("os").with_method("system|popen|spawnl|spawnle|spawnlp|spawnlpe|spawnv|spawnve|spawnvp|spawnvpe"),
            TaintSink::new("subprocess calls", TaintKind::CommandInjection)
                .with_module("subprocess").with_method("call|check_call|check_output|run|Popen"),
            TaintSink::new("exec/eval", TaintKind::CodeInjection)
                .with_method("exec|eval|execfile|compile"),
            // SQL injection
            TaintSink::new("raw SQL", TaintKind::SQLInjection)
                .with_method("execute|executemany|executemany|raw"),
            TaintSink::new("SQL f-string", TaintKind::SQLInjection)
                .with_pattern("f\"SELECT|f\"INSERT|f\"UPDATE|f\"DELETE"),
            // XSS
            TaintSink::new("template render", TaintKind::XSS)
                .with_method("render_template|render_template_string|render|safe"),
            TaintSink::new("markup output", TaintKind::XSS)
                .with_module("markupsafe").with_method("Markup|escape"),
            // Path traversal
            TaintSink::new("path traversal", TaintKind::PathTraversal)
                .with_method("open|send_file|send_from_directory"),
            // SSRF
            TaintSink::new("HTTP request", TaintKind::SSRF)
                .with_module("requests|httpx|aiohttp|urllib.request").with_method("get|post|put|delete|request|urlopen"),
            // Deserialization
            TaintSink::new("unsafe deserialization", TaintKind::Deserialization)
                .with_module("pickle|cPickle|dill|cloudpickle").with_method("load|loads|Unpickler"),
            TaintSink::new("yaml unsafe", TaintKind::Deserialization)
                .with_module("yaml").with_method("load|full_load|unsafe_load"),
            TaintSink::new("marshal.loads", TaintKind::Deserialization)
                .with_module("marshal").with_method("loads"),
            // XXE
            TaintSink::new("XML parse", TaintKind::XXE)
                .with_module("xml.etree.ElementTree|lxml.etree|defusedxml.ElementTree").with_method("parse|fromstring|iterparse|XMLParser"),
            // LDAP injection
            TaintSink::new("LDAP query", TaintKind::LDAPInjection)
                .with_module("ldap|ldap3|python-ldap").with_method("search|search_s|search_ext|search_ext_s"),
            // Log injection
            TaintSink::new("log output", TaintKind::LogInjection)
                .with_module("logging").with_method("debug|info|warn|warning|error|critical|fatal|log"),
            // File write (potential compromise)
            TaintSink::new("file write", TaintKind::FileWrite)
                .with_method("write|writelines|dump|dumps|savetxt|to_csv"),
            // HTTP Response
            TaintSink::new("HTTP response body", TaintKind::HTTPResponse)
                .with_method("Response|HttpResponse|StreamingHttpResponse|JsonResponse|HTMLResponse|PlainTextResponse"),
            // GraphQL
            TaintSink::new("GraphQL resolve", TaintKind::Injection)
                .with_module("graphql|strawberry|graphene|ariadne"),
            // Template engine
            TaintSink::new("Jinja2 template", TaintKind::TemplateInjection)
                .with_module("jinja2").with_method("Template|from_string|Environment|render"),
            TaintSink::new("Mako template", TaintKind::TemplateInjection)
                .with_module("mako").with_method("Template|render|render_unicode"),
        ]
    }

    fn sanitizers(&self) -> Vec<Sanitizer> {
        vec![
            Sanitizer::new("html.escape", SanitizeKind::XSS)
                .with_module("html").with_method("escape|unescape"),
            Sanitizer::new("markupsafe.escape", SanitizeKind::XSS)
                .with_module("markupsafe").with_method("escape"),
            Sanitizer::new("shlex.quote", SanitizeKind::CommandInjection)
                .with_module("shlex").with_method("quote|join"),
            Sanitizer::new("re.escape", SanitizeKind::RegexInjection)
                .with_module("re").with_method("escape"),
            Sanitizer::new("url encode", SanitizeKind::URLInjection)
                .with_module("urllib.parse").with_method("quote|quote_plus|urlencode"),
            Sanitizer::new("parameterized SQL", SanitizeKind::SQLInjection)
                .with_module("sqlalchemy").with_pattern("text\\(|bindparam|params"),
            Sanitizer::new("Django ORM filter", SanitizeKind::SQLInjection)
                .with_module("django.db.models").with_method("filter|exclude|get|all|values|annotate"),
            Sanitizer::new("pathlib Path", SanitizeKind::PathTraversal)
                .with_module("pathlib").with_pattern("Path|PurePath"),
            Sanitizer::new("hashlib", SanitizeKind::SensitiveDataExposure)
                .with_module("hashlib").with_method("sha256|sha512|pbkdf2_hmac|scrypt|blake2b"),
            Sanitizer::new("decouple config", SanitizeKind::Environment)
                .with_module("decouple").with_method("config|Csv|Config"),
            Sanitizer::new("pydantic validation", SanitizeKind::InputValidation)
                .with_module("pydantic").with_pattern("BaseModel|validator|field_validator|model_validator|Field"),
            Sanitizer::new("marshmallow validation", SanitizeKind::InputValidation)
                .with_module("marshmallow").with_pattern("Schema|fields|validate|validates_schema|pre_load|post_load"),
            Sanitizer::new("attr.validators", SanitizeKind::InputValidation)
                .with_module("attrs").with_pattern("validators|field|define|attrib"),
            Sanitizer::new("cerberus schema", SanitizeKind::InputValidation)
                .with_module("cerberus").with_method("Validator|validate|normalized"),
            Sanitizer::new("bleach clean", SanitizeKind::XSS)
                .with_module("bleach").with_method("clean|linkify"),
            Sanitizer::new("defusedxml", SanitizeKind::XXE)
                .with_module("defusedxml"),
            Sanitizer::new("bcrypt hash", SanitizeKind::SensitiveDataExposure)
                .with_module("bcrypt").with_method("hashpw|gensalt|kdf"),
            Sanitizer::new("secrets.token", SanitizeKind::WeakRandom)
                .with_module("secrets").with_method("token_hex|token_urlsafe|token_bytes|choice|randbelow"),
        ]
    }

    fn propagation_rules(&self) -> Vec<PropagationRule> {
        vec![
            // String concatenation propagates taint
            PropagationRule::new_taint_prop("+", TaintKind::All),
            PropagationRule::new_taint_prop("f\"...\"", TaintKind::All),
            PropagationRule::new_taint_prop("str.format", TaintKind::All),
            PropagationRule::new_taint_prop("str.join", TaintKind::All),
            PropagationRule::new_taint_prop("str.replace", TaintKind::All),
            PropagationRule::new_taint_prop("str.strip", TaintKind::All),
            PropagationRule::new_taint_prop("dict.get", TaintKind::All),
            PropagationRule::new_taint_prop("list[...]", TaintKind::All),
            PropagationRule::new_taint_prop("json.dumps", TaintKind::All),
            PropagationRule::new_taint_prop("json.loads", TaintKind::All),
            PropagationRule::new_taint_prop("copy.copy", TaintKind::All),
            PropagationRule::new_taint_prop("copy.deepcopy", TaintKind::All),
            PropagationRule::new_taint_prop("pickle.dumps", TaintKind::All),
            PropagationRule::new_taint_prop("* (unpack)", TaintKind::All),
            PropagationRule::new_taint_prop("return statement", TaintKind::All),
            PropagationRule::new_taint_prop("yield statement", TaintKind::All),
            PropagationRule::new_taint_prop("assignment", TaintKind::All),
            PropagationRule::new_taint_prop("list comprehension", TaintKind::All),
            PropagationRule::new_taint_prop("dict comprehension", TaintKind::All),
        ]
    }
}
```

#### 3.4.2 JavaScript Taint Model

```rust
impl TaintModel for JavaScriptTaintModel {
    fn sources(&self) -> Vec<TaintSource> {
        vec![
            // Browser / DOM sources
            TaintSource::new("window.location", TaintKind::HTTPRequest)
                .with_pattern("window.location|location.href|location.search|location.hash|location.pathname"),
            TaintSource::new("document.URL", TaintKind::HTTPRequest)
                .with_pattern("document.URL|document.documentURI|document.baseURI|document.referrer"),
            TaintSource::new("document.cookie", TaintKind::HTTPRequest)
                .with_pattern("document.cookie"),
            TaintSource::new("postMessage", TaintKind::MessagePassing)
                .with_method("addEventListener(\"message\"|addEventListener('message')")
                .with_pattern("onmessage|event.data"),
            TaintSource::new("WebSocket", TaintKind::Network)
                .with_pattern("new WebSocket|ws.send|ws.onmessage"),
            TaintSource::new("fetch API", TaintKind::Network)
                .with_method("fetch|response.json|response.text|response.blob|response.arrayBuffer|response.formData"),
            TaintSource::new("XMLHttpRequest", TaintKind::Network)
                .with_pattern("new XMLHttpRequest|xhr.responseText|xhr.responseXML|xhr.response"),
            TaintSource::new("localStorage/sessionStorage", TaintKind::ClientStorage)
                .with_method("getItem"),
            TaintSource::new("IndexedDB", TaintKind::ClientStorage)
                .with_pattern("indexedDB.open|IDBRequest.result|IDBCursor.value"),
            TaintSource::new("import()", TaintKind::CodeImport)
                .with_pattern("import(.*)"),

            // Node.js sources
            TaintSource::new("process.argv", TaintKind::CLI)
                .with_pattern("process.argv|process.env"),
            TaintSource::new("process.stdin", TaintKind::StdIO)
                .with_pattern("process.stdin.read|process.stdin.on"),
            TaintSource::new("fs.readFile", TaintKind::FileIO)
                .with_module("fs").with_method("readFile|readFileSync|createReadStream"),
            TaintSource::new("http.IncomingMessage", TaintKind::HTTPRequest)
                .with_pattern("http.createServer|req.query|req.params|req.body|req.headers"),
            TaintSource::new("express req", TaintKind::HTTPRequest)
                .with_pattern("req.query|req.params|req.body|req.headers|req.cookies|req.files"),
            TaintSource::new("url.parse", TaintKind::URL)
                .with_module("url").with_method("parse|format|resolve"),
            TaintSource::new("querystring.parse", TaintKind::HTTPRequest)
                .with_module("querystring").with_method("parse|decode"),
            TaintSource::new("child_process.execSync output", TaintKind::Process)
                .with_method("execSync|execFileSync|spawnSync"),
            TaintSource::new("crypto.randomBytes", TaintKind::WeakRandom)
                .with_method("randomBytes|pseudoRandomBytes"),
            TaintSource::new("prototype chain", TaintKind::PrototypePollution)
                .with_pattern("__proto__|constructor.prototype|object.__proto__"),
            TaintSource::new("JSON.parse", TaintKind::Deserialization)
                .with_method("parse"),
            TaintSource::new("require/import dynamic", TaintKind::CodeInjection)
                .with_pattern("require(variable)|import(variable)"),
            TaintSource::new("EventEmitter", TaintKind::MessagePassing)
                .with_method("emit|on"),
            TaintSource::new("Web Worker", TaintKind::MessagePassing)
                .with_pattern("new Worker|worker.postMessage|worker.onmessage"),
            TaintSource::new("service worker message", TaintKind::MessagePassing)
                .with_pattern("clients.matchAll|self.addEventListener('message'"),
            TaintSource::new("BroadcastChannel", TaintKind::MessagePassing)
                .with_pattern("new BroadcastChannel|channel.onmessage|channel.postMessage"),
            TaintSource::new("SharedArrayBuffer", TaintKind::Concurrency)
                .with_pattern("new SharedArrayBuffer|Atomics.load"),
        ]
    }

    fn sinks(&self) -> Vec<TaintSink> {
        vec![
            // XSS (DOM-based)
            TaintSink::new("innerHTML", TaintKind::XSS)
                .with_pattern(".innerHTML =|.outerHTML ="),
            TaintSink::new("document.write", TaintKind::XSS)
                .with_pattern("document.write|document.writeln"),
            TaintSink::new("eval", TaintKind::CodeInjection)
                .with_pattern("eval(|Function(|setTimeout(|setInterval(|setImmediate("),
            TaintSink::new("React dangerouslySetInnerHTML", TaintKind::XSS)
                .with_pattern("dangerouslySetInnerHTML"),
            TaintSink::new("jQuery html()", TaintKind::XSS)
                .with_pattern(".html(|.append(|.prepend(|.before(|.after(|.wrap("),
            TaintSink::new("insertAdjacentHTML", TaintKind::XSS)
                .with_method("insertAdjacentHTML"),
            TaintSink::new("location assignment", TaintKind::OpenRedirect)
                .with_pattern("location.href =|location.assign(|location.replace(|location ="),
            TaintSink::new("open()", TaintKind::OpenRedirect)
                .with_pattern("window.open("),
            TaintSink::new("postMessage", TaintKind::SensitiveDataLeak)
                .with_method("postMessage"),

            // Node.js sinks
            TaintSink::new("exec/execSync", TaintKind::CommandInjection)
                .with_module("child_process").with_method("exec|execSync|spawn|spawnSync|fork|execFile|execFileSync"),
            TaintSink::new("eval (Node)", TaintKind::CodeInjection)
                .with_pattern("eval(|new Function(|vm.runInContext|vm.runInNewContext|vm.runInThisContext|vm.compileFunction"),
            TaintSink::new("require variable", TaintKind::CodeInjection)
                .with_pattern("require(variable)|import(variable)"),
            TaintSink::new("fs.writeFile", TaintKind::FileWrite)
                .with_method("writeFile|writeFileSync|appendFile|appendFileSync|createWriteStream"),
            TaintSink::new("SQL query", TaintKind::SQLInjection)
                .with_pattern(".query(|.execute(|.run(|.all(|.get("),
            TaintSink::new("MongoDB query", TaintKind::NoSQLInjection)
                .with_method("find|findOne|findById|update|updateOne|updateMany|aggregate")
                .with_pattern("{$where|{$expr"),
            TaintSink::new("LDAP search", TaintKind::LDAPInjection)
                .with_pattern("ldap.search|ldap.searchAsync|client.search"),
            TaintSink::new("XML parse", TaintKind::XXE)
                .with_pattern("libxmljs.parseXml|xml2js.parse|DOMParser.parse|xml2js.parseString"),
            TaintSink::new("res.send/end", TaintKind::HTTPResponse)
                .with_pattern("res.send(|res.end(|res.json(|res.render(|res.sendFile("),
            TaintSink::new("res.setHeader", TaintKind::HeaderInjection)
                .with_method("setHeader|writeHead|header"),
            TaintSink::new("os command in require('child_process')", TaintKind::CommandInjection)
                .with_pattern("execSync|spawnSync|execFileSync"),
            TaintSink::new("prototype assignment", TaintKind::PrototypePollution)
                .with_pattern("__proto__|constructor.prototype"),
            TaintSink::new("JSONP callback", TaintKind::XSS)
                .with_pattern("callback|jsonp"),
            TaintSink::new("template literal injection", TaintKind::TemplateInjection)
                .with_pattern("`.*${.*}.*`"),
            TaintSink::new("server-side includes", TaintKind::Injection)
                .with_pattern("<!--#include|<!--#exec|<!--#echo"),
            TaintSink::new("GraphQL resolution", TaintKind::Injection)
                .with_pattern("resolver|resolve|graphql("),
            TaintSink::new("serialize-javascript", TaintKind::Deserialization)
                .with_module("serialize-javascript").with_method("serialize|deserialize"),
            TaintSink::new("VM2 sandbox escape", TaintKind::CodeInjection)
                .with_module("vm2").with_method("VM|NodeVM|VMScript"),
        ]
    }

    fn sanitizers(&self) -> Vec<Sanitizer> {
        vec![
            Sanitizer::new("DOMPurify.sanitize", SanitizeKind::XSS)
                .with_module("DOMPurify|dompurify").with_method("sanitize"),
            Sanitizer::new("escape-html", SanitizeKind::XSS)
                .with_module("escape-html").with_method("escape"),
            Sanitizer::new("helmet", SanitizeKind::XSS)
                .with_module("helmet").with_method("contentSecurityPolicy|xssFilter|noSniff"),
            Sanitizer::new("express-validator", SanitizeKind::InputValidation)
                .with_module("express-validator").with_method("body|param|query|check|sanitize"),
            Sanitizer::new("joi validation", SanitizeKind::InputValidation)
                .with_module("joi|@hapi/joi").with_method("validate|object|string|number|boolean|array"),
            Sanitizer::new("zod validation", SanitizeKind::InputValidation)
                .with_module("zod").with_method("parse|safeParse|string|number|object|array|enum"),
            Sanitizer::new("sanitize-html", SanitizeKind::XSS)
                .with_module("sanitize-html").with_method("sanitize|defaults"),
            Sanitizer::new("mysql.escape", SanitizeKind::SQLInjection)
                .with_module("mysql|mysql2").with_method("escape|escapeId|format"),
            Sanitizer::new("pg-format", SanitizeKind::SQLInjection)
                .with_module("pg-format").with_method("format"),
            Sanitizer::new("parameterized query", SanitizeKind::SQLInjection)
                .with_pattern("$1|$2|$3|?")
                .with_context("SQL template literal"),
            Sanitizer::new("sequelize escape", SanitizeKind::SQLInjection)
                .with_module("sequelize").with_method("escape|literal"),
            Sanitizer::new("prisma query", SanitizeKind::SQLInjection)
                .with_module("@prisma/client"),
            Sanitizer::new("path.join/resolve", SanitizeKind::PathTraversal)
                .with_module("path").with_method("join|resolve|normalize"),
            Sanitizer::new("bcrypt", SanitizeKind::SensitiveDataLeak)
                .with_module("bcrypt|bcryptjs").with_method("hash|hashSync|compare|compareSync"),
            Sanitizer::new("argon2", SanitizeKind::SensitiveDataLeak)
                .with_module("argon2").with_method("hash|verify"),
            Sanitizer::new("crypto.createHmac", SanitizeKind::WeakRandom)
                .with_module("crypto").with_method("createHmac|createHash|randomBytes|randomFill"),
            Sanitizer::new("validator.js", SanitizeKind::InputValidation)
                .with_module("validator").with_method("escape|normalizeEmail|trim|stripLow|isEmail|isURL|isAlphanumeric|isLength"),
            Sanitizer::new("ajv schema validation", SanitizeKind::InputValidation)
                .with_module("ajv").with_method("validate|compile|addSchema"),
            Sanitizer::new("JSON.stringify", SanitizeKind::XSS)
                .with_method("stringify"),
            Sanitizer::new("encodeURIComponent", SanitizeKind::URLInjection)
                .with_method("encodeURIComponent|encodeURI"),
            Sanitizer::new("crypto.subtle.encrypt", SanitizeKind::SensitiveDataLeak)
                .with_pattern("crypto.subtle.encrypt|window.crypto.subtle"),
            Sanitizer::new("OWASP ESAPI", SanitizeKind::Injection)
                .with_module("owasp-esapi-js|node-esapi"),
            Sanitizer::new("RateLimiter", SanitizeKind::DoS)
                .with_module("express-rate-limit|rate-limiter-flexible|bottleneck"),
            Sanitizer::new("CSRF token check", SanitizeKind::CSRF)
                .with_module("csurf|cookie-parser|csrf"),
        ]
    }

    fn propagation_rules(&self) -> Vec<PropagationRule> {
        vec![
            PropagationRule::new_taint_prop("+", TaintKind::All),
            PropagationRule::new_taint_prop("`${}`", TaintKind::All),
            PropagationRule::new_taint_prop("str.concat", TaintKind::All),
            PropagationRule::new_taint_prop("str.replace", TaintKind::All),
            PropagationRule::new_taint_prop("str.slice", TaintKind::All),
            PropagationRule::new_taint_prop("str.substring", TaintKind::All),
            PropagationRule::new_taint_prop("Array.join", TaintKind::All),
            PropagationRule::new_taint_prop("Array.map", TaintKind::All),
            PropagationRule::new_taint_prop("Array.filter", TaintKind::All),
            PropagationRule::new_taint_prop("Object.assign", TaintKind::All),
            PropagationRule::new_taint_prop("spread operator", TaintKind::All),
            PropagationRule::new_taint_prop("destructuring", TaintKind::All),
            PropagationRule::new_taint_prop("return statement", TaintKind::All),
            PropagationRule::new_taint_prop("console.log", TaintKind::LogInjection),
            PropagationRule::new_taint_prop("JSON.stringify", TaintKind::All),
            PropagationRule::new_taint_prop("JSON.parse", TaintKind::All),
            PropagationRule::new_taint_prop("Promise.resolve", TaintKind::All),
            PropagationRule::new_taint_prop("Promise.then", TaintKind::All),
            PropagationRule::new_taint_prop("async/await", TaintKind::All),
            PropagationRule::new_taint_prop("event listener callback", TaintKind::All),
            PropagationRule::new_taint_prop("setState (React)", TaintKind::All),
            PropagationRule::new_taint_prop("useState (React)", TaintKind::All),
            PropagationRule::new_taint_prop("Redux dispatch", TaintKind::All),
            PropagationRule::new_taint_prop("Vuex commit", TaintKind::All),
            PropagationRule::new_taint_prop("Svelte store.set", TaintKind::All),
        ]
    }
}
```

---

## 4. Language-Specific Mutation Operators for Mutation Testing

Mutation testing is supported for all Tier 1 languages and key Tier 2 languages. Each language defines operators in `mutation.rs`.

### 4.1 Universal Mutation Operators (applied to ALL languages)

```rust
/// Universal mutation operators that apply to any language with
/// recognizable patterns (via tree-sitter generic node matching).
pub struct UniversalMutationOperators;

impl UniversalMutationOperators {
    pub fn all() -> Vec<MutationOperator> {
        vec![
            // Arithmetic operators
            MutationOperator::new("AOR", "Arithmetic Operator Replacement")
                .replace("+", "-").replace("-", "+").replace("*", "/").replace("/", "*").replace("%", "*"),
            MutationOperator::new("AOD", "Arithmetic Operator Deletion")
                .delete("++").delete("--").delete("+=").delete("-=").delete("*=").delete("/="),
            // Relational operators
            MutationOperator::new("ROR", "Relational Operator Replacement")
                .replace(">", ">=").replace(">=", ">").replace("<", "<=").replace("<=", "<")
                .replace("==", "!=").replace("!=", "=="),
            // Logical operators
            MutationOperator::new("LCR", "Logical Connector Replacement")
                .replace("&&", "||").replace("||", "&&").replace("!", ""),
            // Conditional boundary
            MutationOperator::new("CBR", "Conditional Boundary Replacement")
                .replace(">", ">=").replace("<", "<=").replace("<=", "<").replace(">=", ">"),
            // Statement deletion
            MutationOperator::new("SSD", "Statement Deletion")
                .delete_statement("*"),
            // Return value
            MutationOperator::new("RVR", "Return Value Replacement")
                .replace_return("null").replace_return("0").replace_return("\"\"").replace_return("[]"),
            // Variable replacement
            MutationOperator::new("VVR", "Variable Value Replacement")
                .replace_variable("*", "similar_type"),
            // Negate conditionals
            MutationOperator::new("NCC", "Negate Conditional")
                .negate_if().negate_while().negate_guard(),
            // Swap function arguments
            MutationOperator::new("SFA", "Swap Function Arguments")
                .swap_args_with(lambda args: args.len() == 2),
            // Constant replacement  
            MutationOperator::new("CRP", "Constant Replacement")
                .mutate_constants::<i64>(|c| c + 1).mutate_constants::<f64>(|c| c + 1.0)
                .mutate_constants::<&str>(|c| if c == "true" { "false" } else { "true" }),
        ]
    }
}
```

### 4.2 Language-Specific Operators

```rust
// Python-specific mutation operators
pub struct PythonMutationOperators;
impl PythonMutationOperators {
    pub fn additional() -> Vec<MutationOperator> {
        vec![
            MutationOperator::new("PYM-LST", "List comprehension to for loop")
                .transform("[x for x in y]", "for x in y:\n    ..."),
            MutationOperator::new("PYM-GEN", "Generator to list")
                .transform("(x for x in y)", "[x for x in y]"),
            MutationOperator::new("PYM-DEC", "Remove decorator")
                .strip_attribute("@route|@login_required|@transaction.atomic"),
            MutationOperator::new("PYM-CMP", "Context manager removal")
                .remove_block("with"),
            MutationOperator::new("PYM-EXC", "Exception handler removal")
                .remove_block("try").remove_block("except"),
            MutationOperator::new("PYM-LAM", "Lambda to def")
                .transform("lambda", "def"),
            MutationOperator::new("PYM-TCI", "Ternary conditional inversion")
                .invert_ternary(),
            MutationOperator::new("PYM-ABS", "abs() removal")
                .strip_function("abs"),
            MutationOperator::new("PYM-SLC", "Slice mutation")
                .replace("[:]", "[:-1]").replace("[1:]", "[:]").replace("[::-1]", "[:]"),
            MutationOperator::new("PYM-DTL", "DTL autoescape off")
                .add_parameter("render", "autoescape=False"),
            MutationOperator::new("PYM-HSH", "Hash algorithm change")
                .replace("sha256", "md5").replace("sha512", "sha1"),
            MutationOperator::new("PYM-SSL", "SSL verification off")
                .add_parameter("requests.get", "verify=False")
                .add_parameter("httpx.get", "verify=False"),
            MutationOperator::new("PYM-CSR", "CSRF exempt")
                .add_decorator("@csrf_exempt"),
            MutationOperator::new("PYM-WKN", "Weak key mutation")
                .replace("rsa.generate(2048)", "rsa.generate(512)")
                .replace("default_backend()", ""),
            MutationOperator::new("PYM-TME", "Timeout removal")
                .delete_parameter("timeout"),
            MutationOperator::new("PYM-UNP", "Unpickle permission")
                .replace("yaml.safe_load", "yaml.load")
                .replace("json.loads", "pickle.loads"),
            MutationOperator::new("PYM-DBG", "Debug flag on")
                .add_parameter("app.run", "debug=True"),
            MutationOperator::new("PYM-CRS", "CORS origin wildcard")
                .replace("origins=[...]", "origins=['*']"),
            MutationOperator::new("PYM-HRD", "Hardcoded secret")
                .replace_variable("SECRET_KEY", "\"insecure-dev-key\""),
            MutationOperator::new("PYM-KWD", "Keyword argument removal")
                .delete_keyword("safe").delete_keyword("autoescape").delete_keyword("escape"),
        ]
    }
}

// C/C++ specific mutation operators
pub struct CMutationOperators;
impl CMutationOperators {
    pub fn additional() -> Vec<MutationOperator> {
        vec![
            MutationOperator::new("C-MEM", "malloc -> stack allocation")
                .replace("malloc(size)", "alloca(size)"),
            MutationOperator::new("C-FRE", "Remove free()")
                .delete_statement("free"),
            MutationOperator::new("C-NUL", "Null pointer assignment")
                .assign_null_preserved(),
            MutationOperator::new("C-OVF", "Integer overflow trigger")
                .replace("+ 1", "+ INT_MAX").replace("* 2", "* INT_MAX"),
            MutationOperator::new("C-BUF", "Buffer size reduction")
                .replace("sizeof(buf)", "sizeof(buf) / 2"),
            MutationOperator::new("C-SNG", "Signed/unsigned mutation")
                .replace("int", "unsigned int").replace("size_t", "int"),
            MutationOperator::new("C-PTR", "Pointer arithmetic mutation")
                .replace("*ptr++", "*--ptr").replace("ptr + 1", "ptr - 1"),
            MutationOperator::new("C-CKP", "Check removal (bounds/format)")
                .delete_conditional("if (n < size)").delete_conditional("if (p != NULL)"),
            MutationOperator::new("C-FRM", "Format string mutation")
                .replace("printf(\"%s\", s)", "printf(s)"),
            MutationOperator::new("C-SPN", "sprintf -> strcpy (bounds loss)")
                .replace("snprintf(buf, size, ....)", "sprintf(buf, ...)"),
            MutationOperator::new("C-MKY", "memcpy/memset size mutation")
                .replace("memcpy(d, s, n)", "memcpy(d, s, n * 2)"),
            MutationOperator::new("C-ASB", "assert to abort mutation")
                .replace("assert(cond)", "/* assert removed */"),
            MutationOperator::new("C-RCF", "recursion guard removal")
                .delete_conditional("if (depth > MAX_RECURSION)"),
            MutationOperator::new("C-UB", "Undefined behavior injection")
                .replace("int x;", "int x; /* uninitialized */"),
            MutationOperator::new("C-RCF2", "Race condition: lock removal")
                .delete_statement("pthread_mutex_lock").delete_statement("pthread_rwlock_rdlock"),
            MutationOperator::new("C-TOCTOU", "TOCTOU: stat then use")
                .reorder_statements_between("stat(", "open("),
            MutationOperator::new("C-LBC", "Loop bound change")
                .replace("i < n", "i <= n").replace("i >= 0", "i > 0"),
        ]
    }
}

// Go-specific mutation operators
pub struct GoMutationOperators;
impl GoMutationOperators {
    pub fn additional() -> Vec<MutationOperator> {
        vec![
            MutationOperator::new("GO-ERR", "Error check removal")
                .delete_conditional("if err != nil"),
            MutationOperator::new("GO-DEF", "Defer removal")
                .delete_statement("defer"),
            MutationOperator::new("GO-GOR", "Goroutine addition")
                .add_prefix("go "),
            MutationOperator::new("GO-CHN", "Channel direction mutation")
                .replace("<-", "->"),
            MutationOperator::new("GO-SEL", "Select default addition")
                .add_default_to_select(),
            MutationOperator::new("GO-COP", "Copy size mutation")
                .replace("copy(dst, src)", "copy(dst[:len(dst)-1], src)"),
            MutationOperator::new("GO-UNS", "Unsafe pointer mutation")
                .replace("*int", "unsafe.Pointer"),
            MutationOperator::new("GO-SLI", "Slice bounds mutation")
                .replace("s[:]", "s[1:]").replace("s[len(s)-1:]", "s[:]"),
            MutationOperator::new("GO-INT", "Interface mutation")
                .replace("error", "interface{}"),
            MutationOperator::new("GO-CTX", "Context timeout removal")
                .delete_parameter("ctx").delete_parameter("context.Background()"),
            MutationOperator::new("GO-SQL", "SQL query string mutation")
                .replace("db.Query(\"SELECT ...\", args...)", "db.Query(\"SELECT ...\"+\" OR 1=1\", args...)"),
            MutationOperator::new("GO-TLS", "TLS config weakening")
                .replace("tls.Config{InsecureSkipVerify: false}", "tls.Config{InsecureSkipVerify: true}"),
            MutationOperator::new("GO-RCE", "Race condition enablement")
                .delete_atomic().add_shared_state_access(),
            MutationOperator::new("GO-PAN", "Panic recovery removal")
                .delete_block("recover()"),
            MutationOperator::new("GO-HRD", "Hardcoded token")
                .replace("os.Getenv(\"API_KEY\")", "\"hardcoded-token\""),
        ]
    }
}

// Java-specific mutation operators
pub struct JavaMutationOperators;
impl JavaMutationOperators {
    pub fn additional() -> Vec<MutationOperator> {
        vec![
            MutationOperator::new("JV-EQU", "equals() -> == mutation")
                .replace(".equals(", " == "),
            MutationOperator::new("JV-HSH", "hashCode() removal")
                .delete_method("hashCode"),
            MutationOperator::new("JV-SER", "serialVersionUID removal")
                .delete_field("serialVersionUID"),
            MutationOperator::new("JV-TRY", "try-with-resources -> try")
                .remove_resource_from_try(),
            MutationOperator::new("JV-LMD", "Lambda to anonymous class")
                .transform_lambda_to_anonymous(),
            MutationOperator::new("JV-STM", "Stream to for-loop")
                .transform_stream_to_loop(),
            MutationOperator::new("JV-NUL", "Null check removal")
                .delete_conditional("if (x != null)").delete_conditional("if (Objects.nonNull"),
            MutationOperator::new("JV-THR", "throws clause removal")
                .delete_keyword("throws"),
            MutationOperator::new("JV-SYN", "Synchronized removal")
                .delete_keyword("synchronized"),
            MutationOperator::new("JV-VOL", "Volatile removal")
                .delete_keyword("volatile"),
            MutationOperator::new("JV-DES", "Deserialization: readObject unrestricted")
                .delete_conditional("validateObject"),
            MutationOperator::new("JV-SQL", "Statement -> PreparedStatement mutation")
                .replace("createStatement()", "PreparedStatement"), // removes parameterization
            MutationOperator::new("JV-XML", "XXE: disable secure processing")
                .add_property("XMLInputFactory", "javax.xml.stream.isSupportingExternalEntities", "true"),
            MutationOperator::new("JV-LOG", "Log forging injection")
                .replace("log.info(msg)", "log.info(\"\\n\\nFATAL: user logged out\" + msg)"),
            MutationOperator::new("JV-EL", "EL injection")
                .replace("${...}", "${7*7}"),
            MutationOperator::new("JV-RFL", "Reflective access")
                .add_import("java.lang.reflect").insert_code("Method m = clazz.getDeclaredMethod(name); m.setAccessible(true);"),
            MutationOperator::new("JV-XSS", "XSS: unescaped output")
                .replace("c:out value=\"${val}\"", "${val}"),
            MutationOperator::new("JV-PER", "Permission check removal")
                .delete_statement("checkPermission"),
            MutationOperator::new("JV-CRY", "Weak crypto")
                .replace("SHA-256", "MD5").replace("AES/GCM", "DES"),
            MutationOperator::new("JV-HST", "HostnameVerifier weakening")
                .replace("new DefaultHostnameVerifier()", "ALLOW_ALL_HOSTNAME_VERIFIER"),
        ]
    }
}
```

---

## 5. Language-Specific Sanitizer Integration

Each language tier integrates with its native sanitizer/runtime introspection tools:

### 5.1 Tier 1 Sanitizer Map

| Language | Sanitizer / Tool | Integration Method | Detects |
|----------|-----------------|-------------------|---------|
| **C** | AddressSanitizer (ASAN) | Compile with `-fsanitize=address -g` | Heap/stack buffer overflow, use-after-free, double-free, memory leak |
| **C** | UndefinedBehaviorSanitizer (UBSAN) | Compile with `-fsanitize=undefined` | Integer overflow, misaligned access, null deref, shift out-of-bounds |
| **C** | MemorySanitizer (MSAN) | Compile with `-fsanitize=memory` | Uninitialized memory read |
| **C** | ThreadSanitizer (TSAN) | Compile with `-fsanitize=thread` | Data races, deadlocks |
| **C** | LeakSanitizer (LSAN) | `-fsanitize=leak` | Memory leaks |
| **C++** | AddressSanitizer + UBSAN + TSAN | Same flags as C plus `-fsanitize=vptr,alignment` | C++ specific: vptr corruption, misaligned member access |
| **C++** | HWASAN (Hardware ASAN) | ARM64: `-fsanitize=hwaddress` | Tag-based memory error detection |
| **Python** | hypothesis | `from hypothesis import given, strategies` | Property-based test case generation |
| **Python** | afl-utils + python-afl | Fuzzing harness | Crashing inputs |
| **Python** | bandit | Static security analysis integration | SQLi, XSS, command injection |
| **Python** | pytype / mypy | Type checking with strict mode | Type-related bugs |
| **JavaScript** | `node --inspect` | Runtime introspection | Memory snapshots, CPU profiles |
| **JavaScript** | `node --experimental-fuzzing` | Native fuzzing harness | Edge cases in v8 |
| **JavaScript** | `node --experimental-policy` | Policy enforcement | Unauthorized module loads |
| **JavaScript** | `node --stack-trace-limit=1000` | Deep stack inspection | Recursion issues |
| **TypeScript** | `tsc --strict` + `ts-prune` | Full strict mode checking | Dead code, type holes |
| **TypeScript** | zod / io-ts runtime | Schema-based runtime validation | Type/contract violations |
| **Rust** | Miri | `cargo miri test` | UB in unsafe code |
| **Rust** | `cargo-geiger` | Unsafe code audit | Unsafe block identification |
| **Rust** | `cargo-fuzz` (libfuzzer) | `cargo fuzz run target` | Crash reproduction |
| **Rust** | `cargo-audit` | `cargo audit` | Known vulnerability dependencies |
| **Rust** | Loom | `#[test] fn with loom` | Concurrency bug detection |
| **Rust** | Kani Rust Verifier | `cargo kani` | Formal verification of properties |
| **Go** | `go build -race` | Race detector | Data races |
| **Go** | `go test -fuzz=FuzzX` | Native fuzzing (1.18+) | Crashing inputs |
| **Go** | `go vet` + `staticcheck` | Linting + static analysis | Common mistakes |
| **Go** | `delve` debugger | Runtime introspection | Stack, goroutine, memory inspection |
| **Go** | `gosec` | Security analysis | SQLi, command injection, TLS issues |
| **Java** | SpotBugs / FindBugs | Bytecode analysis | 400+ bug patterns |
| **Java** | Checker Framework | `-processor org.checkerframework...` | Nullness, tainting, regex, format |
| **Java** | jCSE (Java Concolic Engine) | Symbolic execution | Path exploration |
| **Java** | ASM + ByteBuddy | Runtime code inspection | Dynamic code generation |
| **Java** | JMH (Microbenchmark harness) | Performance regression | Timing bugs |
| **C#** | Roslyn Analyzers | `.editorconfig` + NuGet packages | Security and quality rules |
| **C#** | `dotnet fuzz` (SharpFuzz) | Fuzzing harness | Crash reproduction |
| **C#** | `dotnet trace` + `dotnet-gcdump` | Runtime diagnostics | Memory, GC issues |
| **C#** | BenchmarkDotNet | Performance testing | Regression detection |
| **Ruby** | `bundle exec rspec --bisect` | Shrinking test case | Minimal reproduction |
| **Ruby** | `bundle exec brakeman` | Security scanner | Rails-specific vulns |
| **Ruby** | `ObjectSpace.trace_object_allocations` | Runtime introspection | Memory leaks |
| **Ruby** | `ruby -W:deprecated` + `ruby -W:performance` | Warnings | Deprecation and perf issues |
| **PHP** | `php -d memory_limit=-1 -d xdebug.mode=coverage` | Coverage + debugging | Untested paths |
| **PHP** | PHPStan / Psalm | Static analysis levels | Type errors, dead code |
| **PHP** | `phpdbg -qrr` | Debugger integration | Step-through analysis |
| **PHP** | RIPS / Progpilot | Security taint analysis | Injection vulnerabilities |

### 5.2 Tier 2 Sanitizer Map

| Language | Sanitizer / Tool | Integration Method |
|----------|-----------------|-------------------|
| Kotlin | detekt + `kotlinx-coroutines-debug` | Static analysis + coroutine debugger |
| Swift | `swift test --sanitize=address` + TSan | Swift-native ASAN/TSAN |
| Scala | WartRemover + ScalaFix | Compiler plugin linting |
| Dart | `dart analyze` + `dart test --pause-after-load` | Static + debug |
| Lua | `luac -p -l` + LuaInspect | Bytecode verification |
| Perl | `perl -c` + `perl -Mstrict -Mwarnings` + Perl::Critic | Compile check + lint |
| R | `R -d valgrind` + `R CMD check --as-cran` | Memory check + CRAN compliance |
| Julia | `julia --check-bounds=yes` + `JET.jl` | Bounds checking + type analysis |
| Haskell | `hlint` + `weeder` + `liquidhaskell` | Lint + dead code + refinement types |
| Elixir | `mix dialyzer` + `mix credo` | Type checking + lint |
| Erlang | `dialyzer` + `eqWAlizer` + `concuerror` | Type + concurrency |
| Clojure | `clj-kondo` + `eastwood` + `core.typed` | Lint + static checks |
| OCaml | `opam install bisect_ppx` + `crowbar` | Coverage + fuzzing |
| Groovy | CodeNarc + `groovyc --configscript` | Static analysis |
| Zig | `zig build -fsanitize=undefined` + `zig test` | UBSAN integration |

---

## 6. Auto-Detection System: `bugswarm-cpg index`

The `bugswarm-cpg index` command recursively scans a directory and auto-detects the language of each file.

```rust
/// Auto-detection priority chain:
/// 1. File extension mapping (most reliable)
/// 2. Shebang line (for scripts)
/// 3. Package manager files (for project-level detection)
/// 4. Content heuristics (deep learning fallback)
impl LanguageDetector {
    pub fn detect(file_path: &Path, content: Option<&str>) -> Result<LanguageId, DetectionError> {
        // Priority 1: Extension mapping
        if let Some(ext) = file_path.extension() {
            if let Some(lang) = EXTENSION_MAP.get(ext.to_str().unwrap_or("")) {
                return Ok(*lang);
            }
        }

        // Handle special filenames without extensions
        if let Some(name) = file_path.file_name() {
            if let Some(name_str) = name.to_str() {
                if let Some(lang) = SPECIAL_FILENAME_MAP.get(name_str) {
                    return Ok(*lang);
                }
            }
        }

        // Priority 2: Shebang line
        let content_ref = content.unwrap_or("");
        if let Some(first_line) = content_ref.lines().next() {
            if first_line.starts_with("#!") {
                if let Some(lang) = SHEBANG_MAP.iter().find_map(|(pattern, lang)| {
                    first_line.contains(pattern).then_some(*lang)
                }) {
                    return Ok(lang);
                }
            }
        }

        // Priority 3: Package manager / config file siblings
        let parent = file_path.parent().unwrap_or(Path::new("."));
        let siblings = std::fs::read_dir(parent)?;
        for entry in siblings.flatten() {
            let name = entry.file_name();
            if let Some(name_str) = name.to_str() {
                for (config_file, lang) in PACKAGE_FILE_MAP.iter() {
                    if name_str == *config_file {
                        return Ok(*lang);
                    }
                }
            }
        }

        // Priority 4: Content-based heuristic (bayesian or ML model)
        if let Some(c) = content {
            if let Some(lang) = Self::classify_by_content(c) {
                return Ok(lang);
            }
        }

        Err(DetectionError::UnknownLanguage(file_path.to_path_buf()))
    }
}

/// Extension map: 140+ mappings
static EXTENSION_MAP: Lazy<FxHashMap<&str, LanguageId>> = Lazy::new(|| {
    let mut m = FxHashMap::default();
    // Tier 1
    m.insert("py", Python); m.insert("pyi", Python); m.insert("pyx", Python); m.insert("pxd", Python);
    m.insert("js", JavaScript); m.insert("mjs", JavaScript); m.insert("cjs", JavaScript); m.insert("jsx", JavaScript);
    m.insert("ts", TypeScript); m.insert("tsx", TypeScript); m.insert("mts", TypeScript); m.insert("cts", TypeScript);
    m.insert("c", C); m.insert("h", C);
    m.insert("cpp", Cpp); m.insert("cc", Cpp); m.insert("cxx", Cpp); m.insert("c++", Cpp); m.insert("hpp", Cpp); m.insert("hh", Cpp); m.insert("hxx", Cpp);
    m.insert("rs", Rust);
    m.insert("go", Go);
    m.insert("java", Java);
    m.insert("cs", CSharp);
    m.insert("rb", Ruby);
    m.insert("php", Php); m.insert("phtml", Php); m.insert("php3", Php); m.insert("php4", Php); m.insert("php5", Php);
    // Tier 2
    m.insert("kt", Kotlin); m.insert("kts", Kotlin);
    m.insert("swift", Swift);
    m.insert("scala", Scala); m.insert("sc", Scala);
    m.insert("dart", Dart);
    m.insert("lua", Lua);
    m.insert("pl", Perl); m.insert("pm", Perl); m.insert("t", Perl);
    m.insert("r", R); m.insert("R", R);
    m.insert("jl", Julia);
    m.insert("hs", Haskell); m.insert("lhs", Haskell);
    m.insert("ex", Elixir); m.insert("exs", Elixir);
    m.insert("erl", Erlang); m.insert("hrl", Erlang);
    m.insert("clj", Clojure); m.insert("cljs", Clojure); m.insert("cljc", Clojure); m.insert("edn", Clojure);
    m.insert("ml", OCaml); m.insert("mli", OCaml);
    m.insert("groovy", Groovy); m.insert("gvy", Groovy); m.insert("gy", Groovy); m.insert("gsh", Groovy);
    m.insert("zig", Zig); m.insert("zir", Zig);
    // Tier 3 — select highlights
    m.insert("sh", Bash); m.insert("bash", Bash); m.insert("zsh", Bash); m.insert("ksh", Bash);
    m.insert("sql", Sql);
    m.insert("json", Json);
    m.insert("yaml", Yaml); m.insert("yml", Yaml);
    m.insert("toml", Toml);
    m.insert("html", Html); m.insert("htm", Html);
    m.insert("css", Css);
    m.insert("scss", Scss);
    m.insert("vue", Vue);
    m.insert("svelte", Svelte);
    m.insert("graphql", GraphQL); m.insert("gql", GraphQL);
    m.insert("md", Markdown); m.insert("markdown", Markdown); m.insert("mdx", Markdown);
    m.insert("xml", Xml); m.insert("svg", Xml); m.insert("xsd", Xml);
    m.insert("sol", Solidity);
    m.insert("tf", Hcl); m.insert("tfvars", Hcl); m.insert("hcl", Hcl);
    m.insert("nix", Nix);
    m.insert("nim", Nim);
    m.insert("v", Verilog); m.insert("vh", Verilog); m.insert("sv", Verilog); m.insert("svh", Verilog);
    m.insert("vhd", Vhdl); m.insert("vhdl", Vhdl);
    m.insert("f90", Fortran); m.insert("f95", Fortran); m.insert("f03", Fortran); m.insert("f08", Fortran); m.insert("f", Fortran); m.insert("for", Fortran);
    m.insert("adb", Ada); m.insert("ads", Ada);
    m.insert("pas", Pascal); m.insert("pp", Pascal); m.insert("dpr", Pascal);
    m.insert("elm", Elm);
    m.insert("purs", PureScript);
    m.insert("proto", Protobuf);
    m.insert("rkt", Racket);
    m.insert("scm", Scheme); m.insert("ss", Scheme);
    m.insert("lisp", CommonLisp); m.insert("lsp", CommonLisp); m.insert("cl", CommonLisp);
    m.insert("el", EmacsLisp);
    m.insert("tex", LaTeX); m.insert("sty", LaTeX); m.insert("cls", LaTeX);
    m.insert("org", Org);
    m.insert("norg", Norg);
    m.insert("diff", Diff); m.insert("patch", Diff);
    m.insert("ron", Ron);
    m.insert("cue", Cue);
    m.insert("rego", Rego);
    m.insert("ps1", PowerShell); m.insert("psm1", PowerShell); m.insert("psd1", PowerShell);
    m.insert("tcl", Tcl); m.insert("tk", Tcl);
    m.insert("re", Reason); m.insert("rei", Reason);
    m.insert("res", ReScript); m.insert("resi", ReScript);
    m.insert("fnl", Fennel);
    m.insert("fish", Fish);
    m.insert("awk", Awk);
    m.insert("gd", GDScript);
    m.insert("vert", Glsl); m.insert("frag", Glsl); m.insert("geom", Glsl); m.insert("comp", Glsl);
    m.insert("hlsl", Hlsl); m.insert("fx", Hlsl); m.insert("fxh", Hlsl); m.insert("hlsli", Hlsl);
    m.insert("wgsl", Wgsl);
    m.insert("w", Wing);
    m.insert("wit", Wit);
    m.insert("yang", Yang);
    m.insert("thrift", Thrift);
    m.insert("capnp", Capnp);
    m.insert("bicep", Bicep);
    m.insert("prql", Prql);
    m.insert("ql", Ql); m.insert("qll", Ql);
    m.insert("qml", Qml);
    m.insert("hoon", Hoon);
    m.insert("chpl", Chapel);
    m.insert("d", D); m.insert("di", D);
    m.insert("mojo", Mojo);
    m.insert("pony", Pony);
    m.insert("odin", Odin);
    m.insert("vala", Vala);
    m.insert("ha", Hare);
    m.insert("nu", Nushell);
    m.insert("typ", Typst);
    m.insert("tal", Uxntal);
    m.insert("tla", Tlaplus);
    m.insert("wl", Wolfram);
    m.insert("tsp", Typespec);
    m.insert("rasi", Rasi);
    m.insert("Rmd", RMarkdown);
    m.insert("scd", SuperCollider);
    m.insert("e", Eiffel);
    m.insert("abap", Abap);
    m.insert("clw", Clarion);
    m
});

/// Shebang patterns for script detection
static SHEBANG_MAP: Lazy<Vec<(&str, LanguageId)>> = Lazy::new(|| {
    vec![
        ("python", Python), ("python3", Python), ("python2", Python),
        ("node", JavaScript), ("nodejs", JavaScript),
        ("ts-node", TypeScript), ("tsx", TypeScript), ("deno", TypeScript), ("bun", TypeScript),
        ("ruby", Ruby),
        ("php", Php),
        ("perl", Perl),
        ("lua", Lua),
        ("bash", Bash), ("sh", Bash), ("zsh", Bash), ("ksh", Bash), ("dash", Bash),
        ("fish", Fish),
        ("awk", Awk),
        ("tclsh", Tcl), ("wish", Tcl),
        ("julia", Julia),
        ("guile", Scheme), ("racket", Racket),
        ("clojure", Clojure),
        ("escript", Erlang),
        ("elixir", Elixir),
        ("scala", Scala),
        ("groovy", Groovy),
        ("crystal", Crystal),
        ("nim", Nim),
        ("ghci", Haskell), ("runhaskell", Haskell), ("runghc", Haskell),
        ("ocaml", OCaml),
        ("fennel", Fennel),
        ("hy", Hy),
        ("ps1", PowerShell), ("pwsh", PowerShell),
    ]
});

/// Package manager / project file detection
static PACKAGE_FILE_MAP: Lazy<Vec<(&str, LanguageId)>> = Lazy::new(|| {
    vec![
        ("package.json", JavaScript),
        ("tsconfig.json", TypeScript),
        ("Cargo.toml", Rust),
        ("go.mod", Go),
        ("pom.xml", Java), ("build.gradle", Java), ("build.gradle.kts", Kotlin),
        ("Gemfile", Ruby),
        ("composer.json", Php),
        ("pyproject.toml", Python), ("setup.py", Python), ("requirements.txt", Python),
        ("mix.exs", Elixir),
        ("rebar.config", Erlang),
        ("deps.edn", Clojure),
        ("project.clj", Clojure),
        ("build.sbt", Scala),
        ("pubspec.yaml", Dart),
        ("Package.swift", Swift),
        ("CMakeLists.txt", Cpp),
        ("Makefile", Make),
        ("stack.yaml", Haskell), ("package.yaml", Haskell), ("*.cabal", Haskell),
        ("dune-project", OCaml), ("dune", OCaml),
        ("Project.toml", Julia),
        ("DESCRIPTION", R),
        ("nim.cfg", Nim),
        ("build.zig", Zig),
        ("shard.yml", Crystal),
        ("build.sbt", Scala),
        ("meson.build", Meson),
        ("WORKSPACE", Bazel), ("BUILD", Bazel),
        ("snapcraft.yaml", Yaml),
        ("chart.yaml", Yaml),
        ("terraform.tf", Hcl), (".terraform.lock.hcl", Hcl),
        ("Dockerfile", Dockerfile),
        ("Justfile", Just),
        ("flake.nix", Nix),
        ("shell.nix", Nix),
    ]
});
```

---

## 7. Parallel Parsing via Rayon

All files are processed in parallel using Rayon thread-pool work-stealing:

```rust
use rayon::prelude::*;
use std::sync::{Arc, Mutex, RwLock};
use arc_swap::ArcSwap;

pub struct ParallelParserPool {
    language_detector: Arc<LanguageDetector>,
    parser_registry: Arc<ParserRegistry>,
    max_file_size: usize,
    num_threads: usize,
}

impl ParallelParserPool {
    /// Parse all files in a directory tree in parallel.
    /// Returns a consolidated CryptoGraph with cross-file references resolved.
    pub fn parse_directory(
        &self,
        root: &Path,
        progress: Arc<ProgressReporter>,
    ) -> Result<ConsolidatedCpg, ParseError> {
        // Step 1: Walk the directory and collect file paths
        let files: Vec<PathBuf> = walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| !e.path().to_string_lossy().contains("node_modules"))
            .filter(|e| !e.path().to_string_lossy().contains("target"))
            .filter(|e| !e.path().to_string_lossy().contains(".git"))
            .filter(|e| e.metadata().map(|m| m.len() < self.max_file_size as u64).unwrap_or(false))
            .map(|e| e.path().to_path_buf())
            .collect();

        let total = files.len();
        progress.set_total(total as u64);

        // Step 2: Parallel file processing with ArcSwap for lock-free state sharing
        let global_cpg = Arc::new(ArcSwap::from_pointee(ConsolidatedCpg::new()));
        let errors: Arc<Mutex<Vec<(PathBuf, ParseError)>>> = Arc::new(Mutex::new(Vec::new()));

        files.par_iter().for_each(|file_path| {
            let file_result = self.parse_single_file(file_path, &global_cpg);
            progress.increment();

            match file_result {
                Ok(local_cpg) => {
                    // Lock-free merge: load, merge, CAS store
                    let current = global_cpg.load_full();
                    let mut merged = (*current).clone();
                    merged.merge_local(local_cpg);
                    global_cpg.store(Arc::new(merged));
                }
                Err(err) => {
                    errors.lock().unwrap().push((file_path.clone(), err));
                }
            }
        });

        progress.finish();

        let cpg = global_cpg.load_full();
        let errs = Arc::try_unwrap(errors).unwrap().into_inner().unwrap();

        if !errs.is_empty() {
            log::warn!("{} files had parse errors", errs.len());
            for (path, err) in &errs {
                log::debug!("Parse error in {}: {:?}", path.display(), err);
            }
        }

        Ok((*cpg).clone())
    }

    fn parse_single_file(
        &self,
        file_path: &Path,
        _global_cpg: &Arc<ArcSwap<ConsolidatedCpg>>,
    ) -> Result<LocalCpg, ParseError> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| ParseError::Io(file_path.to_path_buf(), e))?;

        let language = self.language_detector.detect(file_path, Some(&content))?;

        let parser = self.parser_registry.get(language)
            .ok_or(ParseError::NoParser(language))?;

        // Parse with timeout
        let timeout = Duration::from_secs(30);
        let result = std::thread::scope(|s| {
            let handle = s.spawn(|| {
                parser.parse_full(&content, file_path)
            });
            // Cannot use parking_lot timeout with scope easily; use channel approach
            // In production: use tokio::time::timeout or similar
            handle.join().unwrap_or(Err(ParseError::Timeout(language)))
        });

        result
    }
}

/// Tier-specific parser dispatch. Tiers determine which CPG components are built.
impl ParserRegistry {
    pub fn parse_full(&self, source: &str, path: &Path, lang: LanguageId) -> Result<LocalCpg, ParseError> {
        match self.tier_for(lang) {
            Tier::One => self.parse_tier_one(source, path, lang),
            Tier::Two => self.parse_tier_two(source, path, lang),
            Tier::Three => self.parse_tier_three(source, path, lang),
        }
    }

    fn parse_tier_one(&self, source: &str, path: &Path, lang: LanguageId) -> Result<LocalCpg, ParseError> {
        let ast = self.extract_ast(source, path, lang)?;
        let call_graph = self.extract_call_graph(&ast, lang)?;
        let cfg = self.construct_cfg(&ast, lang)?;
        let ssa = self.construct_ssa(&cfg, lang)?;
        let taint = self.run_taint_analysis(&ast, &call_graph, &cfg, &ssa, lang)?;
        let mutations = self.generate_mutations(&ast, lang)?;

        Ok(LocalCpg {
            language: lang,
            file_path: path.to_path_buf(),
            ast: Some(ast),
            call_graph: Some(call_graph),
            cfg: Some(cfg),
            ssa: Some(ssa),
            taint_findings: Some(taint),
            mutations: Some(mutations),
            ..Default::default()
        })
    }

    fn parse_tier_two(&self, source: &str, path: &Path, lang: LanguageId) -> Result<LocalCpg, ParseError> {
        let ast = self.extract_ast(source, path, lang)?;
        let call_graph = self.extract_call_graph(&ast, lang)?;
        let taint = self.run_basic_taint(&ast, &call_graph, lang)?;
        let mutations = self.generate_mutations(&ast, lang)?;

        Ok(LocalCpg {
            language: lang,
            file_path: path.to_path_buf(),
            ast: Some(ast),
            call_graph: Some(call_graph),
            cfg: None,
            ssa: None,
            taint_findings: Some(taint),
            mutations: Some(mutations),
            ..Default::default()
        })
    }

    fn parse_tier_three(&self, source: &str, path: &Path, lang: LanguageId) -> Result<LocalCpg, ParseError> {
        let ast = self.extract_ast(source, path, lang)?;
        let call_graph = self.extract_call_graph(&ast, lang)?;

        Ok(LocalCpg {
            language: lang,
            file_path: path.to_path_buf(),
            ast: Some(ast),
            call_graph: Some(call_graph),
            cfg: None,
            ssa: None,
            taint_findings: None,
            mutations: None,
            ..Default::default()
        })
    }
}
```

---

## 8. Incremental Parsing

On subsequent runs, only changed files are re-parsed:

```rust
pub struct IncrementalParser {
    cache_dir: PathBuf,
    pool: ParallelParserPool,
}

impl IncrementalParser {
    /// Track file state with content hashing + modification time.
    pub fn incremental_index(
        &self,
        root: &Path,
        previous_state: Option<&ParseState>,
    ) -> Result<IncrementalResult, ParseError> {
        let mut file_manifest = self.build_manifest(root)?;
        let previous = previous_state.unwrap_or(&ParseState::empty());

        // Categorize files
        let mut added: Vec<PathBuf> = Vec::new();
        let mut modified: Vec<PathBuf> = Vec::new();
        let mut deleted: Vec<PathBuf> = Vec::new();
        let mut unchanged: Vec<PathBuf> = Vec::new();

        for (path, fingerprint) in &file_manifest {
            match previous.fingerprints.get(path) {
                None => added.push(path.clone()),
                Some(old_fp) if old_fp != fingerprint => modified.push(path.clone()),
                Some(_) => unchanged.push(path.clone()),
            }
        }

        for path in previous.fingerprints.keys() {
            if !file_manifest.contains_key(path) {
                deleted.push(path.clone());
            }
        }

        log::info!(
            "Incremental: {} added, {} modified, {} deleted, {} unchanged",
            added.len(), modified.len(), deleted.len(), unchanged.len()
        );

        // Parse only changed files
        let changed: Vec<_> = added.iter().chain(modified.iter()).collect();

        let mut cpg = if let Some(base_cpg) = previous.cpg.as_ref() {
            // Clone and remove deleted file artifacts
            let mut c = base_cpg.clone();
            for path in &deleted {
                c.remove_file(path);
            }
            c
        } else {
            ConsolidatedCpg::new()
        };

        // Parse changed files in parallel
        if !changed.is_empty() {
            let newly_parsed = self.pool.parse_files_parallel(&changed, &PathBuf::new())?;
            for local_cpg in newly_parsed {
                cpg.merge_local(local_cpg);
            }
        }

        let new_state = ParseState {
            fingerprints: file_manifest,
            cpg: Some(cpg.clone()),
            timestamp: Utc::now(),
        };

        // Persist state to cache
        let state_path = self.cache_dir.join("parse_state.bincode");
        let encoded = bincode::serialize(&new_state)?;
        std::fs::write(&state_path, encoded)?;

        Ok(IncrementalResult {
            added: added.len(),
            modified: modified.len(),
            deleted: deleted.len(),
            unchanged: unchanged.len(),
            cpg,
            state: new_state,
        })
    }

    /// Build a content-addressable fingerprint for each file.
    fn build_manifest(&self, root: &Path) -> Result<FxHashMap<PathBuf, Fingerprint>, ParseError> {
        let mut fingerprints = FxHashMap::default();
        for entry in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let path = entry.path().to_path_buf();
                let metadata = entry.metadata()?;
                let modified = metadata.modified()?;
                let fingerprint = Fingerprint {
                    size: metadata.len(),
                    modified_nanos: modified.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64,
                    content_hash: None, // Delayed hashing for unchanged files
                };
                fingerprints.insert(path, fingerprint);
            }
        }
        Ok(fingerprints)
    }
}
```

---

## 9. Implementation Timeline — 16 Weeks, 4 Engineers

### Phase 0 — Infrastructure (Weeks 1-2)

| Task | Engineer(s) | Effort | Deliverable |
|------|------------|--------|------------|
| Set up tree-sitter WASM build pipeline | E1 | 3d | CI builds all grammars to .wasm |
| Define VNode IR and CPG data structures | E1, E2 | 5d | `cpg-ir` crate with stable schema |
| Implement `ParserRegistry` with tier dispatch | E2 | 3d | Dynamic parser loading |
| Build auto-detection system | E3 | 4d | `bugswarm-cpg index` with 140+ mappings |
| Implement parallel parsing via Rayon | E4 | 3d | `ParallelParserPool` |
| Build incremental parsing engine | E4 | 3d | Content fingerprinting + cache |
| Build test infrastructure | E3 | 2d | Snapshot testing + golden files |

### Phase 1 — Tier 1 Languages (Weeks 3-4) — Parallelizable

| Week | Task | Engineers | Scope |
|------|------|-----------|-------|
| 3 | Python + JavaScript (full) | E1, E2 | AST, CG, CFG, SSA, Taint, Mutations |
| 3 | TypeScript + C (full) | E3, E4 | AST, CG, CFG, SSA, Taint, Mutations |
| 4 | C++ + Rust (full) | E1, E2 | AST, CG, CFG, SSA, Taint, Mutations (C++ templates) |
| 4 | Go + Java + C# (full) | E3, E4 | AST, CG, CFG, SSA, Taint, Mutations |

### Phase 2 — Tier 1 remaining (Weeks 5-6)

| Week | Task | Engineers | Scope |
|------|------|-----------|-------|
| 5 | Ruby + PHP (full) | E1, E2 | All 6 components |
| 5 | Polishing + testing Tier 1 | E3, E4 | Juliet test suites for all 11 languages |
| 6 | Sanitizer integrations (all Tier 1) | E1, E2, E3, E4 | ASAN, hypothesis, Node --inspect, Miri, go -race |

### Phase 3 — Tier 2 Languages (Weeks 7-8) — Parallelizable

| Week | Languages | Engineers | Scope |
|------|-----------|-----------|-------|
| 7 | Kotlin, Swift, Scala, Dart, Lua | E1, E2, E3, E4 | AST + CallGraph + Taint + Mutations |
| 7 | Perl, R, Julia, Haskell, Elixir | E1, E2, E3, E4 | AST + CallGraph + Taint + Mutations |
| 8 | Erlang, Clojure, OCaml, Groovy, Zig | E1, E2, E3, E4 | AST + CallGraph + Taint + Mutations |
| 8 | Tier 2 sanitizer integrations | E1, E2, E3, E4 | Dialyzer, Credo, Clj-Kondo, etc. |

### Phase 4 — Tier 3 Languages Batch 1 (Weeks 9-10) — 40 languages

| Language Group | Quantity | Effort per lang | Total Effort |
|---------------|----------|-----------------|--------------|
| Shell/Make/Build (Bash, Make, CMake, Meson, Bazel, Gradle, Just, Ninja, Earthfile) | 9 | 0.5d | 4.5d |
| Config/Data (JSON, YAML, TOML, XML, INI, CUE, Dhall, JSONnet, HOCON, RON, Bicep) | 11 | 0.3d | 3.3d |
| Web (HTML, CSS, SCSS, Vue, Svelte, QML) | 6 | 0.5d | 3d |
| Query/DB (SQL, GraphQL, PRQL, CodeQL, HCL, Rego, Protobuf, Thrift, Capnp, WIT) | 10 | 0.5d | 5d |
| Rust-adjacent (Nix, TOML, RON) | 3 | 0.3d | 0.9d |
| Total Week 9-10 | ~39 | | ~16.7d / 4 eng = 4.2d |

### Phase 5 — Tier 3 Languages Batch 2 (Weeks 11-12) — 40 languages

| Language Group | Quantity | Effort per lang | Total Effort |
|---------------|----------|-----------------|--------------|
| Functional (Haskell, PureScript, Elm, F#, OCaml extras, Reason, ReScript, Clojure extras, Scheme, Racket, CommonLisp, EmacsLisp, Hy, Fennel) | 14 | 0.5d | 7d |
| Systems (Zig extras, Odin, Nim, Vala, Pony, Hare, D, Crystal) | 8 | 0.5d | 4d |
| Scientific (Fortran, R extras, Julia extras, MATLAB, Octave, Wolfram, Stata, SAS) | 8 | 0.5d | 4d |
| HDL (Verilog, VHDL, SystemVerilog, GLSL, HLSL, WGSL) | 6 | 0.5d | 3d |
| Misc (Elm, Ada, Pascal, Groovy extras, PowerShell, Fish, Awk) | 7 | 0.3d | 2.1d |
| Total Week 11-12 | ~43 | | ~20.1d / 4 eng = 5d |

### Phase 6 — Tier 3 Languages Batch 3 (Weeks 13-14) — Remaining ~40 languages

| Language Group | Quantity | Effort per lang | Total Effort |
|---------------|----------|-----------------|--------------|
| Docs/Markup (LaTeX, Markdown, RMarkdown, Org, Norg, BibTeX, Typst, AsciiDoc, ReStructuredText) | 9 | 0.3d | 2.7d |
| Verification (TLA+, Alloy, Coq, Agda, Idris, Isabelle, Lean, LiquidHaskell) | 8 | 0.5d | 4d |
| DSLs (GDScript, GodotResource, MaxScript, MEL, UnrealScript, Blueprint, Tiger, Cooklang) | 8 | 0.3d | 2.4d |
| Protocols (Protobuf, Thrift, Capnp, Sproto, Turtle, WIT) | 6 | 0.3d | 1.8d |
| Misc (Solidity, Cairo, Squirrel, SuperCollider, Tcl, PioASM, Uxntal, Elvish, Vimscript, Vimdoc) | 10 | 0.3d | 3d |
| Total Week 13-14 | ~41 | | ~13.9d / 4 eng = 3.5d |

### Phase 7 — Integration, Testing, Docs (Weeks 15-16)

| Task | Engineers | Effort | Deliverable |
|------|-----------|--------|------------|
| Cross-language Juliet test suite equivalents | E1, E2 | 5d | 100+ language test corpora |
| Mutation testing validation | E3 | 3d | 1000+ language-specific mutants validated |
| Performance benchmarking | E4 | 3d | Parse 1M LOC in <60s |
| Documentation: per-language usage guide | E1, E2 | 2d | Docs site |
| CI/CD pipeline for 100+ grammars | E3 | 2d | GitHub Actions matrix build |
| Release: `bugswarm-cpg v3.0` | E1-E4 | 2d | Ship to crates.io + Homebrew |

---

## 10. Testing Strategy

### 10.1 Juliet Test Suite Equivalents Per Language

The NIST Juliet Test Suite provides 65,000+ test cases across 118 CWEs. We build language-adapted versions for Tier 1-2 languages.

| Language | Test Cases (adapted from Juliet) | Source of Adaptation |
|----------|----------------------------------|---------------------|
| Python | 10,000+ (OWASP Python Security, Bandit test corpus, PyT test cases) | pyt/test/, bandit/examples/ |
| JavaScript | 12,000+ (NodeGoat, OWASP JuiceShop, Semgrep JS rules) | semgrep-rules/javascript/ |
| TypeScript | 8,000+ (TypeScript-specific + JS tests) | TS adapted from JS |
| C | 65,000+ (original Juliet 1.3) | samate.nist.gov/SRD |
| C++ | 35,000+ (original Juliet 1.3 C++) | samate.nist.gov/SRD |
| Rust | 5,000+ (RustSec/advisory-db, cargo-geiger) | github.com/RustSec |
| Go | 8,000+ (Go CWE corpus, gosec test suite) | github.com/securego/gosec |
| Java | 20,000+ (original Juliet 1.3 Java) | samate.nist.gov/SRD |
| C# | 10,000+ (Juliet C#, Roslyn analyzer tests) | samate.nist.gov/SRD |
| Ruby | 5,000+ (Brakeman test apps, RubySec) | github.com/presidentbeef/brakeman |
| PHP | 5,000+ (PHP Security Checker, Psalm test suite) | github.com/vimeo/psalm |

### 10.2 Tier 3 Testing

| Tier 3 Group | Test Source | Coverage Target |
|-------------|-------------|-----------------|
| Shell/Bash | ShellCheck test suite | AST + call graph accuracy > 95% |
| SQL | SQLFluff test corpus | AST + call graph accuracy > 95% |
| Config languages | Custom corpus of 500 files per language | AST accuracy > 98% |
| Markup/ML | Pandoc test suite + custom corpus | AST + call graph > 95% |
| DSLs | Language-specific test suites | AST accuracy > 90% |

---

## 11. Summary

| Metric | Value |
|--------|-------|
| Total languages supported | 141 |
| Tier 1 (full CPG) | 11 languages |
| Tier 2 (AST+CG+Taint) | 15 languages |
| Tier 3 (AST+CG) | 115 languages |
| Total parser files | ~900 (6-8 per Tier 1, 4-5 per Tier 2, 1 per Tier 3) |
| Estimated total LOC | ~150,000 |
| Engineering effort | 4 engineers × 16 weeks = 64 person-weeks |
| Parallel throughput target | 1M LOC parsed in <60 seconds |
| Incremental parse latency | <5 seconds for typical PR changes |
| Coverage target | 95%+ AST accuracy across all languages |
