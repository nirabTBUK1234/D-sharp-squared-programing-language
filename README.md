# D#² (D-sharp-squared)

"""

A small, interpreted programming language implemented in Rust. The keywords are
Pythonic (`def`, `if`/`elif`/`else`, `x = 1`, `True`/`False`/`None`, `#`
comments) but blocks use braces instead of indentation. It has a real lexer and
recursive-descent parser, functions with defaults and recursion, loops,
hashsets, hashmaps, matrices, Python-style lists, type-enforced lists,
Rust-style formatted output, return type annotations, built-in test blocks and
assertions, structs with `impl` methods, traits and trait-bounded functions,
token macros, multifile imports, console and file built-ins, desktop graphics,
and a REPL.

It is still an early prototype: no namespaced modules, `match`/enums, or
closures yet — see [Roadmap](#roadmap) and [Known limitations](#known-limitations).

- **v1.8.0** — current release (2026-11-18): desktop graphics through `gl.*`
  (OpenGL 3.3 core with shaders, buffers, vertex arrays, draws, and uniforms)
  and `vk.*` (Vulkan window, swapchain, clear, and present).
- **v1.7.0** — value conversions for every built-in type, `file(path)` objects,
  Rust-style ranges, union type annotations, aliases `=<`/`=>`, and the `**`
  exponentiation operator (2026-11-18). Also includes the test-branch sync
  automation.
- **v1.6.0** — Pythonic `test` blocks and
  `assert` / `assert_eq` / `assert_ne` helpers, run with `--test` (2026-11-07).
- **v1.5.0** — return type annotations with
  `-> Type` on functions and methods, including generic and `List[...]` returns
  (2026-11-07).
- **v1.4.0** — type-enforced lists
  (`List[Int]`), nested typed lists, and Rust-style `print!`/`println!` format
  macros (2026-11-06).
- **v1.3.0** — Python-style lists with slicing and
  methods, multifile imports, typed parameters, two-variable hashmap loops, and
  implicit final-expression returns (2026-11-06).
- **v1.2.0** — structs with default fields, `impl` methods, traits with
  required/default methods, trait-bounded functions, and `name$(...)` token
  macros (2026-11-02).
- **v1.1.0** — collection syntax release (2026-11-01): `hashSet(...)`, `{...}`
  hashmaps with `[ ]` indexing, text indexing, and `mat{...}`-only matrices.
- **v1.0.0** — first stable release (2026-11-01): Pythonic keywords with braced
  blocks, plain assignment, functions, control flow, operators, text escapes,
  console built-ins, and a persistent REPL; see
  [CHANGELOG.md](CHANGELOG.md) for the complete list.
- **Version 1 Pre-release 2** (2026-10-31, tag `PreReleace2OfV1`) — Pythonic
  rewrite: `x = 1` assignment, expressions and operators, `if`/`elif`/`else`,
  `while`/`loop`/`for`, `def` functions with defaults and recursion,
  `True`/`False`/`None`, `#` comments, and braced blocks.
- **Version 1 Pre-release 1** (2026-10-31, tag `PreReleace1OfV1.0`) — first
  working interpreter: tokenizer, statement parser, Rust-like `let`/`mut`
  variables, `hashset`/`matrix` values, console built-ins, and a REPL.
- Graphics use the `sdl2`, `glow`, and `ash` crates; the interpreter and
  language runtime otherwise use the Rust standard library.
- Rust 2024 edition (Rust 1.85+)
- Source: <https://github.com/nirabTBUK1234/D-sharp-squared-programing-language>

"""

## Build and run

"""

```bash
cargo build
cargo run -- program.dsq   # run a script (any file extension works)
cargo run -- --test tests.dsq  # run `test` blocks
cargo run                  # start the REPL
cargo test                 # run the Rust test suite
```

### Test branch automation

Development happens on the `test` branch; `main` stays as the released branch.
Two independent mechanisms keep `origin/test` up to date.

**GitHub Actions.** Pushing any branch other than `test` or `main` makes the
[`Sync test branch`](.github/workflows/sync-test-branch.yml) workflow mirror
that commit to `test`. `test` is excluded so the mirror never re-triggers, and
`main` is excluded so a release never rewrites development work. The workflow
needs only the default `GITHUB_TOKEN` with `contents: write`; it never pushes
`main`.

**Local hooks.** In a clone where you want immediate syncing, the repository
also ships versioned git hooks that mirror every commit, merge, amend, and
rebase to `origin/test`:

```bash
scripts/install-test-sync.sh   # enable the hooks in a fresh clone
```

- The automation only pushes `HEAD` to `refs/heads/test`.
- It never pushes or rewrites `main`.
- A failed push (offline, no permissions) prints a warning but does not block
  the commit.
- Skip one sync with `DSH2_SKIP_TEST_SYNC=1 git commit ...`.
- Merge `test` into `main` deliberately when a change is ready to release.

"""

## Quick start

"""

`fizzbuzz.dsq`:

```text
# FizzBuzz
def fizzbuzz(n) {
    if n % 15 == 0 {
        return "FizzBuzz"
    } elif n % 3 == 0 {
        return "Fizz"
    } elif n % 5 == 0 {
        return "Buzz"
    } else {
        return n
    }
}

i = 1
while i <= 15 {
    print(fizzbuzz(i))
    i = i + 1
}
```

```bash
$ cargo run -- fizzbuzz.dsq
1
2
Fizz
4
Buzz
...
FizzBuzz
```

"""

## Language reference

"""
The language reference is organized by construct: statements, values, variables,
control flow, functions, structs, traits, macros, operators, collections, and
console built-ins.
"""

### Statements

