use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

const COMPILER_BIN: &str = env!("CARGO_BIN_EXE_compiler");

#[test]
fn returns_integer_literal() {
    assert_program_exit_code("int main() { return 5; }\n", 5);
}

#[test]
fn evaluates_addition() {
    assert_program_exit_code("int main() { return 2 + 3; }\n", 5);
}

#[test]
fn evaluates_multiplication_with_parentheses() {
    assert_program_exit_code("int main() { return 4 * (2 + 1); }\n", 12);
}

#[test]
fn respects_operator_precedence() {
    assert_program_exit_code("int main() { return 2 + 3 * 4; }\n", 14);
}

#[test]
fn evaluates_division_and_subtraction() {
    assert_program_exit_code("int main() { return 20 / 5 - 1; }\n", 3);
}

#[test]
fn evaluates_ternary_conditional() {
    assert_program_exit_code("int main() { return 1 ? 10 : 20; }\n", 10);
    assert_program_exit_code("int main() { return 0 ? 10 : 20; }\n", 20);
}

#[test]
fn evaluates_chained_ternary_right_associativity() {
    assert_program_exit_code(
        "int main() { int x = 2; return x == 1 ? 10 : x == 2 ? 20 : 30; }\n",
        20,
    );
}

#[test]
fn ternary_evaluates_only_taken_branch() {
    // The untaken branch divides by zero; lazy evaluation means no trap.
    assert_program_exit_code("int main() { int zero = 0; return 0 ? 5 / zero : 7; }\n", 7);
}

#[test]
fn evaluates_ternary_with_calls_and_assignment() {
    assert_program_exit_code(
        "int max(int a, int b) { return a > b ? a : b; }\nint main() { int m = max(3, 9); m += 1 ? 2 : 100; return m; }\n",
        11,
    );
}

#[test]
fn evaluates_break_in_while_loop() {
    assert_program_exit_code(
        "int main() { int x = 0; while (1) { x++; if (x == 7) { break; } } return x; }\n",
        7,
    );
}

#[test]
fn evaluates_continue_in_for_loop() {
    assert_program_exit_code(
        "int main() { int total = 0; for (int i = 0; i < 10; i++) { if (i % 2 == 0) { continue; } total += i; } return total; }\n",
        25,
    );
}

#[test]
fn break_applies_to_innermost_loop() {
    assert_program_exit_code(
        "int main() { int count = 0; for (int i = 0; i < 3; i++) { while (1) { break; } count++; } return count; }\n",
        3,
    );
}

#[test]
fn rejects_break_outside_loop() {
    let test_dir = make_test_dir();
    let input_path = test_dir.join("input.c");
    fs::write(&input_path, "int main() { break; return 0; }\n").expect("should write input");
    let compile_output = Command::new(COMPILER_BIN)
        .arg(&input_path)
        .current_dir(&test_dir)
        .output()
        .expect("should invoke compiler");
    assert!(!compile_output.status.success());
    let stderr = String::from_utf8_lossy(&compile_output.stderr);
    assert!(stderr.contains("`break` used outside of a loop"));
}

#[test]
fn evaluates_increment_and_decrement_statements() {
    assert_program_exit_code(
        "int main() { int x = 5; x++; ++x; x--; --x; x++; return x; }\n",
        6,
    );
}

#[test]
fn evaluates_increment_in_for_post() {
    assert_program_exit_code(
        "int main() { int total = 0; for (int i = 0; i < 5; i++) { total += i; } return total; }\n",
        10,
    );
}

#[test]
fn evaluates_compound_assignments() {
    assert_program_exit_code(
        "int main() { int x = 10; x += 5; x -= 3; x *= 4; x /= 6; x %= 5; return x; }\n",
        3,
    );
}

#[test]
fn evaluates_compound_assignment_through_pointer() {
    assert_program_exit_code(
        "int main() { int x = 40; int *p = &x; *p += 2; return x; }\n",
        42,
    );
}

