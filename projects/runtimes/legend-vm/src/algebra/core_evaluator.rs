//! Dictionary-environment algebraic evaluator.

use std::collections::HashMap;

use super::BuiltinFunction;
use crate::value::LegacyValue;

/// Algebraic evaluator backed by a variable environment.
#[derive(Debug, Clone)]
pub struct CoreEvaluator {
    env: HashMap<String, LegacyValue>,
}

impl CoreEvaluator {
    /// Create an evaluator with an optional initial environment.
    pub fn new(env: Option<HashMap<String, LegacyValue>>) -> Self {
        Self { env: env.unwrap_or_default() }
    }

    /// Borrow the live environment.
    pub fn env(&self) -> &HashMap<String, LegacyValue> {
        &self.env
    }

    /// Borrow the live environment mutably.
    pub fn env_mut(&mut self) -> &mut HashMap<String, LegacyValue> {
        &mut self.env
    }

    /// Unit / null value.
    pub fn unit(&self) -> LegacyValue {
        LegacyValue::Null
    }

    /// Integer constant.
    pub fn int_const(&self, value: i64) -> LegacyValue {
        LegacyValue::Int(value)
    }

    /// Floating-point constant.
    pub fn float_const(&self, value: f64) -> LegacyValue {
        LegacyValue::Float(value)
    }

    /// Boolean constant.
    pub fn bool_const(&self, value: bool) -> LegacyValue {
        LegacyValue::Bool(value)
    }

    /// String constant.
    pub fn str_const(&self, value: impl Into<String>) -> LegacyValue {
        LegacyValue::String(value.into())
    }

    /// Read a variable.
    pub fn var(&self, name: &str) -> LegacyValue {
        self.env.get(name).cloned().unwrap_or(LegacyValue::Null)
    }

    /// Assign a variable.
    pub fn set_var(&mut self, name: impl Into<String>, value: LegacyValue) -> LegacyValue {
        let name = name.into();
        self.env.insert(name, value.clone());
        value
    }

    /// Addition (string concat or numeric add).
    pub fn add(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        if let (LegacyValue::String(left), LegacyValue::String(right)) = (left, right) {
            return LegacyValue::String(format!("{left}{right}"));
        }

        let result = left.to_f64() + right.to_f64();
        if is_integral(left) && is_integral(right) { LegacyValue::Int(result as i64) } else { LegacyValue::Float(result) }
    }

    /// Subtraction.
    pub fn sub(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        let result = left.to_f64() - right.to_f64();
        if is_integral(left) && is_integral(right) { LegacyValue::Int(result as i64) } else { LegacyValue::Float(result) }
    }

    /// Multiplication.
    pub fn mul(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        let result = left.to_f64() * right.to_f64();
        if is_integral(left) && is_integral(right) { LegacyValue::Int(result as i64) } else { LegacyValue::Float(result) }
    }

    /// Division.
    pub fn div(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        let divisor = right.to_f64();
        if divisor == 0.0 {
            return LegacyValue::Float(f64::NAN);
        }
        let result = left.to_f64() / divisor;
        if is_integral(left) && is_integral(right) { LegacyValue::Int(result as i64) } else { LegacyValue::Float(result) }
    }

    /// Modulo.
    pub fn modulo(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        let divisor = right.to_i64();
        if divisor == 0 {
            return LegacyValue::Int(0);
        }
        LegacyValue::Int(left.to_i64() % divisor)
    }

    /// Equality.
    pub fn eq(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(eq_impl(left, right))
    }

    /// Inequality.
    pub fn ne(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(!eq_impl(left, right))
    }

    /// Less than.
    pub fn lt(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(left.to_f64() < right.to_f64())
    }

    /// Greater than.
    pub fn gt(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(left.to_f64() > right.to_f64())
    }

    /// Less than or equal.
    pub fn lte(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(left.to_f64() <= right.to_f64())
    }

    /// Greater than or equal.
    pub fn gte(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(left.to_f64() >= right.to_f64())
    }

