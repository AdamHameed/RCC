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
    assert!(stderr.contains("expected function name to be 'main'"));
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
fn rejects_invalid_parameters() {
    let test_dir = make_test_dir();
    let input_path = test_dir.join("input.c");
    fs::write(&input_path, "int main(int x) { return 0; }\n").expect("should write input");
    let compile_output = Command::new(COMPILER_BIN)
        .arg(&input_path)
        .current_dir(&test_dir)
        .output()
        .expect("should invoke compiler");
    assert!(!compile_output.status.success());
    let stderr = String::from_utf8_lossy(&compile_output.stderr);
    assert!(stderr.contains("expected `)` or `void`"));
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
