# D#² (D-sharp-squared)

A small, interpreted programming language implemented in Rust. The keywords are
Pythonic (`def`, `if`/`elif`/`else`, `x = 1`, `True`/`False`/`None`, `#`
comments) but blocks use braces instead of indentation. It has a real lexer and
recursive-descent parser, functions with defaults and recursion, loops, two
compound value types (`hashset` and `matrix`), console built-ins, and a REPL.

It is still an early prototype: no classes, modules, `match`, or closures yet —
see [Roadmap](#roadmap) and [Known limitations](#known-limitations).

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
| Hashset | `hashset 1 2 2` / `set 1 2` | Duplicates collapse; scalars only |
| Matrix | `mat{row1: {a: 1, b: 2}}` | Named rows of named cells; `matrix`/`matrex` are aliases |
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
- `for name in <value>` iterates a `hashset` (its items, unordered), a `matrix`
  (row names, sorted), or `text` (characters).
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
nums = hashset 1 2 2 3
grid = mat{row1: {a: 1, b: 2}, row2: {a: 3}}
print(nums)
print(grid)
```

prints:

```text
hashset 1 2 3
mat{row1: {a: 1, b: 2}, row2: {a: 3}}
```

A matrix is braces after `mat` (or `matrix` / `matrex`), with rows shaped
`name: { cell: value, ... }`. Commas between rows and cells are optional, and a
trailing comma is allowed. Row and cell names are identifiers or text literals,
and values may themselves be matrices:

```text
weird = mat{
    "row 1": {"a b": 1,},
    outer: {inner: mat{x: {y: 2}}},
}
```

The older form is still accepted: `matrix row1 { a 1 b 2 } row2 { a 3 }`.
Hashset elements must be Int, Float, Bool, or Text — nesting a hashset or matrix
inside a hashset is an error. Values inside a hashset print in sorted order, but
iteration order is the hash order.

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
| `matrix row1 { a 1 b 2 }` | `mat{row1: {a: 1, b: 2}}` (old form still accepted) |
| `-- comment` | `# comment` |
| `type Id = int` | removed |

## REPL

`cargo run` with no arguments starts the prompt:

```text
> total = 0
> for n in hashset 1 2 3 {
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
  plus symbols), `True`/`False`/`None`, and `#` comments.
- Next: `match`, real closures/lexical scoping, then `class`/`struct`/`impl`,
  modules, and `try`/`except`/`raise`.
- Built-in methods: only `console.*` (plus `print`/`input`) is implemented; the
  `file`, `data`, `collections`, `query`, `matrex`, `time`, `math`, `game`, and
  `system` groups are not wired up yet.

## Known limitations

- No classes, modules, `match`, `try`/`except`, lambdas, or dict literals
  (`mat{...}` is the matrix type, not a general dictionary).
- Functions are dynamically scoped: a called function sees the *caller's*
  variables. There are no closures and no first-class function values yet.
- No named/keyword arguments at call sites; defaults are positional.
- No imports.
- Numeric literals cannot start with a dot (`0.5` works, `.5` does not).
- `hashset` still uses a keyword prefix (`hashset 1 2 3`); matrices use
  `mat{...}`, but braces cannot also serve as dict literals because `{ }` means
  "block" here.
- `builtin_functions.json` is larger than the implemented surface (see above).