"""

- Statements end at a newline or `;`.
- `{ ... }` groups statements into a block.
- `#` starts a comment that runs to the end of the line.
- Text literals use `"double"` or `'single'` quotes and may contain spaces, `;`,
  and braces. Escapes: `\n`, `\t`, `\r`, `\\`, `\"`, `\'`.
- A stray token after a statement is an error rather than being silently
  ignored (it catches leftovers from the old `let` syntax):
  `unexpected 'x' after a statement (missing a newline or ';'?)`.

"""

### Values

"""

| Type | Examples | Notes |
|---|---|---|
| Int | `42`, `-7` | 64-bit signed |
| Float | `3.14`, `-0.5` | 64-bit |
| Bool | `True`, `False` | `true`/`false` are also accepted |
| Text | `"hello"`, `'hi there'` | Supports indexing and slicing |
| List | `[1, 2, 3]`, `[]` | Mutable, reference-shared, mixed values; indexing, slicing, and methods |
| Typed list | `List[Int](1, 2, 3)` | A list whose items are checked against one element type |
| Hashset | `hashSet(1, 2, 2)` / `set(1, 2)` | Duplicates collapse; scalars only |
| Hashmap | `{"a": 1, "b": 2}` | Keys can be int, float, bool, or text; read/write with `[ ]` |
| Matrix | `mat{row1: {a: 1, b: 2}}` | Named rows of named cells; `matrix`/`matrex` are aliases |
| Struct | `Point(1, 2)` | Fixed named fields; declared with `struct`, methods with `impl`, behavior with traits |
| None | `None` | Returned by functions that do not return a value |

"""

### Variables

"""

Assigning creates the name; there are no declaration keywords:

```text
x = 1
x = x + 1          # x is now 2
name = "Nirab"
del name           # remove it again
```

- `del name` errors if the name is unknown.
- Unlike Python, blocks are shared, not scopes: only a function call creates a
  scope. A name first assigned inside an `if` or loop body stays visible
  afterwards, and assignment updates the nearest existing binding.
- Reading an unknown name is an error: `unknown variable 'missing'`.

"""

### Control flow

"""

```text
if score >= 90 {
    print("A")
} elif score >= 80 {
    print("B")
} else {
    print("C")
}

i = 0
while i < 3 {
    print(i)
    i = i + 1
}

loop {                     # extension: Python has no `loop`
    i = i + 1
    if i == 5 { break }
}

for ch in "abc" {
    print(ch)
}
```

- Conditions must be `bool`; there is no truthiness, so `if 1 { }` is an error.
- `elif` may follow `}` on the same line or the next line.
- `for name in <value>` iterates a `list` or `hashSet` (items), a `hashmap`
  (keys), a `matrix` (row names, sorted), or `text` (characters).
- `for key, value in <hashmap>` binds both the key and its value. Two-variable
  loops are only supported for hashmaps.
- `break` and `continue` only work inside a loop.
- `loop { }` is a convenience for `while True` and is the one non-Python
  control-flow keyword kept as an extension.

"""

### Functions

"""

```text
def greet(name: Text, punctuation = "!") -> Text {
    return "Hello, " + name + punctuation
}

def fact(n: Int) -> Int {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}

def double(x: Int) -> Int {
    x * 2                    # the final expression is the return value
}

def min(input: List[Int]) -> Int {
    lowest = input[0]
    for i in input[1:] {
        if i < lowest { lowest = i }
    }
    lowest
}

print(greet("Nirab"))          # Hello, Nirab!
print(greet("Nirab", "?"))     # Hello, Nirab?
print(fact(5))                 # 120
print(double(4))               # 8
print(min([4, 1, 9]))          # 1
```

- Parameters come before the optional defaults; a required parameter may not
  follow one with a default.
- A parameter may be annotated as `name: Type`, for example `x: Int`. Built-in
  annotations are `Int`, `Float`, `Number`, `Bool`, `Text`, `Any`, `List`,
  `HashSet`, `HashMap`, and `Matrix`; a struct, trait, or type-parameter name can
  also be used. Annotations are checked when the function is called. Use
  `List[Int]` (or `List[List[Text]]`, and so on) to require one element type.
- Add `-> Type` after the parameter list to declare a return type:
  `def max(input: List[Int]) -> Int { ... }`. The returned value, including an
  implicit final expression, is checked on every call. `-> T` works for generic
  functions, and `-> List[Int]` promotes a returned untyped list.
- Defaults are evaluated when the function is called, in the function's scope,
  so they can refer to earlier parameters.
- `return` ends the function and returns a value. A bare final expression is
  returned implicitly (`def double(x) { x * 2 }`); otherwise falling off the
  end gives `None`.
- Recursion works (`fact`, `fib`, mutual recursion).
- A `def` inside a block is still a plain assignment in that scope, so it stays
  visible after the block ends.
- Arity is checked: `function 'f' takes 2 argument(s), but 3 given`.

A call scope is created per call; there are no closures yet (see limitations).

"""

### Tests