#[test]
fn evaluates_compound_assignment_in_for_post() {
    assert_program_exit_code(
        "int main() { int total = 0; for (int i = 1; i <= 4; i += 1) { total += i; } return total; }\n",
        10,
    );
}

#[test]
fn evaluates_modulo() {
    assert_program_exit_code("int main() { return 17 % 5; }\n", 2);
}

#[test]
fn modulo_shares_multiplicative_precedence() {
    // 10 % 4 * 2 groups left-to-right: (10 % 4) * 2 = 4, plus 1 = 5
    assert_program_exit_code("int main() { return 1 + 10 % 4 * 2; }\n", 5);
}

#[test]
fn ignores_comments() {
    assert_program_exit_code(
        "// leading comment\nint main() {\n    int x = 6; // set x\n    /* block\n       comment */\n    return x % 4;\n}\n",
        2,
    );
}

#[test]
fn evaluates_unary_negation() {
    // -5 modulo 256 is 251
    assert_program_exit_code("int main() { return -5; }\n", 251);
}

#[test]
fn evaluates_unary_expression_with_spaces() {
    assert_program_exit_code("int main() { return - -5; }\n", 5);
}

#[test]
fn evaluates_void_parameter() {
    assert_program_exit_code("int main(void) { return 42; }\n", 42);
}

#[test]
fn rejects_non_main_function() {
    let test_dir = make_test_dir();
    let input_path = test_dir.join("input.c");
    fs::write(&input_path, "int foo() { return 5; }\n").expect("should write input");
    let compile_output = Command::new(COMPILER_BIN)
        .arg(&input_path)
        .current_dir(&test_dir)
        .output()
        .expect("should invoke compiler");
    assert!(!compile_output.status.success());
    let stderr = String::from_utf8_lossy(&compile_output.stderr);
    assert!(stderr.contains("expected a function named 'main'"));
}

#[test]
fn evaluates_nested_unary_expression() {
    assert_program_exit_code("int main() { return - - -5; }\n", 251);
}

#[test]
fn evaluates_unary_precedence() {
    assert_program_exit_code("int main() { return -2 * -3; }\n", 6);
}

#[test]
fn evaluates_variables() {
    assert_program_exit_code(
        "int main() {\n    int x = 10;\n    int y = 20;\n    x = x + y;\n    return x;\n}\n",
        30,
    );
}

#[test]
fn evaluates_variable_negation() {
    assert_program_exit_code("int main() {\n    int x = 5;\n    return -x;\n}\n", 251);
}

#[test]
fn evaluates_comparisons() {
    assert_program_exit_code("int main() { return 5 < 10; }\n", 1);
    assert_program_exit_code("int main() { return 5 > 10; }\n", 0);
    assert_program_exit_code("int main() { return 5 == 5; }\n", 1);
    assert_program_exit_code("int main() { return 5 != 5; }\n", 0);
    assert_program_exit_code("int main() { return 5 <= 5; }\n", 1);
    assert_program_exit_code("int main() { return 5 >= 6; }\n", 0);
}

#[test]
fn evaluates_logical_operators() {
    assert_program_exit_code("int main() { return 1 && 2; }\n", 1);
    assert_program_exit_code("int main() { return 1 && 0; }\n", 0);
    assert_program_exit_code("int main() { return 0 || 2; }\n", 1);
    assert_program_exit_code("int main() { return 0 || 0; }\n", 0);
    assert_program_exit_code("int main() { return !5; }\n", 0);
    assert_program_exit_code("int main() { return !0; }\n", 1);
}

#[test]
fn evaluates_if_statement() {
    assert_program_exit_code(
        "int main() {\n    int x = 10;\n    if (x == 10) {\n        return 1;\n    } else {\n        return 2;\n    }\n}\n",
        1,
    );
}

