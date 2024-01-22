use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::parser::ast::{
    BlockStatement, Expression, HashLiteralExpression, IdentifierExpression, IfExpression, Node,
    Statement,
};
use environment::Environment;
use object::{HashKey, HashPair, Object};

pub mod environment;
pub mod object;
mod test_evaluator;

type EvalError = String;

pub fn eval(node: Node, env: Rc<RefCell<Environment>>) -> Object {
    match node {
        Node::Program(prgm) => match eval_program(&prgm.0, env) {
            Ok(evaluated) => evaluated,
            Err(err) => Object::Error(err),
        },
        Node::Statement(stmt) => match eval_statement(&stmt, env) {
            Ok(evaluated) => evaluated,
            Err(err) => Object::Error(err),
        },
        Node::Expression(expr) => match eval_expression(&expr, env) {
            Ok(evaluated) => evaluated,
            Err(err) => Object::Error(err),
        },
    }
}

fn eval_program(stmts: &[Statement], env: Rc<RefCell<Environment>>) -> Result<Object, EvalError> {
    let mut result = Object::Null;

    for stmt in stmts.iter() {
        result = eval_statement(stmt, env.clone())?;

        if let Object::ReturnValue(value) = result {
            return Ok(*value);
        }
    }
    Ok(result)
}

fn eval_block_statement(
    stmts: &BlockStatement,
    env: Rc<RefCell<Environment>>,
) -> Result<Object, EvalError> {
    let mut result = Object::Null;

    for stmt in stmts.statements.iter() {
        result = eval_statement(stmt, env.clone())?;

        if let Object::ReturnValue(_) = result {
            return Ok(result);
        }
    }
    Ok(result)
}

fn eval_statement(stmt: &Statement, env: Rc<RefCell<Environment>>) -> Result<Object, EvalError> {
    match stmt {
        Statement::Let(stmt) => {
            let val = eval_expression(&stmt.value, env.clone())?;
            env.borrow_mut().set(stmt.identifier.name.to_owned(), val);
            Ok(Object::Null)
        }
        Statement::Return(stmt) => {
            let value = eval_expression(&stmt.value, env)?;
            Ok(Object::ReturnValue(Box::new(value)))
        }
        Statement::Expression(expr) => eval_expression(&expr.expr, env),
    }
}

fn eval_expression(expr: &Expression, env: Rc<RefCell<Environment>>) -> Result<Object, EvalError> {
    match expr {
        Expression::Identifier(expr) => eval_identifier(expr, env),
        Expression::Integer(expr) => Ok(Object::Integer(expr.value)),
        Expression::Boolean(expr) => Ok(get_bool_object(expr.value)),
        Expression::String(expr) => Ok(Object::String(expr.value.to_owned())),
        Expression::Prefix(expr) => {
            let rhs = eval_expression(&expr.operand, env)?;
            eval_prefix_expression(expr.operator.get_literal(), &rhs)
        }
        Expression::Infix(expr) => {
            let lhs = eval_expression(&expr.lhs, env.clone())?;
            let rhs = eval_expression(&expr.rhs, env.clone())?;
            eval_infix_expression(expr.operator.get_literal(), &lhs, &rhs)
        }
        Expression::If(expr) => eval_if_expression(expr, env),
        Expression::FnLiteral(expr) => Ok(Object::Function {
            parameters: expr.parameters.to_owned(),
            body: expr.body.to_owned(),
            env,
        }),
        Expression::ArrayLiteral(expr) => Ok(Object::Array(eval_expressions(&expr.elements, env)?)),
        Expression::HashLiteral(expr) => eval_hash_literal(expr, env),
        Expression::Call(expr) => {
            let function = eval_expression(&expr.function, env.clone())?;
            let args = eval_expressions(&expr.arguments, env.clone())?;
            if args.len() == 1 {
                if let Object::Error(_) = args[0] {
                    return Ok(args[0].to_owned());
                }
            }
            apply_function(&function, &args)
        }
        Expression::Index(expr) => {
            let identifier = eval_expression(&expr.identifier, env.clone())?;
            let index = eval_expression(&expr.index, env)?;
            eval_index_expression(&identifier, &index)
        }
    }
}

fn eval_expressions(
    exprs: &[Expression],
    env: Rc<RefCell<Environment>>,
) -> Result<Vec<Object>, EvalError> {
    let mut result = Vec::new();

    for expr in exprs.iter() {
        let evaluated = eval_expression(expr, env.clone())?;
        result.push(evaluated);
    }
    Ok(result)
}

fn eval_prefix_expression(prefix: String, expr: &Object) -> Result<Object, EvalError> {
    match prefix {
        prefix if prefix == *"!" => eval_bang_operator_expression(expr),
        prefix if prefix == *"-" => eval_minus_operator_expression(expr),
        _ => Err(format!(
            "unknown operator: {}{}",
            prefix,
            expr.get_type_str()
        )),
    }
}