"""

`test name { ... }` or `test "description" { ... }` declares a test. Tests are
registered when the file runs but only execute in `--test` mode:

```bash
cargo run -- --test math_tests.dsq
```

```text
def min(input: List[Int]) -> Int {
    lowest = input[0]
    for i in input[1:] {
        if i < lowest { lowest = i }
    }
    lowest
}

test min_finds_lowest {
    assert_eq(min([4, 1, 9, -2]), -2)
}

test "min handles a single item" {
    assert_eq(min([7]), 7)
    assert(1 < 2)
}
```

Output:

```text
running 2 test(s)
test min_finds_lowest ... ok
test min handles a single item ... ok

test result: ok. 2 passed; 0 failed
```

Assertions are ordinary builtins (no `!` suffix):

| Call | Effect |
|---|---|
| `assert(condition)` | Fails unless `condition` is `True` |
| `assert(condition, message)` | Same, with a custom message |
| `assert_eq(left, right)` | Fails when the values differ, showing both |
| `assert_eq(left, right, message)` | Same, with a custom message |
| `assert_ne(left, right)` | Fails when the values are equal |

A failed assertion reports both values and the line:

```text
test math_works ... FAILED
  math_tests.dsq:line 12: assert_eq failed: left = 3, right = 4
```

Each test runs in its own scope, definitions in one test do not leak into
another, and duplicate test names are rejected. Normal script runs ignore test
blocks, so a test file can also be imported or executed without running tests.

"""

### Structs and methods

"""

```text
struct Point {
    x = 0
    y = 0
}

impl Point {
    def sum(self) {
        return self.x + self.y
    }

    def shift(self, dx, dy) {
        self.x = self.x + dx
        self.y = self.y + dy
        return self
    }
}

p = Point(1, 2)
print(p)            # Point(x=1, y=2)
print(p.sum())      # 3
p.x = 10            # fields are mutable
p.shift(1, 2)       # methods update the receiver
print(p)            # Point(x=11, y=4)
q = Point()         # both fields use their defaults
print(q)            # Point(x=0, y=0)
```

- `struct Name { field, field = default }` defines a fixed-field record and a
  constructor function with the same name. Fields without defaults must come
  before fields with defaults.
- `Name(...)` builds a value; the argument count is checked (`struct 'Point'
  takes between 0 and 2 argument(s)`).
- Reads and writes use `.`: `p.x`, `p.x = 10`. Unknown fields are errors.
- `impl Name { def method(self, ...) { ... } }` attaches methods. The first
  parameter must be named `self`; call them with `p.method(...)`.
- Methods mutate the receiver when called on a variable: `p.shift(1, 2)` writes
  the final `self` back into `p`. Calls on a temporary or a nested field (e.g.
  `make_point().shift(...)` or `outer.inner.shift(...)`) still work but the
  mutation is not written back.
- Structs are values: `q = p` copies the fields, so `q.x = 1` does not change
  `p`. Field assignment through the original variable does.
- There are no generic structs or visibility modifiers yet. Function and method
  parameters may be annotated, but struct fields cannot yet.

"""

### Traits

"""

A trait is a named set of methods. A method without a body is required; a
method with a body is a default that implementations inherit.

```text
trait Shape {
    def area(self)
    def describe(self) {
        print("area:", self.area())
    }
}

struct Circle {
    radius
}

struct Square {
    side
}

impl Shape for Circle {
    def area(self) {
        return 3 * self.radius * self.radius
    }
}

impl Shape for Square {
    def area(self) {
        return self.side * self.side
    }
    def describe(self) {
        print("square with side", self.side)
    }
}

c = Circle(2)
print(c.area())     # 12
c.describe()        # area: 12

s = Square(3)
print(s.area())     # 9
s.describe()        # square with side 3
```

- Declare with `trait Name { def method(self, ...) ... }`. Every trait method
  must start with `self`. A method with no `{ ... }` body is required.
- Implement with `impl TraitName for StructName { ... }`. All required methods
  must be supplied in that impl, and it cannot add methods the trait does not
  declare.
- Defaults can be inherited as-is or overridden by the implementation.
- Dispatch is dynamic: `value.method(...)` checks the struct's inherent `impl`
  methods first, then its trait methods. An inherent method therefore wins over
  a trait method with the same name.
- A trait can be implemented only once per struct, and two different traits
  cannot supply the same method name for one struct.
- Traits are interfaces plus defaults, not runtime types: a trait name can be
  used as a parameter annotation, but there are no `is` checks or trait objects.
  A trait with no methods is a marker.
- The trait and the struct must be defined before the `impl` runs.

"""

#### Trait-bounded functions

"""

Functions can require that their leading arguments are structs implementing a
trait. Type parameters go in `<...>` after the function name:

```text
def show<T: Shape>(value) {
    value.describe()
}

show(Circle(2))   # ok
show(42)          # error: argument 'value' must implement trait 'Shape'
```

- `def name<T: Trait>(...)` declares a type parameter and its bound. The type
  parameter is not a runtime variable; it only attaches a check to a value
  parameter.
- Type parameters bind to the leading value parameters by position:
  `def f<T: Shape, U: Shape>(first, second)` checks `first` and `second`
  against `Shape`. To constrain a later parameter, include an unbounded type
  parameter for each earlier one, e.g. `def f<T, U: Shape>(first, second)`.
- Type parameters without a bound (`<T>`) are accepted, but they do nothing at
  runtime.
- Bounds are checked at every call, after defaults have been filled in. Passing
  a non-struct or a struct that has not implemented the trait is an error at the
  call line.
- The bound trait must exist when the function is called. Defining the function
  before the trait is allowed as long as the trait exists by call time.
- Only free functions take type parameters for now; methods keep the
  `def method(self, ...)` form.

"""

### Macros

