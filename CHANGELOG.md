# Changelog

"""
Release history for D#².

Each release lists the language changes and the notes that matter for users.
"""

## v1.8.0 — 2026-11-18

"""
OpenGL and Vulkan windows from D#2 scripts.
"""

### Language

"""

- `gl.init(title, width, height)` opens an SDL2 window with an OpenGL 3.3 core
  context. `gl.poll()`, `gl.shouldClose()`, `gl.present()`, and `gl.close()`
  drive the event loop.
- OpenGL state calls are available: `clearColor`, `clear`, `clearDepth`,
  `viewport`, `enable`, `disable`, `blendFunc`, `depthFunc`, `cullFace`,
  `getError`, `errorName`, and `getString`.
- OpenGL objects are exposed as Int handles: shader compilation, program
  linking, buffers, vertex arrays, vertex attributes, draws, uniforms, and
  matrix uploads all have `gl.*` calls.
- `vk.init(title, width, height)` opens an SDL2 Vulkan window with an instance,
  device, swapchain, render pass, command buffers, and synchronisation objects.
  `vk.poll()`, `vk.shouldClose()`, `vk.present()`, and `vk.close()` drive the
  frame loop.
- Vulkan also exposes `clearColor`, `setTitle`, `size`, `deviceName`,
  `apiVersion`, and `waitIdle`. The present path clears the swapchain image.
- Both backends use Python-style keyword-free calls and report errors with the
  script line, for example
  `line 1: OpenGL is not initialized; call gl.init(title, width, height)`.

"""

### Notes

"""

- Graphics require a desktop session and the SDL2, OpenGL, and Vulkan runtime
  libraries. The Rust dependencies are `sdl2`, `glow`, and `ash`.
- The OpenGL backend is the complete drawing path today. Vulkan shader modules,
  graphics pipelines, buffers, and draw commands are recorded as `TODO` in
  `src/builtin_functions.json`.
- One window per backend can be open at a time. Close it before opening
  another.

"""

## v1.7.0 — 2026-11-18

"""
Value conversions between every built-in type.
"""

### Language

"""

- `str(x)` and its aliases `text` / `Text` convert any value to Text using its
  display form. `str()` returns `""`.
- `int(x)` / `Int` accepts Int, Float (truncated toward zero), Bool, and whole
  number Text. `float(x)` / `Float` accepts Int, Float, Bool, and numeric Text.
- `bool(x)` / `Bool` applies Python-style truthiness to every value: `0`, `0.0`,
  `""`, empty collections, empty matrices, and `None` are false; structs are
  true.
- `set(x)` / `hashSet` / `Set` expands one iterable argument (list, hashset,
  hashmap keys, matrix row names, or text characters) and treats multiple
  arguments as individual items.
- `dict(x)` / `hashMap` / `Dict` builds a hashmap from a hashmap, matrix,
  struct fields, or a list of `[key, value]` pairs.
- `matrix(x)` / `Matrix` / `mat` / `matrex` builds a matrix from a matrix, a
  nested hashmap `{row: {cell: value}}`, a struct, or a list of
  `[row, cells]` pairs.
- `matrix(...)` and `mat(...)` calls are now parsed; `mat{...}` literals still
  work exactly as before.
- Calling a conversion with no argument produces the empty value for its type.
- Impossible conversions raise errors that name the source and target, such as
  `cannot convert text "abc" to Int` or `cannot convert list to Float`.
- A function whose body ends with `if`/`elif`/`else` now returns the final bare
  expression of the selected branch, so `abs`-style functions no longer fall
  through to `None`.
- `=<` and `=>` are accepted as aliases for `<=` and `>=`.
- `file(path)` creates a file object. It supports `read()`, `write(data)`,
  `append(data)`, `delete()`, `copy(dest)`, `rename(new)`, `exists()`,
  `create()`, and `size()`.
- File paths, written data, and copy destinations must be Text. `copy` returns
  the copied byte count, `size` returns bytes as Int, and `rename` updates the
  object's path when called on a plain variable.
- Rust-style ranges are written `start..end` (end excluded) and
  `start..=end` (end included). Endpoints must be Int, and backwards ranges
  are empty.
- Ranges work in `for` loops, membership tests (`if n in 0..5`), `list(...)`,
  `len(...)`, and the conversion builtins. `for` loops consume ranges lazily
  instead of materializing every value.
- `Range` is accepted as a parameter and list element type annotation.
- Union type annotations accept either type: `Int or Float`, including in
  returns (`-> Int or Float`) and typed lists (`List[Int or Float]`).
- `**` is exponentiation: `2 ** 10` is `1024` and `9 ** 0.5` is `3`. It is
  right-associative and binds tighter than unary minus, so `-2 ** 2` is `-4`.
  Integer bases stay Int for non-negative integer exponents; floats and
  negative exponents produce a Float.

"""

