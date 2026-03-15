use ode_format::style::BlendMode;

/// Map ODE BlendMode to tiny-skia BlendMode.
pub fn to_skia_blend(mode: BlendMode) -> tiny_skia::BlendMode {
    match mode {
        BlendMode::Normal => tiny_skia::BlendMode::SourceOver,
        BlendMode::Multiply => tiny_skia::BlendMode::Multiply,
        BlendMode::Screen => tiny_skia::BlendMode::Screen,
        BlendMode::Overlay => tiny_skia::BlendMode::Overlay,
        BlendMode::Darken => tiny_skia::BlendMode::Darken,
        BlendMode::Lighten => tiny_skia::BlendMode::Lighten,
        BlendMode::ColorDodge => tiny_skia::BlendMode::ColorDodge,
        BlendMode::ColorBurn => tiny_skia::BlendMode::ColorBurn,
        BlendMode::HardLight => tiny_skia::BlendMode::HardLight,
        BlendMode::SoftLight => tiny_skia::BlendMode::SoftLight,
        BlendMode::Difference => tiny_skia::BlendMode::Difference,
        BlendMode::Exclusion => tiny_skia::BlendMode::Exclusion,
        BlendMode::Hue => tiny_skia::BlendMode::Hue,
        BlendMode::Saturation => tiny_skia::BlendMode::Saturation,
        BlendMode::Color => tiny_skia::BlendMode::Color,
        BlendMode::Luminosity => tiny_skia::BlendMode::Luminosity,
    }
}

/// Map ODE FillRule to tiny-skia FillRule.
pub fn to_skia_fill_rule(rule: ode_format::node::FillRule) -> tiny_skia::FillRule {
    match rule {
        ode_format::node::FillRule::NonZero => tiny_skia::FillRule::Winding,
        ode_format::node::FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_blend_modes_map() {
        let modes = [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::HardLight,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Exclusion,
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Luminosity,
        ];
        for mode in modes {
            let _ = to_skia_blend(mode); // Should not panic
        }
    }

    #[test]
    fn normal_maps_to_source_over() {
        assert!(matches!(
            to_skia_blend(BlendMode::Normal),
            tiny_skia::BlendMode::SourceOver
        ));
    }

    #[test]
    fn fill_rule_mapping() {
        assert!(matches!(
            to_skia_fill_rule(ode_format::node::FillRule::NonZero),
            tiny_skia::FillRule::Winding
        ));
        assert!(matches!(
            to_skia_fill_rule(ode_format::node::FillRule::EvenOdd),
            tiny_skia::FillRule::EvenOdd
        ));
    }
}