"""

Macros are real token macros: `macro` stores a token body, and each invocation
substitutes arguments before the expanded code is parsed. Invoke them with a
`$` after the name, like Rust's `!`:

```text
macro unless($cond, $body) {
    if not ($cond) {
        $body
    }
}

unless$(False, { print("runs") })   # prints runs
unless$(True,  { print("never") })

macro twice($body) {
    $body
    $body
}

twice$({ print("hi") })             # prints hi twice
```

- Define with `macro name($a, $b) { ... }`; parameters must start with `$`.
- Invoke in statement position as `name$(arg1, arg2)`. The `$` marks it as a
  macro; a bare `name(...)` is always a normal function call, so a macro and a
  function can share a name.
- Arguments are token groups. They may be expressions, blocks (`{ ... }`), or
  other fragments, and the expansion is parsed as D#² code.
- Substitution is textual: wrap expression parameters in parentheses inside the
  body when precedence matters (`not ($cond)`).
- There is no hygiene: names introduced by a macro land in the caller's scope.
- Arguments are evaluated every time they appear, so `twice$({ bump() })` runs
  `bump()` twice.
- Macros must be defined before use, expand only as statements, and recursive
  expansion stops after 32 levels.

"""

### Imports

"""

Split a program across files with `import`. A quoted path is used as written; an
unquoted path is shorthand for `path.dsq`. Relative imports search the
interpreter’s install directory first, then the importing file’s directory:

```text
# lib/math.dsq
def square(x) {
    return x * x
}

# main.dsq
import lib/math           # install dir, install/lib, then ./lib/math
# import "./lib/math"     # force a script-relative file

print(square(5))
```

- Search order for a bare import such as `import lib/math`:
  1. the directory containing the running interpreter executable
  2. that directory’s `lib/` subfolder
  3. the directory containing the file that has the `import`
- Use `./` or `../` in a quoted path to skip the install search and resolve only
  relative to the importing file. Absolute quoted paths are used as-is.
- When running with `cargo run`, the install directory is `target/debug` or
  `target/release`; for an installed binary it is the binary’s own directory.
- Imported files are parsed with the same macro table, so macros defined in an
  imported file are available after the `import`, and macros defined before the
  `import` are available inside the imported file.
- Imported statements run where the `import` appears, in the current scope.
  Put imports at the top level; there are no namespaces.
- A file runs at most once per program, even if it is imported more than once.
  Import cycles are rejected at parse time with a `cyclic import` error.
- Errors from imported files name the file:
  `lib/math.dsq:line 2: division by zero`.
- A path with no extension gets `.dsq` appended. Unquoted paths use `/` segments
  such as `lib/math`.

"""

### Operators

"""

Lowest to highest precedence:

| Operator | Meaning |
|---|---|
| `or`, `\|\|` | Logical or (short-circuits) |
| `and`, `&&` | Logical and (short-circuits) |
| `==`, `!=` | Equality / inequality |
| `>`, `>=`, `<`, `<=` | Ordering |
| `in`, `not in` | Membership tests for lists, texts, hashsets, hashmaps, and matrix rows |
| `+`, `-` | Addition/subtraction; `+` also joins two texts or concatenates two lists |
| `*`, `/`, `%` | Multiplication, division, remainder |
| `not`, `!`, unary `-` | Logical not, numeric negation |

```text
arithmetic = 1 + 2 * 3        # 7
compare = 1 > 2               # False
logic = True and not False    # True
joined = "a" + "b"            # "ab"
same = 1 == 1.0               # True
```

Notes: `and`/`or` require bools; comparisons work on numbers and texts; dividing
by zero is a runtime error; `1 == 1.0` is true; integer arithmetic is checked,
so overflow is an error rather than a wrap. `[0] * 3` repeats a list, and
`2 in [1, 2, 3]` tests membership.

"""

### Collections