    /// Logical and.
    pub fn and(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(left.to_bool() && right.to_bool())
    }

    /// Logical or.
    pub fn or(&self, left: &LegacyValue, right: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(left.to_bool() || right.to_bool())
    }

    /// Logical not.
    pub fn not(&self, operand: &LegacyValue) -> LegacyValue {
        LegacyValue::Bool(!operand.to_bool())
    }

    /// Conditional expression.
    pub fn if_then(&self, condition: &LegacyValue, then_branch: LegacyValue, else_branch: LegacyValue) -> LegacyValue {
        if condition.to_bool() { then_branch } else { else_branch }
    }

    /// Execute a block and return the last value.
    pub fn block(&self, statements: &[LegacyValue]) -> LegacyValue {
        statements.last().cloned().unwrap_or(LegacyValue::Null)
    }

    /// Create a lambda closure.
    pub fn lambda(&self, parameters: Vec<String>, body: LegacyValue) -> LegacyValue {
        LegacyValue::Lambda { params: parameters, body: Box::new(body), env: self.env.clone() }
    }

    /// Apply a callable.
    pub fn apply(&mut self, func: &LegacyValue, args: &[LegacyValue]) -> LegacyValue {
        match func {
            LegacyValue::Lambda { params, body, env } => {
                let saved = std::mem::take(&mut self.env);
                self.env.extend(env.clone());
                for (index, param) in params.iter().enumerate() {
                    if let Some(arg) = args.get(index) {
                        self.env.insert(param.clone(), arg.clone());
                    }
                }
                let result = body.as_ref().clone();
                self.env = saved;
                result
            }
            LegacyValue::Builtin { func, .. } => func(args),
            _ => LegacyValue::Null,
        }
    }

    /// Apply a [`BuiltinFunction`].
    pub fn apply_builtin(&self, func: &BuiltinFunction, args: &[LegacyValue]) -> LegacyValue {
        func.invoke(args)
    }

    /// Create a return sentinel.
    pub fn ret(&self, value: LegacyValue) -> LegacyValue {
        LegacyValue::Return(Box::new(value))
    }

    /// Unwrap a return sentinel at module top-level.
    pub fn eval_with_return_unwrap(&self, value: LegacyValue) -> LegacyValue {
        match value {
            LegacyValue::Return(inner) => *inner,
            other => other,
        }
    }

    /// While loop with break/continue/return support.
    pub fn while_loop<C, B>(&mut self, mut condition: C, mut body: B) -> LegacyValue
    where
        C: FnMut(&mut Self) -> LegacyValue,
        B: FnMut(&mut Self) -> LegacyValue,
    {
        let mut last = LegacyValue::Null;
        while condition(self).to_bool() {
            match body(self) {
                LegacyValue::Break => break,
                LegacyValue::Continue => continue,
                LegacyValue::Return(value) => return LegacyValue::Return(value),
                value => last = value,
            }
        }
        last
    }
}

fn is_integral(value: &LegacyValue) -> bool {
    matches!(value, LegacyValue::Int(_))
}

fn eq_impl(left: &LegacyValue, right: &LegacyValue) -> bool {
    match (left, right) {
        (LegacyValue::Null, LegacyValue::Null) => true,
        (LegacyValue::Bool(left), LegacyValue::Bool(right)) => left == right,
        (LegacyValue::String(left), LegacyValue::String(right)) => left == right,
        (LegacyValue::Int(_) | LegacyValue::Float(_), LegacyValue::Int(_) | LegacyValue::Float(_)) => {
            (left.to_f64() - right.to_f64()).abs() < f64::EPSILON
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_and_loops() {
        let mut core = CoreEvaluator::new(None);
        core.set_var("x", core.int_const(0));
        let result = core.while_loop(
            |core| core.lt(&core.var("x"), &core.int_const(3)),
            |core| {
                let next = core.add(&core.var("x"), &core.int_const(1));
                core.set_var("x", next.clone());
                next
            },
        );
        assert_eq!(result, LegacyValue::Int(3));
    }
}