fn eval_bang_operator_expression(expr: &Object) -> Result<Object, EvalError> {
    match expr {
        Object::Boolean(true) => Ok(Object::Boolean(false)),
        Object::Boolean(false) => Ok(Object::Boolean(true)),
        Object::Null => Ok(Object::Boolean(true)),
        _ => Ok(Object::Boolean(false)),
    }
}

fn eval_minus_operator_expression(expr: &Object) -> Result<Object, EvalError> {
    match expr {
        Object::Integer(value) => Ok(Object::Integer(-value)),
        _ => Err(format!("unknown operator: -{}", expr.get_type_str())),
    }
}

fn eval_infix_expression(
    operator: String,
    lhs: &Object,
    rhs: &Object,
) -> Result<Object, EvalError> {
    match (&lhs, &rhs) {
        (Object::Integer(lhs_value), Object::Integer(rhs_value)) => {
            eval_integer_infix_expression(&operator, lhs_value, rhs_value)
        }
        (Object::Boolean(lhs_value), Object::Boolean(rhs_value)) => {
            eval_boolean_infix_expression(&operator, lhs_value, rhs_value)
        }
        (Object::String(lhs_value), Object::String(rhs_value)) => {
            eval_string_infix_expression(&operator, lhs_value, rhs_value)
        }
        _ => Err(format!(
            "unknown operator: {} {} {}",
            lhs.get_type_str(),
            operator,
            rhs.get_type_str(),
        )),
    }
}

fn eval_integer_infix_expression(
    operator: &str,
    lhs: &i32,
    rhs: &i32,
) -> Result<Object, EvalError> {
    match operator {
        "+" => Ok(Object::Integer(lhs + rhs)),
        "-" => Ok(Object::Integer(lhs - rhs)),
        "*" => Ok(Object::Integer(lhs * rhs)),
        "/" => Ok(Object::Integer(lhs / rhs)),
        "<" => Ok(get_bool_object(lhs < rhs)),
        ">" => Ok(get_bool_object(lhs > rhs)),
        "==" => Ok(get_bool_object(lhs == rhs)),
        "!=" => Ok(get_bool_object(lhs != rhs)),
        _ => Err(format!("unknown operator: INTEGER {} INTEGER", operator,)),
    }
}

fn eval_boolean_infix_expression(
    operator: &str,
    lhs: &bool,
    rhs: &bool,
) -> Result<Object, EvalError> {
    match operator {
        "==" => Ok(get_bool_object(lhs == rhs)),
        "!=" => Ok(get_bool_object(lhs != rhs)),
        _ => Err(format!("unknown operator: BOOLEAN {} BOOLEAN", operator,)),
    }
}

fn eval_string_infix_expression(operator: &str, lhs: &str, rhs: &str) -> Result<Object, EvalError> {
    match operator {
        "+" => Ok(Object::String([lhs, rhs].join(""))),
        _ => Err(format!("unknown operator: STRING {} STRING", operator,)),
    }
}

fn eval_if_expression(
    expr: &IfExpression,
    env: Rc<RefCell<Environment>>,
) -> Result<Object, EvalError> {
    let condition = eval_expression(&expr.condition, env.clone())?;

    if is_truthy(&condition) {
        eval_block_statement(&expr.consequence, env)
    } else if let Some(alternative) = &expr.alternative {
        eval_block_statement(alternative, env)
    } else {
        Ok(Object::Null)
    }
}

fn eval_identifier(
    identifier: &IdentifierExpression,
    env: Rc<RefCell<Environment>>,
) -> Result<Object, EvalError> {
    let value = &identifier.name;
    match env.borrow().get(value) {
        Some(val) => Ok(val),
        None => match get_builtin_fn(value) {
            Some(builtin) => Ok(builtin),
            None => Err(format!("identifier not found: {}", value)),
        },
    }
}

fn apply_function(function: &Object, args: &Vec<Object>) -> Result<Object, EvalError> {
    match function {
        Object::Function {
            parameters,
            body,
            env,
        } => {
            let extended_env = Rc::new(RefCell::new(extend_function_env(
                parameters,
                env.clone(),
                args,
            )));
            let evaluated = eval_block_statement(body, extended_env)?;
            if let Object::ReturnValue(value) = evaluated {
                return Ok(*value);
            }
            Ok(evaluated)
        }
        Object::Builtin(builtin) => Ok(builtin(args.to_owned())),
        _ => Err(format!("not a function: {}", function.get_type_str(),)),
    }
}

fn extend_function_env(
    parameters: &[IdentifierExpression],
    env: Rc<RefCell<Environment>>,
    args: &[Object],
) -> Environment {
    let mut env = env.borrow().clone().new_enclosed();

    for (i, param) in parameters.iter().enumerate() {
        env.set(param.name.to_owned(), args[i].to_owned());
    }
    env
}

