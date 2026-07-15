use crate::ast::{BinaryOp, Expr, Program, Statement, Type, UnaryOp};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::targets::{CodeModel, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::types::BasicType;
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::{AddressSpace, IntPredicate, OptimizationLevel};
use std::collections::HashMap;

pub fn generate_ir(program: &Program) -> Result<String, String> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|err| format!("failed to initialize LLVM target support: {err}"))?;

    let context = Context::create();
    let module = context.create_module("rcc_module");
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

    // Build the function types map to support type-checking calls
    let mut function_types = HashMap::new();
    for func in &program.functions {
        function_types.insert(func.name.clone(), func.return_type.clone());
    }

    // Step 1: Declare all functions in the LLVM module
    let mut llvm_functions = HashMap::new();
    for func in &program.functions {
        let return_ty = llvm_type(&context, &func.return_type);
        let param_types: Vec<_> = func
            .params
            .iter()
            .map(|p| llvm_type(&context, &p.ty).into())
            .collect();
        let fn_type = return_ty.fn_type(&param_types, false);
        let llvm_func = module.add_function(&func.name, fn_type, None);
        llvm_functions.insert(func.name.clone(), llvm_func);
    }

    // Step 2: Compile the body of each function
    for func in &program.functions {
        let llvm_func = llvm_functions
            .get(&func.name)
            .ok_or_else(|| format!("failed to find declared LLVM function '{}'", func.name))?;

        let entry = context.append_basic_block(*llvm_func, "entry");
        let return_bb = context.append_basic_block(*llvm_func, "return");

        builder.position_at_end(entry);

        let return_ty = llvm_type(&context, &func.return_type);
        let ret_val_ptr = builder
            .build_alloca(return_ty, "retval")
            .map_err(|err| format!("failed to build retval alloca: {err}"))?;

        // Initialize return value to null/zero
        let zero_val: BasicValueEnum = match func.return_type {
            Type::Int => context.i32_type().const_zero().into(),
            Type::Pointer(_) => context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
        };
        builder
            .build_store(ret_val_ptr, zero_val)
            .map_err(|err| format!("failed to store initial retval: {err}"))?;

        let mut variables = vec![HashMap::new()];

        // Allocate parameters as local variables in the entry block
        for (i, param) in func.params.iter().enumerate() {
            let param_val = llvm_func
                .get_nth_param(i as u32)
                .ok_or_else(|| format!("parameter '{}' not found in LLVM function", param.name))?;
            let llvm_ty = llvm_type(&context, &param.ty);
            let alloca = builder.build_alloca(llvm_ty, &param.name).map_err(|err| {
                format!(
                    "failed to build alloca for parameter '{}': {err}",
                    param.name
                )
            })?;
            builder
                .build_store(alloca, param_val)
                .map_err(|err| format!("failed to store parameter '{}': {err}", param.name))?;
            variables[0].insert(param.name.clone(), (param.ty.clone(), alloca));
        }

        for statement in &func.body {
            if builder
                .get_insert_block()
                .and_then(|bb| bb.get_terminator())
                .is_some()
            {
                break;
            }
            emit_statement(
                &context,
                &builder,
                statement,
                &mut variables,
                ret_val_ptr,
                return_bb,
                &function_types,
                &module,
            )?;
        }

        if builder
            .get_insert_block()
            .and_then(|bb| bb.get_terminator())
            .is_none()
        {
            builder
                .build_unconditional_branch(return_bb)
                .map_err(|err| err.to_string())?;
        }

        builder.position_at_end(return_bb);
        let return_val = builder
            .build_load(return_ty, ret_val_ptr, "retval")
            .map_err(|err| format!("failed to load return value: {err}"))?;
        builder
            .build_return(Some(&return_val))
            .map_err(|err| format!("failed to build return instruction: {err}"))?;

        if !llvm_func.verify(true) {
            // Note: verify returns true if the function is valid (not 1)
            return Err(format!(
                "LLVM verification failed for function '{}'",
                func.name
            ));
        }
    }

    if module.verify().is_err() {
        return Err("generated LLVM module did not verify".to_string());
    }

    Ok(module.print_to_string().to_string())
}