#[test]
fn evaluates_if_else_statement() {
    assert_program_exit_code(
        "int main() {\n    int x = 20;\n    if (x == 10) {\n        return 1;\n    } else {\n        return 2;\n    }\n}\n",
        2,
    );
}

#[test]
fn evaluates_while_loop() {
    assert_program_exit_code(
        "int main() {\n    int x = 0;\n    while (x < 5) {\n        x = x + 1;\n    }\n    return x;\n}\n",
        5,
    );
}

#[test]
fn evaluates_for_loop() {
    assert_program_exit_code(
        "int main() {\n    int sum = 0;\n    for (int i = 0; i < 5; i = i + 1) {\n        sum = sum + i;\n    }\n    return sum;\n}\n",
        10,
    );
}

#[test]
fn evaluates_nested_loops_and_conditionals() {
    assert_program_exit_code(
        "int main() {\n    int result = 0;\n    for (int i = 0; i < 3; i = i + 1) {\n        int j = 0;\n        while (j < 2) {\n            if (i == 1) {\n                result = result + 10;\n            } else {\n                result = result + 1;\n            }\n            j = j + 1;\n        }\n    }\n    return result;\n}\n",
        24,
    );
}

#[test]
fn evaluates_simple_pointer() {
    assert_program_exit_code(
        "int main() {\n    int x = 42;\n    int* p = &x;\n    return *p;\n}\n",
        42,
    );
}

#[test]
fn evaluates_pointer_assignment() {
    assert_program_exit_code(
        "int main() {\n    int x = 42;\n    int* p = &x;\n    *p = 100;\n    return x;\n}\n",
        100,
    );
}

#[test]
fn evaluates_double_pointer() {
    assert_program_exit_code(
        "int main() {\n    int x = 42;\n    int* p = &x;\n    int** pp = &p;\n    **pp = 200;\n    return x;\n}\n",
        200,
    );
}

#[test]
fn rejects_invalid_parameters() {
    let test_dir = make_test_dir();
    let input_path = test_dir.join("input.c");
    fs::write(&input_path, "int main(int x y) { return 0; }\n").expect("should write input");
    let compile_output = Command::new(COMPILER_BIN)
        .arg(&input_path)
        .current_dir(&test_dir)
        .output()
        .expect("should invoke compiler");
    assert!(!compile_output.status.success());
    let stderr = String::from_utf8_lossy(&compile_output.stderr);
    assert!(stderr.contains("expected `,` or `)`"));
}

fn assert_program_exit_code(source: &str, expected_exit_code: i32) {
    let test_dir = make_test_dir();
    let input_path = test_dir.join("input.c");
    let executable_path = test_dir.join("output");

    fs::write(&input_path, source).expect("should write input program");

    let compile_output = Command::new(COMPILER_BIN)
        .arg(&input_path)
        .current_dir(&test_dir)
        .output()
        .expect("should invoke compiler binary");

    assert!(
        compile_output.status.success(),
        "compiler failed.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&compile_output.stdout),
        String::from_utf8_lossy(&compile_output.stderr)
    );

    assert!(
        executable_path.exists(),
        "expected executable at {}",
        executable_path.display()
    );

    let run_status = Command::new(&executable_path)
        .current_dir(&test_dir)
        .status()
        .expect("should run compiled executable");

    let exit_code = run_status
        .code()
        .expect("compiled executable should exit with a code");

    assert_eq!(
        exit_code, expected_exit_code,
        "unexpected exit code for source:\n{source}"
    );
}

fn make_test_dir() -> PathBuf {
    let unique_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    let test_dir = std::env::temp_dir().join(format!("rcc-e2e-{unique}-{unique_id}"));
    create_dir(&test_dir);
    test_dir
}

fn create_dir(path: &Path) {
    fs::create_dir_all(path).unwrap_or_else(|error| {
        panic!(
            "failed to create test directory {}: {error}",
            path.display()
        )
    });
}