fn eval_index_expression(identifier: &Object, index: &Object) -> Result<Object, EvalError> {
    match (&identifier, &index) {
        (Object::Array(array), Object::Integer(integer)) => {
            eval_array_index_expression(array, *integer as usize)
        }
        (Object::Hash(hash), index) => eval_hash_index_expression(hash, index),
        _ => Err(format!(
            "index operator not supported: {}",
            identifier.get_type_str()
        )),
    }
}

fn eval_array_index_expression(array: &[Object], index: usize) -> Result<Object, EvalError> {
    if index > array.len() - 1 {
        return Ok(Object::Null);
    }

    Ok(array[index].to_owned())
}

fn eval_hash_index_expression(
    hash: &BTreeMap<HashKey, HashPair>,
    index: &Object,
) -> Result<Object, EvalError> {
    if let Some(hash_key) = index.get_hash_key() {
        if let Some(pair) = hash.get(&hash_key) {
            Ok(pair.value.clone())
        } else {
            Ok(Object::Null)
        }
    } else {
        Err(format!("unusable as hash key: {}", index.get_type_str()))
    }
}

fn eval_hash_literal(
    hash_literal: &HashLiteralExpression,
    env: Rc<RefCell<Environment>>,
) -> Result<Object, EvalError> {
    let mut pairs = BTreeMap::new();

    for (key_expr, value_expr) in hash_literal.pairs.iter() {
        let key = eval_expression(key_expr, env.clone())?;
        match key.get_hash_key() {
            Some(hash_key) => {
                let value = eval_expression(value_expr, env.clone())?;
                pairs.insert(hash_key, HashPair { key, value });
            }
            None => {
                return Err(format!("unusable as hash key: {}", key.get_type_str()));
            }
        }
    }

    Ok(Object::Hash(pairs))
}

fn new_error(message: String) -> Object {
    Object::Error(message)
}

fn is_truthy(object: &Object) -> bool {
    !matches!(object, Object::Boolean(false) | Object::Null)
}

fn get_bool_object(expr: bool) -> Object {
    if expr {
        Object::Boolean(true)
    } else {
        Object::Boolean(false)
    }
}

fn get_builtin_fn(name: &str) -> Option<Object> {
    match name {
        "len" => Some(Object::Builtin(|objs| {
            if objs.len() != 1 {
                return new_error(format!(
                    "wrong number of arguments: expected 1, found {}",
                    objs.len()
                ));
            }

            match &objs[0] {
                Object::String(string) => Object::Integer(string.len() as i32),
                Object::Array(array) => Object::Integer(array.len() as i32),
                _ => new_error(format!(
                    "argument to 'len' not supported, found {}",
                    objs[0].get_type_str()
                )),
            }
        })),
        "first" => Some(Object::Builtin(|objs| {
            if objs.len() != 1 {
                return new_error(format!(
                    "wrong number of arguments: expected 1, found {}",
                    objs.len()
                ));
            }

            match &objs[0] {
                Object::Array(elements) => {
                    if !elements.is_empty() {
                        elements[0].to_owned()
                    } else {
                        Object::Null
                    }
                }
                _ => new_error(format!(
                    "argument to 'first' must be ARRAY, found {}",
                    objs[0].get_type_str()
                )),
            }
        })),
        "last" => Some(Object::Builtin(|objs| {
            if objs.len() != 1 {
                return new_error(format!(
                    "wrong number of arguments: expected 1, found {}",
                    objs.len()
                ));
            }

            match &objs[0] {
                Object::Array(elements) => elements.last().unwrap_or(&Object::Null).clone(),
                _ => new_error(format!(
                    "argument to 'last' must be ARRAY, found {}",
                    objs[0].get_type_str()
                )),
            }
        })),
        "rest" => Some(Object::Builtin(|objs| {
            if objs.len() != 1 {
                return new_error(format!(
                    "wrong number of arguments: expected 1, found {}",
                    objs.len()
                ));
            }

            match &objs[0] {
                Object::Array(elements) => {
                    if !elements.is_empty() {
                        Object::Array(elements[1..].to_owned())
                    } else {
                        Object::Null
                    }
                }
                _ => new_error(format!(
                    "argument to 'rest' must be ARRAY, found {}",
                    objs[0].get_type_str()
                )),
            }
        })),
        "push" => Some(Object::Builtin(|objs| {
            if objs.len() != 2 {
                return new_error(format!(
                    "wrong number of arguments: expected 2, found {}",
                    objs.len()
                ));
            }

            match &objs[0] {
                Object::Array(elements) => {
                    let mut elements = elements.clone();
                    elements.push(objs[1].clone());
                    Object::Array(elements)
                }
                _ => new_error(format!(
                    "argument to 'push' must be ARRAY, found {}",
                    objs[0].get_type_str()
                )),
            }
        })),
        "puts" => Some(Object::Builtin(|objs| {
            for obj in objs.iter() {
                println!("{}", obj);
            }

            Object::Null
        })),
        _ => None,
    }
}
