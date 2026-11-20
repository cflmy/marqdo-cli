//! Resource budgets: fuel, stack depth, output bytes and wall-clock deadline.

use std::time::Instant;

use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError};

/// Default evaluation fuel for one run.
pub const DEFAULT_FUEL: u64 = 5_000_000;
/// Default call-stack depth.
pub const DEFAULT_STACK_LIMIT: usize = 256;
/// Default program output limit (bytes).
pub const DEFAULT_OUTPUT_BYTES: usize = 1 << 20;

/// Bounded resources for a run.
#[derive(Debug, Clone)]
pub struct Budget {
    /// Remaining evaluation fuel.
    pub fuel: u64,
    /// Maximum call-stack depth.
    pub stack_limit: usize,
    /// Maximum program output in bytes.
    pub max_output_bytes: usize,
    /// Output bytes produced so far.
    pub output_bytes: usize,
    /// Optional wall-clock deadline.
    pub deadline: Option<Instant>,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            fuel: DEFAULT_FUEL,
            stack_limit: DEFAULT_STACK_LIMIT,
            max_output_bytes: DEFAULT_OUTPUT_BYTES,
            output_bytes: 0,
            deadline: None,
        }
    }
}

impl Budget {
    /// Consume fuel, failing with `resource.exhausted` when it runs out.
    pub fn consume_fuel(&mut self, amount: u64, span: Option<Span>) -> Result<(), RuntimeError> {
        if amount > self.fuel {
            self.fuel = 0;
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                "evaluation fuel exhausted",
            ));
        }
        self.fuel -= amount;
        Ok(())
    }

    /// Remaining fuel.
    pub fn remaining_fuel(&self) -> u64 {
        self.fuel
    }

    /// Check call-stack depth.
    pub fn check_stack(&self, depth: usize, span: Option<Span>) -> Result<(), RuntimeError> {
        if depth > self.stack_limit {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "call stack depth limit ({} frames) exceeded",
                    self.stack_limit
                ),
            ));
        }
        Ok(())
    }

    /// Account for program output, failing when the limit is exceeded.
    pub fn consume_output(
        &mut self,
        amount: usize,
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        let next = self.output_bytes.saturating_add(amount);
        if next > self.max_output_bytes {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "program output limit ({} bytes) exceeded",
                    self.max_output_bytes
                ),
            ));
        }
        self.output_bytes = next;
        Ok(())
    }

    /// Check the wall-clock deadline.
    pub fn check_deadline(&self, span: Option<Span>) -> Result<(), RuntimeError> {
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                return Err(RuntimeError::new(
                    ErrorCategory::ResourceExhausted,
                    span,
                    "wall-clock time limit exceeded",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuel_is_bounded() {
        let mut budget = Budget {
            fuel: 10,
            ..Budget::default()
        };
        assert!(budget.consume_fuel(4, None).is_ok());
        assert_eq!(budget.remaining_fuel(), 6);
        assert_eq!(
            budget.consume_fuel(7, None).unwrap_err().category,
            ErrorCategory::ResourceExhausted
        );
    }

    #[test]
    fn output_and_stack_limits() {
        let mut budget = Budget {
            max_output_bytes: 5,
            stack_limit: 2,
            ..Budget::default()
        };
        assert!(budget.consume_output(5, None).is_ok());
        assert!(budget.consume_output(1, None).is_err());
        assert!(budget.check_stack(2, None).is_ok());
        assert!(budget.check_stack(3, None).is_err());
    }
}
