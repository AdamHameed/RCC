use crate::ast::{BinaryOp, Expr, Program, Statement, UnaryOp};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::targets::{CodeModel, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::values::{IntValue, PointerValue};
use inkwell::{IntPredicate, OptimizationLevel};
use std::collections::HashMap;

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

    let mut return_value = None;
    let mut variables = HashMap::new();

    for statement in &program.function.body {
        match statement {
            Statement::Declare(decl) => {
                let init_val = emit_expr(&context, &builder, &decl.init, &variables)?;
                let alloca = builder
                    .build_alloca(i32_type, &decl.name)
                    .map_err(|err| format!("failed to build alloca: {err}"))?;
                builder
                    .build_store(alloca, init_val)
                    .map_err(|err| format!("failed to build store: {err}"))?;
                variables.insert(decl.name.clone(), alloca);
            }
            Statement::Assign(assign) => {
                let val = emit_expr(&context, &builder, &assign.expr, &variables)?;
                let ptr = variables
                    .get(&assign.name)
                    .ok_or_else(|| format!("undefined variable '{}'", assign.name))?;
                builder
                    .build_store(*ptr, val)
                    .map_err(|err| format!("failed to build store: {err}"))?;
            }
            Statement::Return(ret_stmt) => {
                let val = emit_expr(&context, &builder, &ret_stmt.expr, &variables)?;
                return_value = Some(val);
                break;
            }
        }
    }

    let return_val = return_value.unwrap_or_else(|| i32_type.const_zero());
    builder
        .build_return(Some(&return_val))
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
    variables: &HashMap<String, PointerValue<'ctx>>,
) -> Result<IntValue<'ctx>, String> {
    match expr {
        Expr::IntegerLiteral(integer) => {
            Ok(context.i32_type().const_int(integer.value as u64, true))
        }
        Expr::Variable(var) => {
            let ptr = variables
                .get(&var.name)
                .ok_or_else(|| format!("undefined variable '{}'", var.name))?;
            builder
                .build_load(context.i32_type(), *ptr, &var.name)
                .map_err(|err| {
                    format!(
                        "failed to build load instruction for variable '{}': {err}",
                        var.name
                    )
                })
                .map(|v| v.into_int_value())
        }
        Expr::Unary(unary) => {
            let operand = emit_expr(context, builder, &unary.expr, variables)?;
            match unary.operator {
                UnaryOp::Negate => builder
                    .build_int_neg(operand, "negtmp")
                    .map_err(|err| format!("failed to emit neg instruction: {err}")),
                UnaryOp::Posate => Ok(operand),
                UnaryOp::LogicalNot => {
                    let zero = context.i32_type().const_zero();
                    let cmp = builder
                        .build_int_compare(IntPredicate::EQ, operand, zero, "nottmp")
                        .map_err(|err| {
                            format!("failed to emit comparison for logical not: {err}")
                        })?;
                    builder
                        .build_int_z_extend(cmp, context.i32_type(), "casttmp")
                        .map_err(|err| format!("failed to emit zext for logical not: {err}"))
                }
            }
        }
        Expr::Binary(binary) => {
            let is_logical_op =
                matches!(binary.operator, BinaryOp::LogicalAnd | BinaryOp::LogicalOr);

            if is_logical_op {
                match binary.operator {
                    BinaryOp::LogicalAnd => {
                        let lhs_val = emit_expr(context, builder, &binary.left, variables)?;
                        let lhs_is_true = builder
                            .build_int_compare(
                                IntPredicate::NE,
                                lhs_val,
                                context.i32_type().const_zero(),
                                "lhs_true",
                            )
                            .map_err(|err| err.to_string())?;

                        let start_bb = builder.get_insert_block().ok_or("no insert block")?;
                        let parent_func = start_bb.get_parent().ok_or("no parent function")?;

                        let rhs_bb = context.append_basic_block(parent_func, "and.rhs");
                        let merge_bb = context.append_basic_block(parent_func, "and.merge");

                        builder
                            .build_conditional_branch(lhs_is_true, rhs_bb, merge_bb)
                            .map_err(|err| err.to_string())?;

                        // RHS block
                        builder.position_at_end(rhs_bb);
                        let rhs_val = emit_expr(context, builder, &binary.right, variables)?;
                        let rhs_is_true = builder
                            .build_int_compare(
                                IntPredicate::NE,
                                rhs_val,
                                context.i32_type().const_zero(),
                                "rhs_true",
                            )
                            .map_err(|err| err.to_string())?;
                        let rhs_res = builder
                            .build_int_z_extend(rhs_is_true, context.i32_type(), "rhs_cast")
                            .map_err(|err| err.to_string())?;
                        builder
                            .build_unconditional_branch(merge_bb)
                            .map_err(|err| err.to_string())?;

                        let actual_rhs_bb = builder.get_insert_block().ok_or("no insert block")?;

                        // Merge block
                        builder.position_at_end(merge_bb);
                        let phi = builder
                            .build_phi(context.i32_type(), "and.result")
                            .map_err(|err| err.to_string())?;
                        phi.add_incoming(&[
                            (&context.i32_type().const_zero(), start_bb),
                            (&rhs_res, actual_rhs_bb),
                        ]);
                        Ok(phi.as_basic_value().into_int_value())
                    }
                    BinaryOp::LogicalOr => {
                        let lhs_val = emit_expr(context, builder, &binary.left, variables)?;
                        let lhs_is_true = builder
                            .build_int_compare(
                                IntPredicate::NE,
                                lhs_val,
                                context.i32_type().const_zero(),
                                "lhs_true",
                            )
                            .map_err(|err| err.to_string())?;

                        let start_bb = builder.get_insert_block().ok_or("no insert block")?;
                        let parent_func = start_bb.get_parent().ok_or("no parent function")?;

                        let rhs_bb = context.append_basic_block(parent_func, "or.rhs");
                        let merge_bb = context.append_basic_block(parent_func, "or.merge");

                        builder
                            .build_conditional_branch(lhs_is_true, merge_bb, rhs_bb)
                            .map_err(|err| err.to_string())?;

                        // RHS block
                        builder.position_at_end(rhs_bb);
                        let rhs_val = emit_expr(context, builder, &binary.right, variables)?;
                        let rhs_is_true = builder
                            .build_int_compare(
                                IntPredicate::NE,
                                rhs_val,
                                context.i32_type().const_zero(),
                                "rhs_true",
                            )
                            .map_err(|err| err.to_string())?;
                        let rhs_res = builder
                            .build_int_z_extend(rhs_is_true, context.i32_type(), "rhs_cast")
                            .map_err(|err| err.to_string())?;
                        builder
                            .build_unconditional_branch(merge_bb)
                            .map_err(|err| err.to_string())?;

                        let actual_rhs_bb = builder.get_insert_block().ok_or("no insert block")?;

                        // Merge block
                        builder.position_at_end(merge_bb);
                        let phi = builder
                            .build_phi(context.i32_type(), "or.result")
                            .map_err(|err| err.to_string())?;
                        phi.add_incoming(&[
                            (&context.i32_type().const_int(1, false), start_bb),
                            (&rhs_res, actual_rhs_bb),
                        ]);
                        Ok(phi.as_basic_value().into_int_value())
                    }
                    _ => unreachable!(),
                }
            } else {
                let left = emit_expr(context, builder, &binary.left, variables)?;
                let right = emit_expr(context, builder, &binary.right, variables)?;

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
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::LessThan
                    | BinaryOp::LessEqual
                    | BinaryOp::GreaterThan
                    | BinaryOp::GreaterEqual => {
                        let pred = match binary.operator {
                            BinaryOp::Equal => IntPredicate::EQ,
                            BinaryOp::NotEqual => IntPredicate::NE,
                            BinaryOp::LessThan => IntPredicate::SLT,
                            BinaryOp::LessEqual => IntPredicate::SLE,
                            BinaryOp::GreaterThan => IntPredicate::SGT,
                            BinaryOp::GreaterEqual => IntPredicate::SGE,
                            _ => unreachable!(),
                        };
                        let cmp = builder
                            .build_int_compare(pred, left, right, "cmptmp")
                            .map_err(|err| format!("failed to emit cmp instruction: {err}"))?;
                        builder
                            .build_int_z_extend(cmp, context.i32_type(), "casttmp")
                            .map_err(|err| format!("failed to emit zext instruction: {err}"))
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_ir;
    use crate::ast::{
        BinaryExpr, BinaryOp, Expr, Function, IntegerLiteral, Program, ReturnStatement, Statement,
        UnaryExpr, UnaryOp, VarAssignStatement, VarDeclareStatement, VariableExpr,
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

    #[test]
    fn generates_llvm_ir_for_unary_negation() {
        let program = Program {
            function: Function {
                name: "main".to_string(),
                body: vec![Statement::Return(ReturnStatement {
                    expr: Expr::Unary(UnaryExpr {
                        operator: UnaryOp::Negate,
                        expr: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 5 })),
                    }),
                })],
            },
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("ret i32 -5"));
    }

    #[test]
    fn generates_llvm_ir_for_variables() {
        let program = Program {
            function: Function {
                name: "main".to_string(),
                body: vec![
                    Statement::Declare(VarDeclareStatement {
                        name: "x".to_string(),
                        init: Expr::IntegerLiteral(IntegerLiteral { value: 42 }),
                    }),
                    Statement::Assign(VarAssignStatement {
                        name: "x".to_string(),
                        expr: Expr::Binary(BinaryExpr {
                            left: Box::new(Expr::Variable(VariableExpr {
                                name: "x".to_string(),
                            })),
                            operator: BinaryOp::Add,
                            right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 10 })),
                        }),
                    }),
                    Statement::Return(ReturnStatement {
                        expr: Expr::Variable(VariableExpr {
                            name: "x".to_string(),
                        }),
                    }),
                ],
            },
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("%x = alloca i32"));
        assert!(ir.contains("store i32 42, ptr %x"));
        assert!(ir.contains("load i32, ptr %x"));
        assert!(ir.contains("store i32"));
        assert!(ir.contains("ret i32"));
    }

    #[test]
    fn generates_llvm_ir_for_comparison() {
        let program = Program {
            function: Function {
                name: "main".to_string(),
                body: vec![
                    Statement::Declare(VarDeclareStatement {
                        name: "x".to_string(),
                        init: Expr::IntegerLiteral(IntegerLiteral { value: 5 }),
                    }),
                    Statement::Return(ReturnStatement {
                        expr: Expr::Binary(BinaryExpr {
                            left: Box::new(Expr::Variable(VariableExpr {
                                name: "x".to_string(),
                            })),
                            operator: BinaryOp::LessThan,
                            right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 10 })),
                        }),
                    }),
                ],
            },
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("icmp slt i32"));
        assert!(ir.contains("zext i1") || ir.contains("cast"));
        assert!(ir.contains("ret i32"));
    }

    #[test]
    fn generates_llvm_ir_for_logical_and() {
        let program = Program {
            function: Function {
                name: "main".to_string(),
                body: vec![Statement::Return(ReturnStatement {
                    expr: Expr::Binary(BinaryExpr {
                        left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 1 })),
                        operator: BinaryOp::LogicalAnd,
                        right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                    }),
                })],
            },
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("and.rhs:"));
        assert!(ir.contains("and.merge:"));
        assert!(ir.contains("phi i32"));
    }
}