"""

```text
nums = hashSet(1, 2, 2, 3)
items = [1, "two", [3]]
scores = {"a": 1, "b": 2}
scores["c"] = 3
grid = mat{row1: {a: 1, b: 2}, row2: {a: 3}}

print(nums)          # hashSet(1, 2, 3)
print(items)         # [1, "two", [3]]
print(scores)        # {"a": 1, "b": 2, "c": 3}
print(scores["b"])   # 2
print(grid)          # mat{row1: {a: 1, b: 2}, row2: {a: 3}}
print("abc"[-1])     # c
```

**Lists.** `[value, ...]` creates a Python-style mutable list and `[]` is the
empty list. Lists may mix types, nest, and are shared references: `b = a` makes
`b` point at the same list, so `b.append(4)` is visible through `a`.

```text
a = [1, 2, 3, 4, 5]
print(a[0], a[-1])        # 1 5
print(a[1:3])             # [2, 3]
print(a[::-1])            # [5, 4, 3, 2, 1]
print(a[::2])             # [1, 3, 5]

a[0] = 9
a[1:3] = [7, 8]
del a[0]
a.append(6)
a.extend([10, 11])
a.insert(1, 4)
a.remove(8)
last = a.pop()
a.sort()
a.reverse()
copy = a.copy()
print(len(a), 2 in a, 4 not in a)
```

Slices use `start:stop:step`, and any part may be omitted (`a[:]`, `a[::2]`,
`a[::-1]`). Indexing and slicing accept negative positions. Slice assignment
replaces a range; extended slices such as `a[::2] = [...]` require the same
number of items. `del a[i]` and `del a[start:stop]` remove items in place.
Text also supports slicing, for example `"abc"[1:]` is `"bc"`.

List methods: `append`, `extend`, `insert`, `remove`, `pop`, `clear`, `index`,
`count`, `sort`, `reverse`, and `copy`. `len(a)` reports the length, and
`list(iterable)` builds a list from a list, text, hashset, hashmap keys, or
matrix row names. `a + b` concatenates lists and `a * n` repeats one. `sort()`
works on lists containing only numbers, only text, or only bools.

**Typed lists.** `List[Int](...)` builds a list that only accepts `Int` items.
The element type is checked at runtime on construction and on every mutation,
including `append`, `extend`, `insert`, index assignment, and slice assignment.
Nested types work too: `List[List[Int]]`, `List[Text]`, `List[Person]`, and
`List[Named]` for a trait.

```text
scores = List[Int](1, 2, 3)
scores.append(4)             # ok
scores.append("five")        # error: expected Int, got text

rows = List[List[Int]]([[1, 2], [3]])
rows[0].append(4)            # [[1, 2, 4], [3]]

names = List[Text]("Ada", "Linus")
```

`List[Int](iterable)` expands a single list, text, hashset, hashmap, or matrix
argument like `list(iterable)`. Slicing, `copy()`, and `*` keep the element
type. A function annotated `values: List[Int]` also promotes an untyped list
argument to `Int`, so later mutations through the same list are checked. Bare
`List` only requires a list and leaves its items mixed.

**Hashsets.** `hashSet(...)` builds a set from its arguments (`set(...)` and
`hashset(...)` are aliases); duplicates collapse, and `hashSet()` is empty.
Items must be Int, Float, Bool, or Text — nesting a list, hashset, hashmap, or
matrix is an error. Values print sorted, but iteration order is the hash order.

**Hashmaps.** `{ key: value, ... }` is a Python-style dictionary. Keys are
expressions and may be int, float, bool, or text; values are any expression.
Keys are written as expressions, so `{"a": 1}` uses the text key `"a"` while
`{a: 1}` looks up a variable named `a`:

```text
d = {"a": 1, 2: "two", True: 3}
d["b"] = 4
print(d["a"], d[2], d[True], d["b"])
```

Read and write entries with `[ ]`. A missing key is an error
(`hashmap key "zz" not found`), and index assignment needs a variable on the
left (`d["a"] = 1`); chained writes like `d["a"]["b"] = 1` are not supported.
Because `{ }` at the start of a statement means a block, write an empty map as
`x = {}` (expression position), not on its own line.

**Matrices.** `mat{...}` (also `matrix{...}` / `matrex{...}`) holds named rows
of named cells, shaped `name: { cell: value, ... }`. Commas between rows and
cells are optional, and a trailing comma is allowed. Row and cell names are
identifiers or text literals, and values may themselves be matrices:

```text
weird = mat{
    "row 1": {"a b": 1,},
    outer: {inner: mat{x: {y: 2}}},
}
```

**Indexing and iteration.** `[ ]` indexes lists and texts (`"abc"[0]` is `"a"`,
`"abc"[-1]` is `"c"`) and reads/writes hashmap entries. Lists and texts also
support slicing. `for` iterates list items, hashset items, hashmap keys, matrix
row names, or text characters. Membership tests (`in` / `not in`) work on the
same containers.

"""

### Conversions

"""

Every value can be converted explicitly with a builtin named after its target
type. The lowercase names are Pythonic; the capitalized forms are aliases.
Conversions never guess: a value that cannot sensibly become the target raises
a clear error instead of being silently coerced.

| Builtin | Aliases | Result |
|---|---|---|
| `int(x)` | `Int` | Int; floats truncate toward zero, bools become `1`/`0`, text must be a whole number |
| `float(x)` | `Float` | Float; ints and bools widen, numeric text is parsed |
| `bool(x)` | `Bool` | Python-style truthiness for every value |
| `str(x)` | `text`, `Text` | Text; any value uses its display form, `str()` is `""` |
| `set(x)` | `hashSet`, `HashSet`, `Set` | Hashset; one iterable argument expands, otherwise items are taken as given |
| `dict(x)` | `hashMap`, `HashMap`, `Dict` | Hashmap; from a hashmap, matrix, struct, or `[key, value]` pairs |
| `matrix(x)` | `Matrix`, `mat`, `matrex` | Matrix; from a matrix, nested hashmap, struct, or `[row, cells]` pairs |
| `list(x)` | — | List; expands text characters or any iterable/container |

```text
int(3.9)            # 3
int("42")           # 42
float("2.5")        # 2.5
bool([])            # False
bool([0])           # True
str(42)             # "42"
str([1, "two"])     # "[1, \"two\"]"

set([3, 1, 3, 2])   # hashSet(1, 2, 3)
set("aba")          # hashSet("a", "b")
dict([["a", 1]])    # {"a": 1}
matrix({"row1": {"a": 1}})   # mat{row1: {a: 1}}
```

Calling a conversion with no argument yields the empty value: `int()` is `0`,
`float()` is `0.0`, `bool()` is `False`, `str()` is `""`, and `set()`, `dict()`,
`matrix()`, and `list()` are empty.

**Truthiness.** `bool` follows Python: `0`, `0.0`, `""`, empty collections,
empty matrices, and `None` are false; non-zero numbers, non-empty text, and
non-empty collections are true. Structs are always true.

**Collections.** With one iterable argument, `set` expands it — lists, hashsets,
hashmap keys, matrix row names, and text characters. With several arguments,
each argument becomes one item: `set(1, 2)` is `hashSet(1, 2)`. `dict` accepts a
hashmap, a matrix (rows become nested hashmaps), a struct (fields become keys),
or a list of `[key, value]` pairs. `matrix` accepts a matrix, a nested hashmap
`{row: {cell: value}}`, a struct (one row named after the type), or a list of
`[row, cells]` pairs where `cells` is a hashmap.

**Errors.** Impossible conversions stop the script with a message naming both
sides, for example:

```text
ERROR: line 1: cannot convert text "abc" to Int
ERROR: line 1: cannot convert list to Float
ERROR: line 1: cannot convert int to a hashmap
ERROR: line 1: matrix() row 'row' must be a hashmap
```

Text conversions tolerate surrounding whitespace, so `int(" 42 ")` is `42`.
Floats that are not finite or that fall outside the 64-bit Int range are
rejected by `int`. `dict` and `matrix` take at most one argument.

"""

