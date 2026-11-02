# D#² (D-sharp-squared)

A small, interpreted programming language implemented in Rust. The keywords are
Pythonic (`def`, `if`/`elif`/`else`, `x = 1`, `True`/`False`/`None`, `#`
comments) but blocks use braces instead of indentation. It has a real lexer and
recursive-descent parser, functions with defaults and recursion, loops,
hashsets, hashmaps, matrices, structs with `impl` methods, traits and
trait-bounded functions, token macros, console built-ins, and a REPL.

It is still an early prototype: no modules, `match`/enums, or closures yet — see
[Roadmap](#roadmap) and [Known limitations](#known-limitations).

- **v1.1.0** — current release (2026-11-01): `hashSet(...)`, `{...}` hashmaps
  with `[ ]` indexing, text indexing, and `mat{...}`-only matrices.
- **v1.0.0** — first stable release (2026-11-01); see
  [CHANGELOG.md](CHANGELOG.md). The earlier tags
  (`PreReleace1OfV1.0`, `PreReleace2OfV1`) were pre-releases.
- No external crates (std only)
- Rust 2024 edition (Rust 1.85+)
- Source: <https://github.com/nirabTBUK1234/D-sharp-squared-programing-language>

## Build and run

```bash
cargo build
cargo run -- program.dsq   # run a script (any file extension works)
cargo run                  # start the REPL
cargo test                 # run the test suite
```

## Quick start

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

## Language reference

### Statements

- Statements end at a newline or `;`.
- `{ ... }` groups statements into a block.
- `#` starts a comment that runs to the end of the line.
- Text literals use `"double"` or `'single'` quotes and may contain spaces, `;`,
  and braces. Escapes: `\n`, `\t`, `\r`, `\\`, `\"`, `\'`.
- A stray token after a statement is an error rather than being silently
  ignored (it catches leftovers from the old `let` syntax):
  `unexpected 'x' after a statement (missing a newline or ';'?)`.

### Values

| Type | Examples | Notes |
|---|---|---|
| Int | `42`, `-7` | 64-bit signed |
| Float | `3.14`, `-0.5` | 64-bit |
| Bool | `True`, `False` | `true`/`false` are also accepted |
| Text | `"hello"`, `'hi there'` | |
| Hashset | `hashSet(1, 2, 2)` / `set(1, 2)` | Duplicates collapse; scalars only |
| Hashmap | `{"a": 1, "b": 2}` | Keys can be int, float, bool, or text; read/write with `[ ]` |
| Matrix | `mat{row1: {a: 1, b: 2}}` | Named rows of named cells; `matrix`/`matrex` are aliases |
| Struct | `Point(1, 2)` | Fixed named fields; declared with `struct`, methods with `impl`, behavior with traits |
| None | `None` | Returned by functions that do not return a value |

### Variables

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

### Control flow

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
- `for name in <value>` iterates a `hashSet` (its items), a `hashmap` (its keys),
  a `matrix` (row names, sorted), or `text` (characters).
- `break` and `continue` only work inside a loop.
- `loop { }` is a convenience for `while True` and is the one non-Python
  control-flow keyword kept as an extension.

### Functions

```text
def greet(name, punctuation = "!") {
    return "Hello, " + name + punctuation
}

def fact(n) {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}

print(greet("Nirab"))          # Hello, Nirab!
print(greet("Nirab", "?"))     # Hello, Nirab?
print(fact(5))                 # 120
```

- Parameters come before the optional defaults; a required parameter may not
  follow one with a default.
- Defaults are evaluated when the function is called, in the function's scope,
  so they can refer to earlier parameters.
- `return` ends the function and returns a value. Bare `return`, or falling off
  the end, gives `None`.
- Recursion works (`fact`, `fib`, mutual recursion).
- A `def` inside a block is still a plain assignment in that scope, so it stays
  visible after the block ends.
- Arity is checked: `function 'f' takes 2 argument(s), but 3 given`.

A call scope is created per call; there are no closures yet (see limitations).

### Structs and methods

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
- There are no generic structs, visibility modifiers, or type annotations yet.

### Traits

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
- Traits are interfaces plus defaults, not runtime types: there are no type
  annotations or `is` checks. A trait with no methods is a marker.
- The trait and the struct must be defined before the `impl` runs.

#### Trait-bounded functions

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

### Macros

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

### Operators

Lowest to highest precedence:

| Operator | Meaning |
|---|---|
| `or`, `\|\|` | Logical or (short-circuits) |
| `and`, `&&` | Logical and (short-circuits) |
| `==`, `!=` | Equality / inequality |
| `>`, `>=`, `<`, `<=` | Ordering |
| `+`, `-` | Addition/subtraction; `+` also joins two texts |
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
so overflow is an error rather than a wrap.

### Collections

```text
nums = hashSet(1, 2, 2, 3)
scores = {"a": 1, "b": 2}
scores["c"] = 3
grid = mat{row1: {a: 1, b: 2}, row2: {a: 3}}

print(nums)          # hashSet(1, 2, 3)
print(scores)        # {"a": 1, "b": 2, "c": 3}
print(scores["b"])   # 2
print(grid)          # mat{row1: {a: 1, b: 2}, row2: {a: 3}}
print("abc"[-1])     # c
```

**Hashsets.** `hashSet(...)` builds a set from its arguments (`set(...)` and
`hashset(...)` are aliases); duplicates collapse, and `hashSet()` is empty.
Items must be Int, Float, Bool, or Text — nesting a hashset, hashmap, or matrix
is an error. Values print sorted, but iteration order is the hash order.

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

**Indexing and iteration.** `[ ]` also indexes text (`"abc"[0]` is `"a"`,
`"abc"[-1]` is `"c"`). `for` iterates hashset items, hashmap keys, matrix row
names, or text characters.

### Console

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

## Migrating from the earlier Rust-like syntax

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

## REPL

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

## Errors

Parsing and runtime errors stop the script and report the source line:

```text
ERROR: line 2: division by zero
ERROR: line 4: condition must be a bool, got int
ERROR: line 2: unknown variable 'missing'
ERROR: line 2: function 'f' takes 2 argument(s), but 1 given
ERROR: line 1: unexpected 'x' after a statement (missing a newline or ';'?)
```

The REPL prints the same format and keeps going.

## Project layout

| Path | Purpose |
|---|---|
| `src/main.rs` | Entry point: script argument or REPL |
| `src/lexer.rs` | Source text → tokens with line numbers, comments |
| `src/error.rs` | `Error { line, message }` |
| `src/parser/ast.rs` | Expression/statement tree |
| `src/parser/parsers.rs` | Recursive-descent parser (precedence climbing) |
| `src/runner/run.rs` | Environments, interpreter, built-ins, REPL |
| `src/value.rs` | `Value` / `SetItem` types and display rules |
| `src/builtin_functions.json` | Planned language keyword/built-in → Rust mapping |

## Roadmap

`builtin_functions.json` lists the intended keyword set. Progress:

- Done, converted to Pythonic syntax: variables (plain assignment + `del`),
  control flow (`if`/`elif`/`else`/`while`/`loop`/`for`/`in`/`break`/`continue`),
  functions (`def`/`return`, defaults, recursion), operators (`and`/`or`/`not`
  plus symbols), `True`/`False`/`None`, `#` comments, and collection literals
  (`hashSet(...)`, `{key: value}` hashmaps with `[ ]` indexing, `mat{...}`).
- Done: `struct` records with defaults, `impl` methods with `self`, traits
  (`trait`/`impl Trait for Type`, required and default methods), trait-bounded
  functions (`def name<T: Trait>(value)`), and statement token macros
  (`macro name($a) { ... }`, called with `name$(...)`).
- Next: `match`/enums, closures/lexical scoping, modules (`use`/`import`), then
  `try`/`except`/`raise`.
- Built-in methods: only `console.*` (plus `print`/`input`) is implemented; the
  `file`, `data`, `collections`, `query`, `matrex`, `time`, `math`, `game`, and
  `system` groups are not wired up yet.

## Known limitations

- No modules, `match`/enums, `try`/`except`, or lambdas. `{...}`
  is a hashmap literal; `mat{...}` is the matrix type.
- No generic structs or generic methods: type parameters with trait bounds are
  only supported on free functions, and they map to leading value parameters by
  position. They are call-time checks, not static types.
- Traits are interfaces with defaults, not types: there are no type annotations,
  runtime `is` checks, or trait objects. Two traits cannot provide the same
  method name for one struct; there is no explicit disambiguation syntax.
- No ownership, borrowing, or static types: this is a dynamic interpreter, so
  Rust's borrow checker has no counterpart here.
- Functions are dynamically scoped: a called function sees the *caller's*
  variables. There are no closures and no first-class function values yet.
- No named/keyword arguments at call sites; defaults are positional.
- No imports.
- Macros are textual and unhygienic; they only expand in statement position and
  must be invoked with `name$(...)`.
- Methods write mutations back only when called on a plain variable; nested
  receivers such as `outer.inner.shift(...)` do not propagate the change.
- Struct names are constructors, not first-class values: `C = Point` does not
  work; call `Point(...)` directly.
- Numeric literals cannot start with a dot (`0.5` works, `.5` does not).
- A map literal cannot start a statement (that position is a block); assign it
  to a name instead (`d = {}`).
- Index assignment only targets a variable base (`d["a"] = 1`); chained writes
  like `d["a"]["b"] = 1` are not supported.
- Hashmap display is sorted, not insertion-ordered.
- `builtin_functions.json` is larger than the implemented surface (see above).
