pub mod contrast_ratio;
pub mod min_value;
pub mod spacing_scale;

use crate::checker::CheckerRegistry;

/// Create a registry pre-populated with all built-in checkers.
pub fn default_registry() -> CheckerRegistry {
    let mut registry = CheckerRegistry::new();
    registry.register(Box::new(contrast_ratio::ContrastRatioChecker));
    registry.register(Box::new(min_value::MinValueChecker));
    registry.register(Box::new(spacing_scale::SpacingScaleChecker));
    registry
}