### Console

"""

The `console` methods come from `src/builtin_functions.json`:

| Method | Effect |
|---|---|
| `console.println(x)` | Print `x` to stdout with a newline |
| `console.print(x)` | Print `x` to stdout without a newline (flushed) |
| `console.debug(x)` | Print `[debug] x` to stderr |
| `console.warn(x)` | Print `x` to stderr |
| `console.error(x)` | Print `x` to stderr |
| `console.input()` | Read one line as Text; at end of input it is an error |
| `console.clear()` | Clear the terminal (ANSI escape) |

Python-style aliases are available when no user function has that name:

| Builtin | Effect |
|---|---|
| `print(a, b, ...)` | Print the arguments separated by spaces, then a newline |
| `input()` | Read one line as Text |

```text
print("Name?")
name = input()
print("Hi " + name)
```

Printing an empty line: `print("")` (or `console.println("")`).

**Rust-style formatted output.** `print!`, `println!`, `eprint!`, and
`eprintln!` take a format string followed by values. `{}` prints the next value,
`{:?}` prints its debug/repr form (text quoted inside collections), and an
explicit index such as `{0}` or `{1:?}` selects an argument. Escapes `{{` and
`}}` produce literal braces.

```text
name = "Nirab"
scores = [91, 88, 95]

println!("Hello, {}!", name)              # Hello, Nirab!
println!("scores: {:?}", scores)          # scores: [91, 88, 95]
println!("{1} then {0}", "first", "second")  # second then first
println!("literal braces: {{}}")          # literal braces: {}
print!("no newline")
eprintln!("error: {}", "something failed")
```

The format string must be text, every argument must be used, and a placeholder
must have a matching argument. Only `{}` and `{:?}` are supported; width and
alignment specifiers such as `{:>5}` are rejected with an error.

"""

### Files

"""

`file(path)` creates a `File` object for a path written as Text:

```text
f = file("notes.txt")
f.create()
f.write("hello")
f.append(" world")

print(f.read())       # hello world
print(f.size())       # 11
print(f.exists())     # True

f.copy("backup.txt")  # returns the number of bytes copied
f.rename("renamed.txt")
f.delete()
```

| Method | Effect |
|---|---|
| `f.read()` | Return the whole file as Text |
| `f.write(data)` | Create or replace the file with Text `data` |
| `f.append(data)` | Append Text `data`, creating the file if needed |
| `f.delete()` | Remove the file |
| `f.copy(dest)` | Copy to `dest`; returns the copied byte count as Int |
| `f.rename(new)` | Rename the file and update the object's path |
| `f.exists()` | Return `True` or `False` |
| `f.create()` | Create or truncate the file |
| `f.size()` | Return the file size in bytes as Int |

- Paths must be Text; `file(1)` is an error.
- `write`, `append`, and `copy` data/destinations must be Text.
- Failures name the operation and path, for example
  `file.read() failed for 'missing.txt': No such file or directory`.
- `rename` updates the receiver when it is a plain variable, so
  `f.rename("new.txt")` then `f.read()` reads the new path. Other file
  operations do not need write-back.
- As with struct methods, a renamed temporary such as
  `file("a.txt").rename("b.txt")` renames the file but the temporary is gone.

"""

### Graphics

