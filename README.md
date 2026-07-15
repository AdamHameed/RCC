# RCC

RCC is a small Rust-based C compiler project for a minimal subset of C. It lexes, parses, builds an AST, lowers that AST to LLVM IR with `inkwell`, and then invokes `clang` or `gcc` to produce an executable.

## Supported C Subset

Current support is intentionally small:

- `int main()` or `int main(void)` function declaration
- integer literals
- unary arithmetic:
  - `+` (positive)
  - `-` (negation)
- binary arithmetic:
  - `+` (addition)
  - `-` (subtraction)
  - `*` (multiplication)
  - `/` (division)
- parentheses for grouping

Examples:

```c
int main(void) { return -5; }
int main() { return 2 + 3 * 4; }
int main() { return 4 * (2 - -1); }
```

## Project Layout

```text
src/
  ast/
  codegen/
  lexer/
  parser/
  main.rs
tests/
samples/
```

## Requirements

- Rust and Cargo
- `clang` or `gcc`
- LLVM 17 for `inkwell`

This repo is currently configured for Homebrew LLVM 17 on macOS in [.cargo/config.toml](/Users/adam/Downloads/Projects/RCC/.cargo/config.toml):

```toml
[env]
LLVM_SYS_170_PREFIX = "/opt/homebrew/opt/llvm@17"
```

If you use a different LLVM install location, update that path.

## Build

```bash
cargo build
```

The compiler binary is named `compiler`.

## Usage

Compile a C file:

```bash
cargo run --bin compiler -- samples/return_5.c -o my_output
```

This produces:

- `my_output.ll` - generated LLVM IR
- `my_output` - compiled executable

If the `-o` option is omitted, it defaults to producing `output` and `output.ll` in the current working directory.

Run the produced executable and inspect its return code:

```bash
./my_output
echo $?
```

## Sample Programs

Sample inputs live in the [samples](/Users/adam/Downloads/Projects/RCC/samples) directory:

- [return_5.c](/Users/adam/Downloads/Projects/RCC/samples/return_5.c)
- [addition.c](/Users/adam/Downloads/Projects/RCC/samples/addition.c)
- [precedence.c](/Users/adam/Downloads/Projects/RCC/samples/precedence.c)
- [parentheses.c](/Users/adam/Downloads/Projects/RCC/samples/parentheses.c)
- [mixed_ops.c](/Users/adam/Downloads/Projects/RCC/samples/mixed_ops.c)

## Testing

Run all tests:

```bash
cargo test
```

The test suite includes:

- unit tests for the lexer, parser, AST, and codegen
- end-to-end tests that compile small C programs, run them, and check the exit code

## How It Works

The compiler pipeline is:

1. Lex source code into tokens
2. Parse tokens into an AST
3. Lower the AST into LLVM IR
4. Write LLVM IR to `output.ll`
5. Invoke the system compiler to build `output`

For example, this AST:

```c
int main() { return 4 * (2 + 1); }
```

can lower to LLVM IR equivalent to:

```llvm
define i32 @main() {
entry:
  ret i32 12
}
```

LLVM may fold constant expressions during IR construction, so simple arithmetic can appear already simplified in the generated IR.
