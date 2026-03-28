use crate::ast::{BinaryOp, Expr, Program, Statement};
use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::targets::{CodeModel, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::values::IntValue;

pub fn generate_ir(program: &Program) -> Result<String, String> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|err| format!("failed to initialize LLVM target support: {err}"))?;

    let context = Context::create();
    let module = context.create_module(&program.function.name);
    let builder = context.create_builder();

    let triple = TargetMachine::get_default_triple();
    module.set_triple(&triple);

    let target = Target::from_triple(&triple)
        .map_err(|err| format!("failed to resolve target from triple: {err}"))?;
    let machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create LLVM target machine".to_string())?;
    let data_layout = machine.get_target_data().get_data_layout();
    module.set_data_layout(&data_layout);

    let i32_type = context.i32_type();
    let function_type = i32_type.fn_type(&[], false);
    let function = module.add_function(&program.function.name, function_type, None);
    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);

    let return_value = match program.function.body.first() {
        Some(Statement::Return(return_stmt)) => emit_expr(&context, &builder, &return_stmt.expr)?,
        None => i32_type.const_zero(),
    };

    builder
        .build_return(Some(&return_value))
        .map_err(|err| format!("failed to build return instruction: {err}"))?;

    if module.verify().is_err() {
        return Err("generated LLVM module did not verify".to_string());
    }

    Ok(module.print_to_string().to_string())
}

fn emit_expr<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    expr: &Expr,
) -> Result<IntValue<'ctx>, String> {
    match expr {
        Expr::IntegerLiteral(integer) => {
            Ok(context.i32_type().const_int(integer.value as u64, true))
        }
        Expr::Binary(binary) => {
            let left = emit_expr(context, builder, &binary.left)?;
            let right = emit_expr(context, builder, &binary.right)?;

            match binary.operator {
                BinaryOp::Add => builder
                    .build_int_add(left, right, "addtmp")
                    .map_err(|err| format!("failed to emit add instruction: {err}")),
                BinaryOp::Subtract => builder
                    .build_int_sub(left, right, "subtmp")
                    .map_err(|err| format!("failed to emit sub instruction: {err}")),
                BinaryOp::Multiply => builder
                    .build_int_mul(left, right, "multmp")
                    .map_err(|err| format!("failed to emit mul instruction: {err}")),
                BinaryOp::Divide => builder
                    .build_int_signed_div(left, right, "divtmp")
                    .map_err(|err| format!("failed to emit div instruction: {err}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_ir;
    use crate::ast::{
        BinaryExpr, BinaryOp, Expr, Function, IntegerLiteral, Program, ReturnStatement, Statement,
    };

    #[test]
    fn generates_llvm_ir_for_integer_return() {
        let program = Program {
            function: Function {
                name: "main".to_string(),
                body: vec![Statement::Return(ReturnStatement {
                    expr: Expr::IntegerLiteral(IntegerLiteral { value: 5 }),
                })],
            },
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("ret i32 5"));
    }

    #[test]
    fn generates_llvm_ir_for_arithmetic_expression() {
        let program = Program {
            function: Function {
                name: "main".to_string(),
                body: vec![Statement::Return(ReturnStatement {
                    expr: Expr::Binary(BinaryExpr {
                        left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 4 })),
                        operator: BinaryOp::Multiply,
                        right: Box::new(Expr::Binary(BinaryExpr {
                            left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                            operator: BinaryOp::Add,
                            right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 1 })),
                        })),
                    }),
                })],
            },
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("ret i32 12"));
    }
}
