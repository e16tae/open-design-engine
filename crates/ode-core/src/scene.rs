use ode_format::color::Color;
use ode_format::node::FillRule;
use ode_format::style::BlendMode;

/// Flat list of render commands produced by converting a Document.
#[derive(Debug, Clone)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub commands: Vec<RenderCommand>,
}

#[derive(Debug, Clone)]
pub enum RenderCommand {
    /// Begin a new compositing layer.
    /// `transform` is used ONLY for transforming the clip path when building the Mask.
    PushLayer {
        opacity: f32,
        blend_mode: BlendMode,
        clip: Option<kurbo::BezPath>,
        transform: tiny_skia::Transform,
    },
    /// End current layer — composite temp Pixmap into parent.
    PopLayer,
    /// Fill a path.
    FillPath {
        path: kurbo::BezPath,
        paint: ResolvedPaint,
        fill_rule: FillRule,
        transform: tiny_skia::Transform,
    },
    /// Stroke a path.
    StrokePath {
        path: kurbo::BezPath,
        paint: ResolvedPaint,
        stroke: StrokeStyle,
        transform: tiny_skia::Transform,
    },
    /// Apply an effect to the current layer.
    ApplyEffect {
        effect: ResolvedEffect,
    },
}

/// Token-resolved paint. The renderer never sees StyleValue or TokenRef.
#[derive(Debug, Clone)]
pub enum ResolvedPaint {
    Solid(Color),
    LinearGradient {
        stops: Vec<ResolvedGradientStop>,
        start: kurbo::Point,
        end: kurbo::Point,
    },
    RadialGradient {
        stops: Vec<ResolvedGradientStop>,
        center: kurbo::Point,
        radius: kurbo::Point,
    },
    AngularGradient {
        stops: Vec<ResolvedGradientStop>,
        center: kurbo::Point,
        angle: f32,
    },
    DiamondGradient {
        stops: Vec<ResolvedGradientStop>,
        center: kurbo::Point,
        radius: kurbo::Point,
    },
}

#[derive(Debug, Clone)]
pub struct ResolvedGradientStop {
    pub position: f32,
    pub color: Color,
}

#[derive(Debug, Clone)]
pub enum ResolvedEffect {
    DropShadow { color: Color, offset_x: f32, offset_y: f32, blur_radius: f32, spread: f32, shape: kurbo::BezPath },
    InnerShadow { color: Color, offset_x: f32, offset_y: f32, blur_radius: f32, spread: f32, shape: kurbo::BezPath },
    LayerBlur { radius: f32 },
    BackgroundBlur { radius: f32 },
}

#[derive(Debug, Clone)]
pub struct StrokeStyle {
    pub width: f32,
    pub position: ode_format::style::StrokePosition,
    pub cap: ode_format::style::StrokeCap,
    pub join: ode_format::style::StrokeJoin,
    pub miter_limit: f32,
    pub dash: Option<ode_format::style::DashPattern>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_can_hold_commands() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: kurbo::BezPath::new(),
                    paint: ResolvedPaint::Solid(Color::black()),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        assert_eq!(scene.commands.len(), 3);
    }

    #[test]
    fn resolved_paint_variants() {
        let _solid = ResolvedPaint::Solid(Color::white());
        let _linear = ResolvedPaint::LinearGradient {
            stops: vec![
                ResolvedGradientStop { position: 0.0, color: Color::black() },
                ResolvedGradientStop { position: 1.0, color: Color::white() },
            ],
            start: kurbo::Point::new(0.0, 0.0),
            end: kurbo::Point::new(100.0, 0.0),
        };
        let _angular = ResolvedPaint::AngularGradient {
            stops: vec![],
            center: kurbo::Point::new(50.0, 50.0),
            angle: 0.0,
        };
    }
}