fn llvm_type<'ctx>(context: &'ctx Context, ty: &Type) -> inkwell::types::BasicTypeEnum<'ctx> {
    match ty {
        Type::Int => inkwell::types::BasicTypeEnum::IntType(context.i32_type()),
        Type::Pointer(_) => {
            inkwell::types::BasicTypeEnum::PointerType(context.ptr_type(AddressSpace::default()))
        }
    }
}

fn lookup_variable<'a, 'ctx>(
    name: &str,
    variables: &'a [HashMap<String, (Type, PointerValue<'ctx>)>],
) -> Option<&'a (Type, PointerValue<'ctx>)> {
    for scope in variables.iter().rev() {
        if let Some(val) = scope.get(name) {
            return Some(val);
        }
    }
    None
}

fn type_of_expr(
    expr: &Expr,
    variables: &[HashMap<String, (Type, PointerValue)>],
    function_types: &HashMap<String, Type>,
) -> Result<Type, String> {
    match expr {
        Expr::IntegerLiteral(_) => Ok(Type::Int),
        Expr::Variable(var) => {
            let mut found = None;
            for scope in variables.iter().rev() {
                if let Some((ty, _)) = scope.get(&var.name) {
                    found = Some(ty.clone());
                    break;
                }
            }
            found.ok_or_else(|| format!("undefined variable '{}'", var.name))
        }
        Expr::Unary(unary) => match unary.operator {
            UnaryOp::Negate | UnaryOp::Posate | UnaryOp::LogicalNot => Ok(Type::Int),
            UnaryOp::Deref => {
                let inner_ty = type_of_expr(&unary.expr, variables, function_types)?;
                match inner_ty {
                    Type::Pointer(boxed_ty) => Ok(*boxed_ty),
                    _ => Err("cannot dereference non-pointer type".to_string()),
                }
            }
            UnaryOp::AddrOf => {
                let inner_ty = type_of_expr(&unary.expr, variables, function_types)?;
                Ok(Type::Pointer(Box::new(inner_ty)))
            }
        },
        Expr::Binary(_) => Ok(Type::Int),
        Expr::Call(call) => {
            let ret_ty = function_types
                .get(&call.name)
                .ok_or_else(|| format!("undefined function '{}'", call.name))?;
            Ok(ret_ty.clone())
        }
    }
}

fn emit_lvalue<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    expr: &Expr,
    variables: &[HashMap<String, (Type, PointerValue<'ctx>)>],
    function_types: &HashMap<String, Type>,
    module: &inkwell::module::Module<'ctx>,
) -> Result<PointerValue<'ctx>, String> {
    match expr {
        Expr::Variable(var) => {
            let (_, ptr) = lookup_variable(&var.name, variables)
                .ok_or_else(|| format!("undefined variable '{}'", var.name))?;
            Ok(*ptr)
        }
        Expr::Unary(unary) if unary.operator == UnaryOp::Deref => {
            let val = emit_expr(
                context,
                builder,
                &unary.expr,
                variables,
                function_types,
                module,
            )?;
            match val {
                BasicValueEnum::PointerValue(ptr) => Ok(ptr),
                _ => Err("dereference target did not evaluate to a pointer".to_string()),
            }
        }
        _ => Err("expression is not an lvalue".to_string()),
    }
}

