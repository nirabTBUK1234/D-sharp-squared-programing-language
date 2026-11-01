# Changelog

## v1.0.0 — 2026-11-01

The first stable release. It replaces the earlier pre-releases
(`PreReleace1OfV1.0`, `PreReleace2OfV1`).

### Language

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

### Notes

- No classes, modules, `match`, `try`/`except`, closures, or list/dict
  literals yet.
- Functions are dynamically scoped.
- The old Rust-like `let`/`mut`/`fn` syntax from the pre-releases no longer
  parses; the README has a migration table.
