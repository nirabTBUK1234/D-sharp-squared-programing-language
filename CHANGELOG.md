# Changelog

"""
Release history for D#².

Each release lists the language changes and the notes that matter for users.
"""

## v1.2.0 — 2026-11-02

"""
Structs, traits, and macros release.
"""

### Language

"""

- `struct Name { field, field = default }` defines a fixed-field record with a
  constructor of the same name; fields are read and written with `.`.
- `impl Name { def method(self, ...) { ... } }` attaches methods. Methods mutate
  the receiver when called on a variable, Python-style, and must take `self`
  first.
- `trait Name { def required(self); def default(self) { ... } }` declares a
  method set, and `impl Trait for Type { ... }` implements it. Required methods
  are checked at impl time, defaults are inherited or overridden, and dispatch
  checks inherent methods before trait methods. A trait can be implemented once
  per struct; two traits cannot supply the same method name.
- `def name<T: Trait>(value)` declares a trait-bounded function. At each call,
  the leading value parameters must be structs implementing their bound. Type
  parameters are positional; `<T>` without a bound is accepted but has no runtime
  effect. Only free functions support this for now.
- `macro name($a, $b) { ... }` defines a token macro, invoked with `name$(...)`
  in statement position; the `$` marks the call as a macro, like Rust's `!`, so
  a macro and a function can share a name. Arguments are substituted textually
  and the expansion is parsed. Macros must be defined before use, are unhygienic,
  and expand at most 32 levels deep.
- Struct display is `Point(x=1, y=2)`.

"""

### Notes

"""

- No generic structs, generic methods, visibility, `match`/enums, or
  ownership/borrowing; the last one has no meaning in a dynamically typed
  interpreter.
- Traits are interfaces plus defaults, not runtime types: there are no type
  annotations, trait objects, or `is` checks.
- Macros work only in statement position and are called with `name$(...)`; a
  bare `name(...)` is a normal function call, so macros and functions can share
  names.

"""

## v1.1.0 — 2026-11-01

"""
Collection syntax release.
"""

### Language

"""

- `hashSet(1, 2, 3)` constructs a hashset (`set`/`hashset` aliases); the old
  `hashset 1 2 3` literal syntax is gone. Hashsets print as `hashSet(...)`.
- `{key: value}` is a Python-style hashmap literal, with `[ ]` lookups and
  assignment (`d["a"]`, `d["b"] = 2`), key-based iteration, and `{}` for empty.
- Hashmap keys can be int, float, bool, or text; missing keys and scalar-key
  violations are errors with the source line.
- Text indexing: `"abc"[0]` and `"abc"[-1]`.
- Removed the old `matrix row1 { a 1 b 2 }` form; `mat{...}` is the only matrix
  literal now.
- Matrix display was already `mat{...}`; hashmap display is `{...}`.

"""

### Notes

"""

- Map literals cannot start a statement (that position is a block): use
  `d = {}`.
- Index assignment needs a variable base; `d["a"]["b"] = 1` is not supported.

"""

## v1.0.0 — 2026-11-01

"""
First stable release.
"""

"""

The first stable release. It replaces the earlier pre-releases
(`PreReleace1OfV1.0`, `PreReleace2OfV1`).

"""

### Language

"""

- Pythonic keywords with braced blocks instead of indentation.
- Plain assignment: `x = 1` creates the name; `del x` removes it.
- `def name(params) { ... }` with `return`, default parameter values,
  recursion, and checked arity.
- Control flow: `if` / `elif` / `else`, `while`, `loop`, `for x in ...`,
  `break`, `continue`.
- `True`, `False`, and `None`.
- `#` comments.
- Operators: `and`, `or`, `not`, `==`, `!=`, `>`, `>=`, `<`, `<=`, `+`, `-`,
  `*`, `/`, `%` (short-circuiting logic, checked integer arithmetic).
- Text with `"double"` or `'single'` quotes and `\n`, `\t`, `\r`, `\\`, `\"`,
  `\'` escapes.
- `hashset 1 2 3` and `mat{row: {cell: value}}` collections, including nested
  matrices and text keys.
- `print(...)` and `input(...)` builtins plus the `console.*` methods from
  `src/builtin_functions.json`.
- REPL with persistent state and multi-line blocks.
- Parser and runtime errors report the source line.

"""

### Notes

"""

- No classes, modules, `match`, `try`/`except`, closures, or list/dict
  literals yet.
- Functions are dynamically scoped.
- The old Rust-like `let`/`mut`/`fn` syntax from the pre-releases no longer
  parses; the README has a migration table.

"""
