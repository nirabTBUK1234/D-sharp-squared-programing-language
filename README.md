# D#² (D-sharp-squared)

A small, interpreted programming language implemented in Rust. It has a real
lexer and recursive-descent parser, expressions and operators, `if`/`else`,
`while`/`loop`/`for` loops, `def` (or `fn`) functions with defaults and
`return`, block scoping, variables, two compound value types (`hashset` and
`matrix`), console built-ins, and a REPL.

It is still an early prototype: no classes, modules, `match`, or closures yet —
see [Roadmap](#roadmap) and [Known limitations](#known-limitations).

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
let mut i = 1
while i less_equal 15 {
    if i % 15 is 0 {
        console.println("FizzBuzz")
    } else if i % 3 is 0 {
        console.println("Fizz")
    } else if i % 5 is 0 {
        console.println("Buzz")
    } else {
        console.println(i)
    }
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
- Blank lines are ignored.
- There are no comments yet.
- Text literals use `"double"` or `'single'` quotes and may contain spaces, `;`,
  and braces. Escapes: `\n`, `\t`, `\r`, `\\`, `\"`, `\'`.

### Values

| Type | Examples | Notes |
|---|---|---|
| Int | `42`, `-7` | 64-bit signed |
| Float | `3.14`, `-0.5` | 64-bit |
| Bool | `true`, `false` | |
| Text | `"hello"`, `'hi there'` | |
| Hashset | `hashset 1 2 2` / `set 1 2` | Duplicates collapse; scalars only |
| Matrix | `matrix row1 { a 1 b 2 } row2 { a 3 }` | Named rows of named values; `matrex` is an alias |
| Void | — | Result of side-effect calls such as `console.println` |

### Variables

| Form | Meaning |
|---|---|
| `let x = 1` | Immutable binding |
| `let mut y = 2` | Mutable binding |
| `mut z = 3` | Mutable binding, shorthand |
| `const C = 4` | Constant; cannot be reassigned or deleted |
| `static S = 5` | Static; cannot be reassigned or deleted |
| `del x` | Delete a `let`/`mut` binding |
| `type Id = int` | Named alias (stored, not enforced yet) |

Only `mut` bindings can be assigned to:

```text
mut y = 1
y = y + 1          -- y is now 2

let x = 1
x = 2              -- ERROR: cannot assign to let variable 'x' (declare it with 'mut')
```

Re-declaring a name in the same scope fails unless both bindings are `mut`.
Declaring the same name in an inner block shadows the outer one instead.

### Control flow

```text
if score greater 90 {
    console.println("A")
} else if score greater 80 {
    console.println("B")
} else {
    console.println("C")
}

let mut i = 0
while i less 3 {
    console.println(i)
    i = i + 1
}

loop {
    i = i + 1
    if i is 5 { break }
}

for ch in "abc" {
    console.println(ch)
}
```

- Conditions must be `bool`; there is no truthiness, so `if 1 { }` is an error.
- `for name in <value>` iterates a `hashset` (its items, unordered), a `matrix`
  (row names, sorted), or `text` (characters).
- `break` and `continue` only work inside a loop; elsewhere they report an error.
- Every block gets its own scope, so bindings created inside a loop body or `if`
  branch disappear when the block ends.

### Functions

Define with `def` (Python-style) and a braced body. `fn` works as an alias.

```text
def greet(name, punctuation = "!") {
    return "Hello, " + name + punctuation
}

fn double(x) {
    return x * 2
}

console.println(greet("Nirab"))          -- Hello, Nirab!
console.println(greet("Nirab", "?"))     -- Hello, Nirab?
console.println(double(21))              -- 42
```

- Parameters come before the optional defaults; a required parameter may not
  follow one with a default.
- Defaults are evaluated when the function is called, in the function's scope,
  so they can refer to earlier parameters.
- `return` ends the function and returns a value. `return` with no value, or
  falling off the end, gives `void`.
- Calls are just `name(arguments)`; recursion works:

  ```text
  def fact(n) {
      if n less_equal 1 { return 1 }
      return n * fact(n - 1)
  }
  ```

- Function names live in the scope they are defined in, so a `def` inside a
  block disappears with the block.
- Arity is checked: `function 'f' takes 2 argument(s), but 3 given`.

### Operators

Lowest to highest precedence:

| Operator | Aliases | Meaning |
|---|---|---|
| `or` | `\|\|` | Logical or (short-circuits) |
| `and` | `&&` | Logical and (short-circuits) |
| `is`, `is_not` | `==`, `!=` | Equality / inequality |
| `greater`, `greater_equal`, `less`, `less_equal` | `>`, `>=`, `<`, `<=` | Ordering |
| `+`, `-` | | Addition/subtraction; `+` also joins two texts |
| `*`, `/`, `%` | | Multiplication, division, remainder |
| `not`, unary `-` | `!` | Logical not, numeric negation |

```text
let arithmetic = 1 + 2 * 3                  -- 7
let compare = 1 greater 2                   -- false
let logic = true and not false              -- true
let joined = "a" + "b"                      -- "ab"
```

Notes: `and`/`or` require bools; comparisons work on numbers and texts; dividing
by zero is a runtime error; ints and floats are distinct (`1 is 1.0` is false);
integer arithmetic is checked, so overflow is an error rather than a wrap.

### Collections

```text
let nums = hashset 1 2 2 3
let grid = matrix row1 { a 1 b 2 } row2 { a 3 }
console.println(nums)
console.println(grid)
```

prints:

```text
hashset 1 2 3
matrix row1 { a 1 b 2 } row2 { a 3 }
```

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

`console.input()` is an expression, so it can be stored:

```text
console.println("Name?")
let name = console.input()
console.println("Hi " + name)
```

Printing an empty line takes an empty string: `console.println("")`.

## REPL

`cargo run` with no arguments starts the prompt:

```text
> let mut total = 0
> for n in hashset 1 2 3 {
    total = total + n
  }
> console.println(total)
6
> quit
```

- One environment for the whole session, so values persist between inputs.
- Blocks can span several lines: the continuation prompt is `  `.
- Leave with `quit`, `exit`, or `Ctrl-D`.
- Empty lines just re-prompt.
- Piped input works too: `printf 'console.println("hi")\n' | cargo run`.

## Errors

Parsing and runtime errors stop the script and report the source line:

```text
ERROR: line 2: division by zero
ERROR: line 6: cannot assign to let variable 'x' (declare it with 'mut')
ERROR: line 7: condition must be a bool, got int
ERROR: line 9: unknown variable 'missing'
```

The REPL prints the same format and keeps going.

## Project layout

| Path | Purpose |
|---|---|
| `src/main.rs` | Entry point: script argument or REPL |
| `src/lexer.rs` | Source text → tokens with line numbers |
| `src/error.rs` | `Error { line, message }` |
| `src/parser/ast.rs` | Expression/statement tree and `VarKind` |
| `src/parser/parsers.rs` | Recursive-descent parser (precedence climbing) |
| `src/runner/run.rs` | Scoped environment, interpreter, console built-ins, REPL |
| `src/value.rs` | `Value` / `SetItem` types and display rules |
| `src/builtin_functions.json` | Planned language keyword/built-in → Rust mapping |

## Roadmap

`builtin_functions.json` lists the intended keyword set. Progress:

- Done: variables (`let`/`mut`/`const`/`static`/`del`/`type`), control flow
  (`if`/`else`/`while`/`loop`/`for`/`in`/`break`/`continue`), operators, blocks,
  and functions (`def`/`fn`/`return`, parameters with defaults, recursion).
- Next: `match`, real closures/lexical scoping, then `class`/`struct`/`impl`,
  modules, and `try`/`catch`/`throw`.
- Built-in methods: only `console.*` is implemented; the `file`, `data`,
  `collections`, `query`, `matrex`, `time`, `math`, `game`, and `system` groups
  are not wired up yet.

## Known limitations

- No classes, modules, `match`, `try`/`catch`, or lambdas.
- Functions are dynamically scoped: a called function sees the *caller's*
  variables. There are no closures and no first-class function values yet.
- No named/keyword arguments at call sites; defaults are positional.
- No comments and no imports.
- Numeric literals cannot start with a dot (`0.5` works, `.5` does not).
- `type` aliases are recorded but never enforced.
- `builtin_functions.json` is larger than the implemented surface (see above).