### Notes

"""

- Conversions are explicit builtins; there are no implicit coercions in
  arithmetic, comparisons, or conditions.
- `int` rejects non-finite floats and values outside the 64-bit signed range.
- `dict()` and `matrix()` take at most one argument.
- `set` remains compatible with its earlier item-list form, so `set(1, 2)`
  still creates a hashset of two items while `set("ab")` now expands to
  characters.

"""

## v1.6.0 — 2026-11-07

"""
Pythonic test blocks and assertions.
"""

### Language

"""

- `test name { ... }` and `test "description" { ... }` declare tests. Tests are
  registered when a file runs and execute only with `--test`.
- `dsharp2 --test file.dsq` (or `cargo run -- --test file.dsq`) runs every test
  declared by the file and its imports, printing `ok`/`FAILED` per test and a
  summary. The process exits with status 1 when any test fails.
- `assert(condition)` and `assert(condition, message)` fail when the condition
  is false.
- `assert_eq(left, right)` and `assert_eq(left, right, message)` compare with
  the language’s value equality and report both values on failure.
- `assert_ne(left, right)` and `assert_ne(left, right, message)` require the
  values to differ.
- Each test runs in its own scope. Duplicate test names are rejected.
- Normal script runs ignore test blocks, so test files can be executed or
  imported without running the tests.

"""

### Notes

"""

- Assertions are plain builtins, not macros: write `assert_eq(a, b)` rather
  than `assert_eq!(a, b)`.
- Test failures report the source file and line, for example
  `assert_eq failed: left = 3, right = 4`.
- Tests run after the file’s top-level statements, so helper functions,
  structs, traits, and imports are available.
- The test runner does not yet support filtering, ignored tests, or expected
  failure annotations.

"""

## v1.5.0 — 2026-11-07

"""
Return type annotations.
"""

### Language

"""

- Functions and methods accept optional `-> Type` after the parameter list:
  `def max(input: List[Int]) -> Int { ... }`.
- The returned value is checked at runtime on every call. This includes values
  from explicit `return`, implicit final expressions, and the implicit `None`
  returned when a body falls through.
- Return annotations support the same types as parameters: built-ins, structs,
  traits, type parameters (`-> T`), and typed lists such as `-> List[Int]`.
- Generic return types resolve through the function’s type parameters; `-> T`
  accepts the value produced for the call.
- Methods in `impl` blocks and trait methods may declare return types. Trait
  implementations are checked for a matching annotation, and default trait
  methods enforce their declared return type.

"""

### Notes

"""

- Return types are checked dynamically, not statically, so a mismatched branch
  reports the error when that branch is executed.
- A return annotation must name a type, not a variable. `-> input` is rejected
  as `unknown type annotation 'input'`; write `-> Int` or the intended type.
- `None` is a valid annotation (`-> None`), and must be written explicitly when
  a function intentionally returns nothing.

"""

## v1.4.0 — 2026-11-06

"""
Typed lists and Rust-style formatted output.
"""

### Language