"""

`gl` and `vk` open real desktop windows. Both backends use SDL2 for the window
and event loop; OpenGL uses a 3.3 core context through `glow`, and Vulkan uses
`ash` with a swapchain and render pass. They are independent, so close one
before opening the other.

**OpenGL**

For a window that only needs a solid background, `frame` handles event polling,
clearing, and buffer swapping in one call:

```text
gl.init("D#2 OpenGL", 800, 600)
while gl.frame(0.1, 0.1, 0.15) { }
gl.close()
```

The optional fourth color value is alpha and defaults to `1.0`. Use the
lower-level calls below when you need to draw objects.

```text
gl.init("D#2 OpenGL", 800, 600)
gl.clearColor(0.1, 0.1, 0.15, 1.0)

vertex = gl.createShader("vertex", "#version 330 core\n\
layout (location = 0) in vec2 position;\n\
void main() { gl_Position = vec4(position, 0.0, 1.0); }\n")
fragment = gl.createShader("fragment", "#version 330 core\n\
out vec4 color;\n\
void main() { color = vec4(1.0, 0.5, 0.2, 1.0); }\n")

program = gl.createProgram(vertex, fragment)
gl.useProgram(program)

vao = gl.createVertexArray()
gl.bindVertexArray(vao)
buffer = gl.createBuffer()
gl.bindBuffer("array", buffer)
gl.bufferData("array", [-0.5, -0.5, 0.5, -0.5, 0.0, 0.5], "static")
gl.enableVertexAttribArray(0)
gl.vertexAttribPointer(0, 2, "float", False, 0, 0)

while not gl.shouldClose() {
    gl.poll()
    gl.clear()
    gl.drawArrays("triangles", 0, 3)
    gl.present()
}

gl.deleteBuffer(buffer)
gl.deleteVertexArray(vao)
gl.deleteProgram(program)
gl.deleteShader(vertex)
gl.deleteShader(fragment)
gl.close()
```

Core OpenGL calls:

| Method | Effect |
|---|---|
| `gl.init(title, width, height)` | Create the window and OpenGL context |
| `gl.frame(r, g, b[, a])` | Poll events, clear to a color, swap buffers; returns `False` when closed |
| `gl.poll()` / `gl.shouldClose()` | Pump events; check whether the window was closed |
| `gl.present()` | Swap buffers |
| `gl.close()` | Destroy the window and context |
| `gl.setTitle(text)` / `gl.size()` | Change the title; read `[width, height]` |
| `gl.clearColor(r, g, b, a)` | Set the clear colour (0.0–1.0 floats) |
| `gl.clear(mask)` | Clear `"color"`, `"depth"`, `"stencil"`, or a `"color \| depth"` combination |
| `gl.viewport(x, y, w, h)` | Set the viewport |
| `gl.enable(name)` / `gl.disable(name)` / `gl.blendFunc(a, b)` / `gl.depthFunc(fn)` / `gl.cullFace(face)` | Render state |
| `gl.getError()` / `gl.errorName(code)` | Read the last GL error |
| `gl.getString("vendor" \| "renderer" \| "version" \| "glsl")` | Driver information |
| `gl.createShader(stage, source)` | Compile `"vertex"` or `"fragment"` GLSL; returns a handle |
| `gl.createProgram(vertex, fragment)` | Link two shader handles; returns a program handle |
| `gl.useProgram(handle)` | Bind a program |
| `gl.createBuffer()` / `gl.createVertexArray()` | Allocate GL objects; return Int handles |
| `gl.bindBuffer(target, handle)` | `target` is `"array"`, `"element"`, or `"uniform"` |
| `gl.bufferData(target, numbers, usage, type)` | Upload `f32`, `u32`, `i32`, `u16`, or `u8` data; `usage` is `"static"`, `"dynamic"`, or `"stream"` |
| `gl.bindVertexArray(handle)` / `gl.enableVertexAttribArray(index)` | Bind a VAO and enable attributes |
| `gl.vertexAttribPointer(index, size, type, normalized, stride, offset)` | Describe an attribute |
| `gl.drawArrays(mode, first, count)` / `gl.drawElements(mode, count, type, offset)` | Draw; modes include `"triangles"`, `"lines"`, and `"points"` |
| `gl.getUniformLocation(program, name)` | Look up a uniform |
| `gl.uniform1f` … `gl.uniform4f` / `gl.uniform1i` | Set scalar and vector uniforms |
| `gl.uniformMatrix4fv(location, values)` | Upload a list of 16 floats |

Every object-returning call returns an `Int` handle. Delete objects with the
matching `gl.deleteShader`, `gl.deleteProgram`, `gl.deleteBuffer`, or
`gl.deleteVertexArray` call.

**Vulkan**

The same one-call loop works for a cleared Vulkan window:

```text
vk.init("D#2 Vulkan", 800, 600)
while vk.frame(0.1, 0.2, 0.35) { }
vk.close()
```

The optional fourth color value is alpha and defaults to `1.0`.

```text
vk.init("D#2 Vulkan", 800, 600)
vk.clearColor(0.1, 0.2, 0.35, 1.0)
print(vk.deviceName())
print(vk.apiVersion())

while not vk.shouldClose() {
    vk.poll()
    vk.present()
}

vk.close()
```

| Method | Effect |
|---|---|
| `vk.init(title, width, height)` | Create the window, instance, device, swapchain, and render pass |
| `vk.frame(r, g, b[, a])` | Poll events, clear to a color, present; returns `False` when closed |
| `vk.poll()` / `vk.shouldClose()` | Pump events; check whether the window was closed |
| `vk.present()` / `vk.clear()` | Clear to the current colour and present the frame |
| `vk.clearColor(r, g, b, a)` | Set the clear colour |
| `vk.setTitle(text)` / `vk.size()` | Change the title; read `[width, height]` |
| `vk.deviceName()` / `vk.apiVersion()` | Selected GPU and Vulkan version |
| `vk.waitIdle()` | Wait for GPU work to finish |
| `vk.close()` | Destroy all Vulkan objects and the window |

The Vulkan backend currently renders a cleared frame. Raw pipeline, shader
module, buffer, and draw calls are listed as `TODO` in
`src/builtin_functions.json`; the OpenGL backend is the complete drawing path
today.

Graphics calls need a desktop session (`DISPLAY` or `WAYLAND_DISPLAY`) plus the
SDL2, OpenGL, and Vulkan runtime libraries. Each backend allows one open window
at a time. A script error leaves the window open until the process exits, so
close windows explicitly when you are done.

"""

## Migrating from the earlier Rust-like syntax

"""

| Old | New |
|---|---|
| `let x = 1` | `x = 1` |
| `let mut y = 2` / `mut y = 2` | `y = 2` |
| `const C = 4`, `static S = 5` | `C = 4`, `S = 5` (plain names) |
| `fn double(x) { ... }` | `def double(x) { ... }` |
| `true` / `false` | `True` / `False` (lowercase still accepted) |
| `void` | `None` |
| `a is b` / `a is_not b` | `a == b` / `a != b` |
| `a greater b`, `a less_equal b`, ... | `a > b`, `a <= b`, ... |
| `else if` | `elif` (`else if` still parses as nested `if`) |
| `hashset 1 2 3` | `hashSet(1, 2, 3)` (`set(...)`/`hashset(...)` also work) |
| `matrix row1 { a 1 b 2 }` | `mat{row1: {a: 1, b: 2}}` |
| `-- comment` | `# comment` |
| `type Id = int` | removed |

"""

## REPL

"""

`cargo run` with no arguments starts the prompt:

```text
> total = 0
> for n in hashSet(1, 2, 3) {
    total = total + n
  }
> print(total)
6
> quit
```

- One environment for the whole session, so values persist between inputs.
- Blocks can span several lines: the continuation prompt is `  `.
- Leave with `quit`, `exit`, or `Ctrl-D`.
- Empty lines just re-prompt.
- Piped input works too: `printf 'print("hi")\n' | cargo run`.

"""

## Errors