fn emit_expr<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    expr: &Expr,
    variables: &[HashMap<String, (Type, PointerValue<'ctx>)>],
    function_types: &HashMap<String, Type>,
    module: &inkwell::module::Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    match expr {
        Expr::IntegerLiteral(integer) => Ok(BasicValueEnum::IntValue(
            context.i32_type().const_int(integer.value as u64, true),
        )),
        Expr::Variable(var) => {
            let (ty, ptr) = lookup_variable(&var.name, variables)
                .ok_or_else(|| format!("undefined variable '{}'", var.name))?;
            let llvm_ty = llvm_type(context, ty);
            builder.build_load(llvm_ty, *ptr, &var.name).map_err(|err| {
                format!(
                    "failed to build load instruction for variable '{}': {err}",
                    var.name
                )
            })
        }
        Expr::Unary(unary) => match unary.operator {
            UnaryOp::Deref => {
                let val = emit_expr(
                    context,
                    builder,
                    &unary.expr,
                    variables,
                    function_types,
                    module,
                )?;
                let ptr = match val {
                    BasicValueEnum::PointerValue(p) => p,
                    _ => {
                        return Err("dereference operand did not evaluate to a pointer".to_string());
                    }
                };
                let expr_ty = type_of_expr(&unary.expr, variables, function_types)?;
                let inner_ty = match expr_ty {
                    Type::Pointer(inner) => *inner,
                    _ => return Err("cannot dereference non-pointer type".to_string()),
                };
                let llvm_ty = llvm_type(context, &inner_ty);
                builder
                    .build_load(llvm_ty, ptr, "dereftmp")
                    .map_err(|err| err.to_string())
            }
            UnaryOp::AddrOf => {
                let ptr = emit_lvalue(
                    context,
                    builder,
                    &unary.expr,
                    variables,
                    function_types,
                    module,
                )?;
                Ok(BasicValueEnum::PointerValue(ptr))
            }
            UnaryOp::Negate | UnaryOp::Posate | UnaryOp::LogicalNot => {
                let operand = emit_expr(
                    context,
                    builder,
                    &unary.expr,
                    variables,
                    function_types,
                    module,
                )?;
                let operand_int = match operand {
                    BasicValueEnum::IntValue(i) => i,
                    _ => return Err("expected integer operand for unary operator".to_string()),
                };
                let res = match unary.operator {
                    UnaryOp::Negate => builder
                        .build_int_neg(operand_int, "negtmp")
                        .map_err(|err| format!("failed to emit neg instruction: {err}"))?,
                    UnaryOp::Posate => operand_int,
                    UnaryOp::LogicalNot => {
                        let zero = context.i32_type().const_zero();
                        let cmp = builder
                            .build_int_compare(IntPredicate::EQ, operand_int, zero, "nottmp")
                            .map_err(|err| {
                                format!("failed to emit comparison for logical not: {err}")
                            })?;
                        builder
                            .build_int_z_extend(cmp, context.i32_type(), "casttmp")
                            .map_err(|err| format!("failed to emit zext for logical not: {err}"))?
                    }
                    _ => unreachable!(),
                };
                Ok(BasicValueEnum::IntValue(res))
            }
        },
        Expr::Binary(binary) => {
            let is_logical_op =
                matches!(binary.operator, BinaryOp::LogicalAnd | BinaryOp::LogicalOr);

            if is_logical_op {
                match binary.operator {
                    BinaryOp::LogicalAnd => {
                        let lhs_val = emit_expr(
                            context,
                            builder,
                            &binary.left,
                            variables,
                            function_types,
                            module,
                        )?;
                        let lhs_int = match lhs_val {
                            BasicValueEnum::IntValue(i) => i,
                            _ => return Err("expected integer for logical AND operand".to_string()),
                        };
                        let lhs_is_true = builder
                            .build_int_compare(
                                IntPredicate::NE,
                                lhs_int,
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
                        let rhs_val = emit_expr(
                            context,
                            builder,
                            &binary.right,
                            variables,
                            function_types,
                            module,
                        )?;
                        let rhs_int = match rhs_val {
                            BasicValueEnum::IntValue(i) => i,
                            _ => return Err("expected integer for logical AND operand".to_string()),
                        };
                        let rhs_is_true = builder
                            .build_int_compare(
                                IntPredicate::NE,
                                rhs_int,
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
                        Ok(BasicValueEnum::IntValue(
                            phi.as_basic_value().into_int_value(),
                        ))
                    }
                    BinaryOp::LogicalOr => {
                        let lhs_val = emit_expr(
                            context,
                            builder,
                            &binary.left,
                            variables,
                            function_types,
                            module,
                        )?;
                        let lhs_int = match lhs_val {
                            BasicValueEnum::IntValue(i) => i,
                            _ => return Err("expected integer for logical OR operand".to_string()),
                        };
                        let lhs_is_true = builder
                            .build_int_compare(
                                IntPredicate::NE,
                                lhs_int,
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
                        let rhs_val = emit_expr(
                            context,
                            builder,
                            &binary.right,
                            variables,
                            function_types,
                            module,
                        )?;
                        let rhs_int = match rhs_val {
                            BasicValueEnum::IntValue(i) => i,
                            _ => return Err("expected integer for logical OR operand".to_string()),
                        };
                        let rhs_is_true = builder
                            .build_int_compare(
                                IntPredicate::NE,
                                rhs_int,
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
                        Ok(BasicValueEnum::IntValue(
                            phi.as_basic_value().into_int_value(),
                        ))
                    }
                    _ => unreachable!(),
                }
            } else {
                let left_val = emit_expr(
                    context,
                    builder,
                    &binary.left,
                    variables,
                    function_types,
                    module,
                )?;
                let right_val = emit_expr(
                    context,
                    builder,
                    &binary.right,
                    variables,
                    function_types,
                    module,
                )?;
                let left = match left_val {
                    BasicValueEnum::IntValue(i) => i,
                    _ => return Err("expected integer operand".to_string()),
                };
                let right = match right_val {
                    BasicValueEnum::IntValue(i) => i,
                    _ => return Err("expected integer operand".to_string()),
                };

                let res = match binary.operator {
                    BinaryOp::Add => builder
                        .build_int_add(left, right, "addtmp")
                        .map_err(|err| format!("failed to emit add instruction: {err}"))?,
                    BinaryOp::Subtract => builder
                        .build_int_sub(left, right, "subtmp")
                        .map_err(|err| format!("failed to emit sub instruction: {err}"))?,
                    BinaryOp::Multiply => builder
                        .build_int_mul(left, right, "multmp")
                        .map_err(|err| format!("failed to emit mul instruction: {err}"))?,
                    BinaryOp::Divide => builder
                        .build_int_signed_div(left, right, "divtmp")
                        .map_err(|err| format!("failed to emit div instruction: {err}"))?,
                    BinaryOp::Modulo => builder
                        .build_int_signed_rem(left, right, "remtmp")
                        .map_err(|err| format!("failed to emit rem instruction: {err}"))?,
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
                            .map_err(|err| format!("failed to emit zext instruction: {err}"))?
                    }
                    _ => unreachable!(),
                };
                Ok(BasicValueEnum::IntValue(res))
            }
        }
        Expr::Call(call) => {
            let func = module
                .get_function(&call.name)
                .ok_or_else(|| format!("undefined function '{}'", call.name))?;
            let mut arg_vals = Vec::new();
            for arg in &call.args {
                let val = emit_expr(context, builder, arg, variables, function_types, module)?;
                arg_vals.push(val.into());
            }
            let call_site = builder
                .build_call(func, &arg_vals, &call.name)
                .map_err(|err| format!("failed to build call to '{}': {err}", call.name))?;
            let call_val = match call_site.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(val) => val,
                _ => return Err(format!("function '{}' did not return a value", call.name)),
            };
            Ok(call_val)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_statement<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    statement: &Statement,
    variables: &mut Vec<HashMap<String, (Type, PointerValue<'ctx>)>>,
    ret_val_ptr: PointerValue<'ctx>,
    return_bb: inkwell::basic_block::BasicBlock<'ctx>,
    function_types: &HashMap<String, Type>,
    module: &inkwell::module::Module<'ctx>,
) -> Result<(), String> {
    match statement {
        Statement::Declare(decl) => {
            let init_val = emit_expr(
                context,
                builder,
                &decl.init,
                variables,
                function_types,
                module,
            )?;
            let llvm_ty = llvm_type(context, &decl.ty);
            let alloca = builder
                .build_alloca(llvm_ty, &decl.name)
                .map_err(|err| format!("failed to build alloca: {err}"))?;
            builder
                .build_store(alloca, init_val)
                .map_err(|err| format!("failed to build store: {err}"))?;
            if let Some(top_scope) = variables.last_mut() {
                top_scope.insert(decl.name.clone(), (decl.ty.clone(), alloca));
            }
        }
        Statement::Assign(assign) => {
            let val = emit_expr(
                context,
                builder,
                &assign.expr,
                variables,
                function_types,
                module,
            )?;
            let ptr = emit_lvalue(
                context,
                builder,
                &assign.target,
                variables,
                function_types,
                module,
            )?;
            builder
                .build_store(ptr, val)
                .map_err(|err| format!("failed to build store: {err}"))?;
        }
        Statement::Return(ret_stmt) => {
            let val = emit_expr(
                context,
                builder,
                &ret_stmt.expr,
                variables,
                function_types,
                module,
            )?;
            let val_int = match val {
                BasicValueEnum::IntValue(i) => i,
                _ => return Err("function return value must be an integer".to_string()),
            };
            builder
                .build_store(ret_val_ptr, val_int)
                .map_err(|err| format!("failed to store return value: {err}"))?;
            builder
                .build_unconditional_branch(return_bb)
                .map_err(|err| format!("failed to branch to return block: {err}"))?;
        }
        Statement::Block(body) => {
            variables.push(HashMap::new());
            for stmt in body {
                if builder
                    .get_insert_block()
                    .and_then(|bb| bb.get_terminator())
                    .is_some()
                {
                    break;
                }
                emit_statement(
                    context,
                    builder,
                    stmt,
                    variables,
                    ret_val_ptr,
                    return_bb,
                    function_types,
                    module,
                )?;
            }
            variables.pop();
        }
        Statement::If(if_stmt) => {
            let cond_val = emit_expr(
                context,
                builder,
                &if_stmt.cond,
                variables,
                function_types,
                module,
            )?;
            let cond_int = match cond_val {
                BasicValueEnum::IntValue(i) => i,
                _ => return Err("if condition must be an integer".to_string()),
            };
            let cond_is_true = builder
                .build_int_compare(
                    IntPredicate::NE,
                    cond_int,
                    context.i32_type().const_zero(),
                    "ifcond",
                )
                .map_err(|err| err.to_string())?;

            let start_bb = builder.get_insert_block().ok_or("no insert block")?;
            let parent_func = start_bb.get_parent().ok_or("no parent function")?;

            let then_bb = context.append_basic_block(parent_func, "then");
            let else_bb = context.append_basic_block(parent_func, "else");
            let merge_bb = context.append_basic_block(parent_func, "ifcont");

            builder
                .build_conditional_branch(cond_is_true, then_bb, else_bb)
                .map_err(|err| err.to_string())?;

            // Emit then branch
            builder.position_at_end(then_bb);
            emit_statement(
                context,
                builder,
                &if_stmt.then_branch,
                variables,
                ret_val_ptr,
                return_bb,
                function_types,
                module,
            )?;
            if builder
                .get_insert_block()
                .and_then(|bb| bb.get_terminator())
                .is_none()
            {
                builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|err| err.to_string())?;
            }

            // Emit else branch
            builder.position_at_end(else_bb);
            if let Some(else_branch) = &if_stmt.else_branch {
                emit_statement(
                    context,
                    builder,
                    else_branch,
                    variables,
                    ret_val_ptr,
                    return_bb,
                    function_types,
                    module,
                )?;
            }
            if builder
                .get_insert_block()
                .and_then(|bb| bb.get_terminator())
                .is_none()
            {
                builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|err| err.to_string())?;
            }

            // Move to merge block
            builder.position_at_end(merge_bb);
        }
        Statement::While(while_stmt) => {
            let start_bb = builder.get_insert_block().ok_or("no insert block")?;
            let parent_func = start_bb.get_parent().ok_or("no parent function")?;

            let cond_bb = context.append_basic_block(parent_func, "while.cond");
            let body_bb = context.append_basic_block(parent_func, "while.body");
            let merge_bb = context.append_basic_block(parent_func, "while.cont");

            builder
                .build_unconditional_branch(cond_bb)
                .map_err(|err| err.to_string())?;

            // Cond block
            builder.position_at_end(cond_bb);
            let cond_val = emit_expr(
                context,
                builder,
                &while_stmt.cond,
                variables,
                function_types,
                module,
            )?;
            let cond_int = match cond_val {
                BasicValueEnum::IntValue(i) => i,
                _ => return Err("while condition must be an integer".to_string()),
            };
            let cond_is_true = builder
                .build_int_compare(
                    IntPredicate::NE,
                    cond_int,
                    context.i32_type().const_zero(),
                    "whilecond",
                )
                .map_err(|err| err.to_string())?;
            builder
                .build_conditional_branch(cond_is_true, body_bb, merge_bb)
                .map_err(|err| err.to_string())?;

            // Body block
            builder.position_at_end(body_bb);
            emit_statement(
                context,
                builder,
                &while_stmt.body,
                variables,
                ret_val_ptr,
                return_bb,
                function_types,
                module,
            )?;
            if builder
                .get_insert_block()
                .and_then(|bb| bb.get_terminator())
                .is_none()
            {
                builder
                    .build_unconditional_branch(cond_bb)
                    .map_err(|err| err.to_string())?;
            }

            // Continue from merge block
            builder.position_at_end(merge_bb);
        }
        Statement::For(for_stmt) => {
            variables.push(HashMap::new());

            if let Some(init) = &for_stmt.init {
                emit_statement(
                    context,
                    builder,
                    init,
                    variables,
                    ret_val_ptr,
                    return_bb,
                    function_types,
                    module,
                )?;
            }

            let start_bb = builder.get_insert_block().ok_or("no insert block")?;
            let parent_func = start_bb.get_parent().ok_or("no parent function")?;

            let cond_bb = context.append_basic_block(parent_func, "for.cond");
            let body_bb = context.append_basic_block(parent_func, "for.body");
            let step_bb = context.append_basic_block(parent_func, "for.step");
            let merge_bb = context.append_basic_block(parent_func, "for.cont");

            builder
                .build_unconditional_branch(cond_bb)
                .map_err(|err| err.to_string())?;

            // Cond block
            builder.position_at_end(cond_bb);
            let cond_is_true = if let Some(cond_expr) = &for_stmt.cond {
                let cond_val = emit_expr(
                    context,
                    builder,
                    cond_expr,
                    variables,
                    function_types,
                    module,
                )?;
                let cond_int = match cond_val {
                    BasicValueEnum::IntValue(i) => i,
                    _ => return Err("for condition must be an integer".to_string()),
                };
                builder
                    .build_int_compare(
                        IntPredicate::NE,
                        cond_int,
                        context.i32_type().const_zero(),
                        "forcond",
                    )
                    .map_err(|err| err.to_string())?
            } else {
                context.bool_type().const_int(1, false)
            };
            builder
                .build_conditional_branch(cond_is_true, body_bb, merge_bb)
                .map_err(|err| err.to_string())?;

            // Body block
            builder.position_at_end(body_bb);
            emit_statement(
                context,
                builder,
                &for_stmt.body,
                variables,
                ret_val_ptr,
                return_bb,
                function_types,
                module,
            )?;
            if builder
                .get_insert_block()
                .and_then(|bb| bb.get_terminator())
                .is_none()
            {
                builder
                    .build_unconditional_branch(step_bb)
                    .map_err(|err| err.to_string())?;
            }

            // Step block
            builder.position_at_end(step_bb);
            if let Some(post) = &for_stmt.post {
                emit_statement(
                    context,
                    builder,
                    post,
                    variables,
                    ret_val_ptr,
                    return_bb,
                    function_types,
                    module,
                )?;
            }
            builder
                .build_unconditional_branch(cond_bb)
                .map_err(|err| err.to_string())?;

            // Continue from merge block
            builder.position_at_end(merge_bb);

            variables.pop();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::generate_ir;
    use crate::ast::{
        BinaryExpr, BinaryOp, Expr, Function, IntegerLiteral, Program, ReturnStatement, Statement,
        Type, UnaryExpr, UnaryOp, VarAssignStatement, VarDeclareStatement, VariableExpr,
    };

    #[test]
    fn generates_llvm_ir_for_integer_return() {
        let program = Program {
            functions: vec![Function {
                name: "main".to_string(),
                return_type: Type::Int,
                params: vec![],
                body: vec![Statement::Return(ReturnStatement {
                    expr: Expr::IntegerLiteral(IntegerLiteral { value: 5 }),
                })],
            }],
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("store i32 5, ptr %retval"));
        assert!(ir.contains("ret i32"));
    }

    #[test]
    fn generates_llvm_ir_for_arithmetic_expression() {
        let program = Program {
            functions: vec![Function {
                name: "main".to_string(),
                return_type: Type::Int,
                params: vec![],
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
            }],
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("store i32 12, ptr %retval"));
        assert!(ir.contains("ret i32"));
    }

    #[test]
    fn generates_llvm_ir_for_unary_negation() {
        let program = Program {
            functions: vec![Function {
                name: "main".to_string(),
                return_type: Type::Int,
                params: vec![],
                body: vec![Statement::Return(ReturnStatement {
                    expr: Expr::Unary(UnaryExpr {
                        operator: UnaryOp::Negate,
                        expr: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 5 })),
                    }),
                })],
            }],
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("store i32 -5, ptr %retval"));
        assert!(ir.contains("ret i32"));
    }

    #[test]
    fn generates_llvm_ir_for_variables() {
        let program = Program {
            functions: vec![Function {
                name: "main".to_string(),
                return_type: Type::Int,
                params: vec![],
                body: vec![
                    Statement::Declare(VarDeclareStatement {
                        name: "x".to_string(),
                        ty: Type::Int,
                        init: Expr::IntegerLiteral(IntegerLiteral { value: 42 }),
                    }),
                    Statement::Assign(VarAssignStatement {
                        target: Expr::Variable(VariableExpr {
                            name: "x".to_string(),
                        }),
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
            }],
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
            functions: vec![Function {
                name: "main".to_string(),
                return_type: Type::Int,
                params: vec![],
                body: vec![
                    Statement::Declare(VarDeclareStatement {
                        name: "x".to_string(),
                        ty: Type::Int,
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
            }],
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("icmp slt i32"));
        assert!(ir.contains("zext i1") || ir.contains("cast"));
        assert!(ir.contains("ret i32"));
    }

    #[test]
    fn generates_llvm_ir_for_logical_and() {
        let program = Program {
            functions: vec![Function {
                name: "main".to_string(),
                return_type: Type::Int,
                params: vec![],
                body: vec![Statement::Return(ReturnStatement {
                    expr: Expr::Binary(BinaryExpr {
                        left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 1 })),
                        operator: BinaryOp::LogicalAnd,
                        right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                    }),
                })],
            }],
        };

        let ir = generate_ir(&program).expect("should generate LLVM IR");

        assert!(ir.contains("and.rhs:"));
        assert!(ir.contains("and.merge:"));
        assert!(ir.contains("phi i32"));
    }
}
