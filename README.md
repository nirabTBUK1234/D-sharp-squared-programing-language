# D#² (D-sharp-squared)

A small, interpreted programming language implemented in Rust. It currently has
a tokenizer, a statement parser, variables, two compound value types (`hashset`
and `matrix`), console built-ins, and a REPL that remembers state until you quit.

It is an early prototype: no expressions, control flow, or user functions yet —
see [Known limitations](#known-limitations).

- No external crates (std only)
- Rust 2024 edition (Rust 1.85+)
- Source lives on the `main` branch: <https://github.com/nirabTBUK1234/D-sharp-squared-programing-language>

## Build and run

```bash
cargo build
cargo run -- program.dsq   # run a script (any file extension works)
cargo run                  # start the REPL
cargo test                 # run the test suite
```

## Quick start

`hello.dsq`:

```text
console.println("Hello, world!")
let name = console.input()
console.println(name)
```

```bash
$ cargo run -- hello.dsq
Hello, world!
Nirab                 # typed at the prompt
Nirab                 # printed by the script
```

`console.input()` does not print a prompt of its own, so print a question first
if you want one (`console.println("Name?")`). Use `console.println("")` for a
blank line.

## Language reference

### Statements

- One statement per line, or several separated by `;`.
- Blank lines are ignored.
- There are no comments yet.
- Text literals use `"double"` or `'single'` quotes and may contain spaces and `;`.

### Values

| Type | Examples | Notes |
|---|---|---|
| Int | `42`, `-7` | 64-bit signed |
| Float | `3.14`, `-0.5` | 64-bit |
| Bool | `true`, `false` | |
| Text | `"hello"`, `'hi there'` | Printed without quotes |
| Hashset | `hashset 1 2 2` / `set 1 2` | Duplicates collapse; scalars only |
| Matrix | `matrix row1 { a 1 b 2 } row2 { a 3 }` | Named rows of named values; `matrex` is an alias |

### Variables

| Form | Meaning |
|---|---|
| `let x = 1` | Immutable binding |
| `let mut y = 2` | Mutable binding |
| `mut z = 3` | Mutable binding, shorthand |
| `const C = 4` | Constant; cannot be deleted |
| `static S = 5` | Static; cannot be deleted |
| `del x` | Delete a `let`/`mut` binding |
| `type Id = int` | Named alias (stored, not enforced yet) |

Re-declaring a name is only allowed when the existing binding and the new one are
both `mut`:

```text
mut y = 1
mut y = 2
```

`y` is now `2`. This fails:

```text
let x = 1
let x = 2
```

with `ERROR: line 2: cannot redeclare 'x' (already a let variable)`.

Plain reassignment (`x = 2`) is not implemented; use `mut x = 2`.

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
inside a hashset is an error.

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

Arguments are single values, not expressions. `console.input()` is the only call
that can be nested, e.g. `let name = console.input()`. A bare `console.input()`
reads and discards a line, which works as a "press Enter to continue" pause.

Printing an empty line takes an empty string: `console.println("")`.

## REPL

`cargo run` with no arguments starts the prompt:

```text
> let counter = 1
> console.println(counter)
1
> quit
```

- One environment for the whole session, so values persist between inputs.
- Leave with `quit`, `exit`, or `Ctrl-D`.
- Empty lines just re-prompt.
- Piped input works too: `printf 'console.println("hi")\n' | cargo run`.

## Errors

Every error says where it happened, and a script keeps running after a bad line.
Example output:

```text
ERROR: line 3: unknown console method 'console.foo' (available: console.debug, console.print, console.println, console.input, console.clear, console.warn, console.error)
ERROR: line 4: console.println() takes 1 argument, but 2 given
ERROR: line 5: unknown variable 'missing'
ERROR: line 6: unterminated text literal: "oops
ERROR: line 7: unknown statement 'x' (expected let, mut, const, static, del, type, or console.<method>(...))
```

In the REPL the line prefix is dropped (`ERROR: <message>`), because each input
is its own buffer.

## Project layout

| Path | Purpose |
|---|---|
| `src/main.rs` | Entry point: script argument or REPL |
| `src/parser.rs`, `src/parser/parsers.rs` | Tokenizer, statement splitting, console-call and value parsing |
| `src/runner.rs`, `src/runner/run.rs` | Environment, statement execution, console built-ins, REPL loop |
| `src/value.rs` | `Value` / `SetItem` types and their display rules |
| `src/builtin_functions.json` | Planned language built-in → Rust implementation mapping |

The parser produces `Value`s directly and resolves variable names against the
environment while parsing, so parsing and evaluation are currently one step.

## Known limitations

- No expressions or operators, so `1 + 2` is not valid.
- No control flow (`if`, `while`, `for`) and no user-defined functions.
- No plain reassignment; only `mut name = value`.
- `type` aliases are recorded but never enforced.
- No comments, imports, or modules.
- Escape sequences inside text (`\n`, `\"`) are not decoded.
- `builtin_functions.json` also lists `file.*` built-ins; only `console.*` is implemented.
- Errors are printed but never affect the exit code, and running a script does not stop at the first error.