"""

- `List[Int](...)` creates a list whose element type is enforced at runtime.
  Element types may be built-ins, structs, traits, type parameters, or nested
  lists such as `List[List[Int]]`.
- Typed-list checks apply on construction and on every mutation: `append`,
  `extend`, `insert`, index assignment, slice assignment, and slice deletion
  leave the element type intact.
- `List[Int](iterable)` expands a single iterable argument like `list(...)`.
  Slicing, `copy()`, and repetition with `*` preserve the element type.
- Passing an untyped list to a `List[Int]` parameter promotes the list to
  `Int`, so later mutations through that list are checked too. Bare `List`
  still requires only that the value is a list.
- `print!`, `println!`, `eprint!`, and `eprintln!` take a format string and
  values. `{}` prints a value, `{:?}` prints its repr form, explicit indices
  such as `{0}`/`{1:?}` select arguments, and `{{`/`}}` escape literal braces.
- Format strings are validated: the string must be text, placeholders must
  reference existing arguments, and unused arguments are reported.

"""

### Notes

"""

- Typed lists are checked dynamically, not statically. Errors name the failed
  element type, for example `expected Int, got text`.
- The format macros intentionally support only `{}` and `{:?}`. Width,
  alignment, and named arguments are not implemented yet.
- `println!` is a macro-like call parsed as an expression; `foo!(...)` is
  rejected with an unknown-format-macro error.
- Rust-style ranges such as `1..100.step_by(2)` are not part of this release.

"""

## v1.3.0 — 2026-11-06

"""
Python-style lists, multifile imports, typed parameters, and implicit returns.
"""

### Language

"""

- `import "path.dsq"` loads another script; `import path` is shorthand for
  `path.dsq`. Relative imports search the running interpreter’s install
  directory and its `lib/` subfolder first, then the importing file’s directory.
- Quoted `./` and `../` paths skip the install search and resolve only relative
  to the importing file; absolute quoted paths are used as-is.
- Imported files are parsed with the shared macro table, so macros can cross
  file boundaries in either direction (before or after the `import`).
- Imported statements execute where the `import` appears and share the current
  scope. A file runs at most once per program; cycles are rejected at parse time
  with a `cyclic import` error.
- Runtime and parse errors from imported files include the file name, for
  example `lib/math.dsq:line 2: division by zero`.
- Function and method parameters accept type annotations such as `x: Int`.
  Built-ins (`Int`, `Float`, `Number`, `Bool`, `Text`, `Any`, `HashSet`,
  `HashMap`, `Matrix`) plus struct, trait, and type-parameter names are checked
  when the function is called.
- `for key, value in hashmap { ... }` binds both the key and its value.
- A function or method whose final statement is a bare expression returns that
  expression implicitly.
- Python-style lists: `[1, 2, 3]`, `[]`, mixed and nested values, `len(list)`,
  `list(iterable)`, concatenation with `+`, and repetition with `*`.
- Lists are mutable reference values like Python: `b = a` shares the same list,
  so appending through `b` is visible through `a`. Indexing supports negative
  positions, and slicing supports `start:stop:step` with omitted bounds and
  negative steps.
- List assignment supports `a[i] = value`, `a[start:stop] = values`, and
  extended-slice assignment with matching lengths. `del a[i]` and
  `del a[start:stop]` remove items; `del d[key]` removes a hashmap entry.
- List methods: `append`, `extend`, `insert`, `remove`, `pop`, `clear`, `index`,
  `count`, `sort`, `reverse`, and `copy`.
- The `in` and `not in` operators work on lists, texts, hashsets, hashmaps, and
  matrix row names.
- Text also supports slicing with the same `start:stop:step` syntax.

"""

### Notes

"""

- Imports are file inclusion, not namespaces: there is no `module.name` access
  or selective import. Put imports at the top level.
- Bare imports prefer installed libraries over local files with the same name;
  use `./name` to force the file next to the importing script.
- A path with no extension gets `.dsq` appended. Quoted paths may be absolute
  or contain `..`; unquoted paths use `/` segments such as `lib/math`.
- Lists are reference values, but `del` and slice assignment only target a
  variable base (`a[0] = 1`, `del a[0]`); chained targets such as
  `d["a"][0] = 1` are not supported.
- `list.sort()` supports lists containing only numbers, only text, or only
  bools; mixed-type sorting is an error.
- There are no list comprehensions, tuple unpacking, keyword arguments, or
  list/dict `.items()`-style iteration pairs yet.

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