"""

Parsing and runtime errors stop the script and report the source line:

```text
ERROR: line 2: division by zero
ERROR: line 4: condition must be a bool, got int
ERROR: line 2: unknown variable 'missing'
ERROR: line 2: function 'f' takes 2 argument(s), but 1 given
ERROR: line 1: unexpected 'x' after a statement (missing a newline or ';'?)
```

The REPL prints the same format and keeps going.

"""

## Project layout

"""

| Path | Purpose |
|---|---|
| `src/main.rs` | Entry point: script argument or REPL |
| `src/lexer.rs` | Source text → tokens with line numbers, comments |
| `src/error.rs` | `Error { line, message }` |
| `src/parser/ast.rs` | Expression/statement tree |
| `src/parser/parsers.rs` | Recursive-descent parser (precedence climbing) |
| `src/runner/run.rs` | Environments, interpreter, built-ins, REPL |
| `src/graphics.rs` | `gl.*` OpenGL (SDL2 + glow) and `vk.*` Vulkan (SDL2 + ash) backends |
| `src/value.rs` | `Value` / `SetItem` types and display rules |
| `src/builtin_functions.json` | Planned language keyword/built-in → Rust mapping |
| `.githooks/` | Versioned git hooks that sync `HEAD` to `origin/test` |
| `.github/workflows/` | CI that mirrors pushed branches to `test` |
| `scripts/` | Test-branch sync and hook installation scripts |

"""

## Roadmap

"""

`builtin_functions.json` lists the intended keyword set. Progress:

- Done, converted to Pythonic syntax: variables (plain assignment + `del`),
  control flow (`if`/`elif`/`else`/`while`/`loop`/`for`/`in`/`break`/`continue`),
  functions (`def`/`return`, defaults, recursion), operators (`and`/`or`/`not`
  plus symbols), `True`/`False`/`None`, `#` comments, and collection literals
  (`hashSet(...)`, `{key: value}` hashmaps with `[ ]` indexing, `mat{...}`).
- Done: `struct` records with defaults, `impl` methods with `self`, traits
  (`trait`/`impl Trait for Type`, required and default methods), trait-bounded
  functions (`def name<T: Trait>(value)`), statement token macros
  (`macro name($a) { ... }`, called with `name$(...)`), and multifile imports
  (`import "file.dsq"` / `import name`).
- Done: explicit conversions between values: `int`, `float`, `bool`, `str`,
  `list`, `set`, `dict`, and `matrix` (with `Int`, `Float`, `Bool`, `Text`,
  `hashSet`, `hashMap`, `Matrix`, and related aliases).
- Done: file objects from `file(path)` with `read`, `write`, `append`, `delete`,
  `copy`, `rename`, `exists`, `create`, and `size`.
- Done: desktop graphics through `gl.*` (OpenGL 3.3 core, including shaders,
  buffers, vertex arrays, draws, and uniforms) and `vk.*` (Vulkan window,
  swapchain, clear, and present).
- Next: `match`/enums, closures/lexical scoping, namespaced modules, then
  `try`/`except`/`raise`.
- Built-in methods: `console.*`, `file.*`, `gl.*`, and `vk.*` (plus
  `print`/`input`) are implemented; the `data`, `collections`, `query`,
  `matrex`, `time`, `math`, `game`, and `system` groups are not wired up yet.

"""

## Known limitations

"""

- No namespaced modules: `import` includes a file's definitions in the current
  scope. There is no `module.name` access or selective `from ... import ...`.
  Put imports at the top level; imported files run at most once.
- No `match`/enums, `try`/`except`, or lambdas. `{...}`
  is a hashmap literal; `mat{...}` is the matrix type.
- No generic structs or generic methods: type parameters with trait bounds are
  only supported on free functions, and they map to leading value parameters by
  position. Parameter annotations and trait bounds are call-time checks, not
  static types.
- Traits are interfaces with defaults, not types: there are no runtime `is`
  checks or trait objects. Two traits cannot provide the same method name for
  one struct; there is no explicit disambiguation syntax.
- No ownership, borrowing, or static types: this is a dynamic interpreter, so
  Rust's borrow checker has no counterpart here.
- Functions are dynamically scoped: a called function sees the *caller's*
  variables. There are no closures and no first-class function values yet.
- No named/keyword arguments at call sites; defaults are positional. In
  particular, `list.sort()` has no `key=` or `reverse=` arguments.
- No list comprehensions, tuple unpacking (`a, b = [1, 2]`), or range objects;
  build lists with literals, `list(iterable)`, loops, and `append`/`extend`.
- Lists support equality and membership, but ordering comparisons such as
  `[1] < [2]` are not implemented.
- Macros are textual and unhygienic; they only expand in statement position and
  must be invoked with `name$(...)`.
- The Vulkan backend clears and presents a swapchain image but does not yet
  expose shader modules, graphics pipelines, buffers, or draw calls. Use
  `gl.*` for complete drawing today.
- Struct methods write mutations back only when called on a plain variable;
  nested receivers such as `outer.inner.shift(...)` do not propagate the change.
  List methods mutate shared list references, so `outer[0].append(x)` does work.
- Struct names are constructors, not first-class values: `C = Point` does not
  work; call `Point(...)` directly.
- Numeric literals cannot start with a dot (`0.5` works, `.5` does not).
- A map literal cannot start a statement (that position is a block); assign it
  to a name instead (`d = {}`).
- Index assignment, slice assignment, and `del` only target a variable base
  (`d["a"] = 1`, `a[0] = 1`, `del a[0]`); chained writes like
  `d["a"]["b"] = 1` are not supported.
- Hashmap display is sorted, not insertion-ordered.
- `builtin_functions.json` is larger than the implemented surface (see above).

"""
