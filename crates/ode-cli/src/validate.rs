use crate::output::{ValidateResponse, ValidationIssue, Warning};
use ode_format::wire::{DocumentWire, NodeKindWire, ViewKindWire};
use std::collections::{HashMap, HashSet};

pub fn validate_json(json: &str) -> ValidateResponse {
    // Phase 1: Parse
    let wire: DocumentWire = match serde_json::from_str(json) {
        Ok(w) => w,
        Err(e) => {
            return ValidateResponse {
                valid: false,
                errors: vec![ValidationIssue {
                    path: String::new(),
                    code: "PARSE_FAILED".to_string(),
                    message: e.to_string(),
                    suggestion: None,
                }],
                warnings: vec![],
            };
        }
    };

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Collect all stable_ids
    let mut id_set: HashSet<&str> = HashSet::new();
    let mut all_ids: Vec<&str> = Vec::new();
    for (i, node) in wire.nodes.iter().enumerate() {
        if !id_set.insert(&node.stable_id) {
            errors.push(ValidationIssue {
                path: format!("nodes[{i}].stable_id"),
                code: "DUPLICATE_ID".to_string(),
                message: format!("duplicate stable_id '{}'", node.stable_id),
                suggestion: None,
            });
        }
        all_ids.push(&node.stable_id);
    }

    // Helper: check reference validity
    let available: String = format!("{all_ids:?}");
    let mut check_ref = |path: &str, ref_id: &str| {
        if !id_set.contains(ref_id) {
            errors.push(ValidationIssue {
                path: path.to_string(),
                code: "INVALID_REFERENCE".to_string(),
                message: format!("referenced stable_id '{ref_id}' not found"),
                suggestion: Some(format!("available stable_ids: {available}")),
            });
        }
    };

    // Check children references
    for (i, node) in wire.nodes.iter().enumerate() {
        let children = get_children_wire(&node.kind);
        for (j, child_id) in children.iter().enumerate() {
            check_ref(&format!("nodes[{i}].kind.children[{j}]"), child_id);
        }
    }

    // Check canvas references
    for (i, canvas_id) in wire.canvas.iter().enumerate() {
        check_ref(&format!("canvas[{i}]"), canvas_id);
    }

    // Check view references
    for (i, view) in wire.views.iter().enumerate() {
        match &view.kind {
            ViewKindWire::Print { pages } => {
                for (j, p) in pages.iter().enumerate() {
                    check_ref(&format!("views[{i}].kind.pages[{j}]"), p);
                }
            }
            ViewKindWire::Web { root } => {
                check_ref(&format!("views[{i}].kind.root"), root);
            }
            ViewKindWire::Presentation { slides } => {
                for (j, s) in slides.iter().enumerate() {
                    check_ref(&format!("views[{i}].kind.slides[{j}]"), s);
                }
            }
            ViewKindWire::Export { .. } => {}
        }
    }

    // Check circular hierarchy
    if errors.is_empty() {
        check_circular_hierarchy(&wire, &mut errors);
    }

    // Check Instance source_component references
    check_component_refs(&wire, &id_set, &mut errors);

    // Check override targets
    check_override_targets(&wire, &id_set, &mut errors);

    // Check token cycles by attempting resolution
    check_token_cycles(&wire, &mut errors);

    // Check layout rules
    check_layout_rules(&wire, &mut errors, &mut warnings);

    // Warnings: CMYK colors that will fall back
    check_cmyk_warnings(&wire, &mut warnings);

    ValidateResponse {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

fn get_children_wire(kind: &NodeKindWire) -> Vec<&str> {
    match kind {
        NodeKindWire::Frame(d) => d.container.children.iter().map(|s| s.as_str()).collect(),
        NodeKindWire::Group(d) => d.children.iter().map(|s| s.as_str()).collect(),
        NodeKindWire::BooleanOp(d) => d.children.iter().map(|s| s.as_str()).collect(),
        NodeKindWire::Instance(d) => d.container.children.iter().map(|s| s.as_str()).collect(),
        NodeKindWire::Vector(_) | NodeKindWire::Text(_) | NodeKindWire::Image(_) => vec![],
    }
}

fn check_circular_hierarchy(wire: &DocumentWire, errors: &mut Vec<ValidationIssue>) {
    let adj: HashMap<&str, Vec<&str>> = wire
        .nodes
        .iter()
        .map(|n| (n.stable_id.as_str(), get_children_wire(&n.kind)))
        .collect();

    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    for node in &wire.nodes {
        if !visited.contains(node.stable_id.as_str())
            && has_cycle(&adj, &node.stable_id, &mut visited, &mut in_stack)
        {
            errors.push(ValidationIssue {
                path: format!("nodes (stable_id='{}')", node.stable_id),
                code: "CIRCULAR_HIERARCHY".to_string(),
                message: "circular parent-child relationship detected".to_string(),
                suggestion: None,
            });
        }
    }
}

fn has_cycle<'a>(
    adj: &HashMap<&'a str, Vec<&'a str>>,
    node: &'a str,
    visited: &mut HashSet<&'a str>,
    in_stack: &mut HashSet<&'a str>,
) -> bool {
    visited.insert(node);
    in_stack.insert(node);

    if let Some(children) = adj.get(node) {
        for child in children {
            if (!visited.contains(child) && has_cycle(adj, child, visited, in_stack))
                || in_stack.contains(child)
            {
                return true;
            }
        }
    }

    in_stack.remove(node);
    false
}

fn check_component_refs(
    wire: &DocumentWire,
    id_set: &HashSet<&str>,
    errors: &mut Vec<ValidationIssue>,
) {
    let component_ids: HashSet<&str> = wire
        .nodes
        .iter()
        .filter(|n| matches!(&n.kind, NodeKindWire::Frame(d) if d.component_def.is_some()))
        .map(|n| n.stable_id.as_str())
        .collect();

    for (i, node) in wire.nodes.iter().enumerate() {
        if let NodeKindWire::Instance(ref d) = node.kind {
            if !id_set.contains(d.source_component.as_str()) {
                errors.push(ValidationIssue {
                    path: format!("nodes[{i}].kind.source_component"),
                    code: "INVALID_COMPONENT_REF".to_string(),
                    message: format!("source_component '{}' not found", d.source_component),
                    suggestion: None,
                });
            } else if !component_ids.contains(d.source_component.as_str()) {
                errors.push(ValidationIssue {
                    path: format!("nodes[{i}].kind.source_component"),
                    code: "INVALID_COMPONENT_REF".to_string(),
                    message: format!(
                        "source_component '{}' exists but has no component_def",
                        d.source_component
                    ),
                    suggestion: None,
                });
            }
        }
    }
}

fn check_override_targets(
    wire: &DocumentWire,
    id_set: &HashSet<&str>,
    errors: &mut Vec<ValidationIssue>,
) {
    // Build adjacency map and node kind map
    let adj: HashMap<&str, Vec<&str>> = wire
        .nodes
        .iter()
        .map(|n| (n.stable_id.as_str(), get_children_wire(&n.kind)))
        .collect();

    let node_kinds: HashMap<&str, &NodeKindWire> = wire
        .nodes
        .iter()
        .map(|n| (n.stable_id.as_str(), &n.kind))
        .collect();

    // Cache descendant sets per component to avoid recomputation
    let mut descendant_cache: HashMap<&str, HashSet<&str>> = HashMap::new();

    for (i, node) in wire.nodes.iter().enumerate() {
        if let NodeKindWire::Instance(ref d) = node.kind {
            // Build/retrieve descendant set for this instance's source component
            let descendants = descendant_cache
                .entry(d.source_component.as_str())
                .or_insert_with(|| {
                    let mut desc = HashSet::new();
                    let mut stack = vec![d.source_component.as_str()];
                    while let Some(current) = stack.pop() {
                        if desc.insert(current) {
                            if let Some(children) = adj.get(current) {
                                stack.extend(children.iter());
                            }
                        }
                    }
                    desc
                });

            for (j, ov) in d.overrides.iter().enumerate() {
                let target = ov.target();
                let path_prefix = format!("nodes[{i}].kind.overrides[{j}]");

                // Check target exists
                if !id_set.contains(target) {
                    errors.push(ValidationIssue {
                        path: format!("{path_prefix}.target"),
                        code: "INVALID_OVERRIDE_TARGET".to_string(),
                        message: format!("override target '{target}' not found"),
                        suggestion: None,
                    });
                    continue;
                }

                // Check target is within component subtree
                if !descendants.contains(target) {
                    errors.push(ValidationIssue {
                        path: format!("{path_prefix}.target"),
                        code: "OVERRIDE_TARGET_NOT_IN_COMPONENT".to_string(),
                        message: format!(
                            "override target '{}' is not a descendant of source_component '{}'",
                            target, d.source_component
                        ),
                        suggestion: None,
                    });
                    continue;
                }

                // Size override on component root is invalid — use instance width/height instead
                if let ode_format::node::Override::Size { .. } = ov {
                    if target == d.source_component.as_str() {
                        errors.push(ValidationIssue {
                            path: format!("{path_prefix}.target"),
                            code: "SIZE_OVERRIDE_ON_COMPONENT_ROOT".to_string(),
                            message: format!(
                                "Size override targets component root '{target}'; use instance width/height fields instead"
                            ),
                            suggestion: Some("set width/height on the instance node directly".to_string()),
                        });
                        continue;
                    }
                }

                // Check type compatibility
                if let Some(kind) = node_kinds.get(target) {
                    if let ode_format::node::Override::TextContent { .. } = ov {
                        if !matches!(kind, NodeKindWire::Text(_)) {
                            errors.push(ValidationIssue {
                                path: format!("{path_prefix}.type"),
                                code: "OVERRIDE_TYPE_MISMATCH".to_string(),
                                message: format!(
                                    "TextContent override targets '{target}' which is not a Text node"
                                ),
                                suggestion: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn check_token_cycles(wire: &DocumentWire, errors: &mut Vec<ValidationIssue>) {
    for col in &wire.tokens.collections {
        for tok in &col.tokens {
            if let Err(e) = wire.tokens.resolve(col.id, tok.id) {
                if matches!(e, ode_format::tokens::TokenError::CyclicAlias) {
                    errors.push(ValidationIssue {
                        path: format!("tokens.collections[{}].tokens[{}]", col.id, tok.id),
                        code: "CYCLIC_TOKEN".to_string(),
                        message: format!("token '{}' has a cyclic alias", tok.name),
                        suggestion: None,
                    });
                }
            }
        }
    }
}

fn check_layout_rules(
    wire: &DocumentWire,
    errors: &mut Vec<ValidationIssue>,
    warnings: &mut Vec<Warning>,
) {
    for (i, node) in wire.nodes.iter().enumerate() {
        // Check layout config on frames
        if let NodeKindWire::Frame(ref d) = node.kind {
            if let Some(ref layout) = d.container.layout {
                let path_prefix = format!("nodes[{i}].kind.container.layout");
                // Negative padding
                if layout.padding.top < 0.0
                    || layout.padding.right < 0.0
                    || layout.padding.bottom < 0.0
                    || layout.padding.left < 0.0
                {
                    errors.push(ValidationIssue {
                        path: format!("{path_prefix}.padding"),
                        code: "NEGATIVE_PADDING".to_string(),
                        message: "layout padding values must not be negative".to_string(),
                        suggestion: None,
                    });
                }
                // Negative item_spacing
                if layout.item_spacing < 0.0 {
                    errors.push(ValidationIssue {
                        path: format!("{path_prefix}.item_spacing"),
                        code: "NEGATIVE_SPACING".to_string(),
                        message: "item_spacing must not be negative".to_string(),
                        suggestion: None,
                    });
                }
            }
        }

        // Check layout_sizing constraints
        if let Some(ref sizing) = node.layout_sizing {
            let path_prefix = format!("nodes[{i}].layout_sizing");
            // min > max warnings
            if let (Some(min_w), Some(max_w)) = (sizing.min_width, sizing.max_width) {
                if min_w > max_w {
                    warnings.push(Warning {
                        path: format!("{path_prefix}.min_width/max_width"),
                        code: "MIN_EXCEEDS_MAX".to_string(),
                        message: format!("min_width ({min_w}) exceeds max_width ({max_w})"),
                    });
                }
            }
            if let (Some(min_h), Some(max_h)) = (sizing.min_height, sizing.max_height) {
                if min_h > max_h {
                    warnings.push(Warning {
                        path: format!("{path_prefix}.min_height/max_height"),
                        code: "MIN_EXCEEDS_MAX".to_string(),
                        message: format!("min_height ({min_h}) exceeds max_height ({max_h})"),
                    });
                }
            }
        }
    }
}

fn check_cmyk_warnings(wire: &DocumentWire, warnings: &mut Vec<Warning>) {
    for (i, node) in wire.nodes.iter().enumerate() {
        let visual = match &node.kind {
            NodeKindWire::Frame(d) => Some(&d.visual),
            NodeKindWire::Vector(d) => Some(&d.visual),
            NodeKindWire::BooleanOp(d) => Some(&d.visual),
            NodeKindWire::Text(d) => Some(&d.visual),
            NodeKindWire::Image(d) => Some(&d.visual),
            NodeKindWire::Group(_) | NodeKindWire::Instance(_) => None,
        };
        if let Some(vis) = visual {
            for (j, fill) in vis.fills.iter().enumerate() {
                if let ode_format::style::Paint::Solid {
                    color: ode_format::style::StyleValue::Raw(c),
                } = &fill.paint
                {
                    if matches!(c, ode_format::color::Color::Cmyk { .. }) {
                        warnings.push(Warning {
                            path: format!("nodes[{i}].kind.visual.fills[{j}].paint.color"),
                            code: "CMYK_FALLBACK".to_string(),
                            message: "CMYK color will fall back to black in PNG export".to_string(),
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_json() -> String {
        r#"{
            "format_version": [0, 2, 0],
            "name": "Test",
            "nodes": [
                {"stable_id": "root", "name": "Root", "type": "frame", "width": 100, "height": 100, "visual": {}, "container": {}, "component_def": null}
            ],
            "canvas": ["root"],
            "tokens": {"collections": [], "active_modes": {}},
            "views": []
        }"#.to_string()
    }

    #[test]
    fn valid_document_passes() {
        let result = validate_json(&make_valid_json());
        assert!(
            result.valid,
            "Expected valid, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn duplicate_stable_id_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "dup", "name": "A", "type": "frame", "width": 10, "height": 10, "visual": {}, "container": {}, "component_def": null},
                {"stable_id": "dup", "name": "B", "type": "frame", "width": 10, "height": 10, "visual": {}, "container": {}, "component_def": null}
            ],
            "canvas": ["dup"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "DUPLICATE_ID"));
    }

    #[test]
    fn invalid_reference_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "root", "name": "Root", "type": "frame", "width": 10, "height": 10, "visual": {}, "container": {"children": ["nonexistent"]}, "component_def": null}
            ],
            "canvas": ["root"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "INVALID_REFERENCE"));
    }

    #[test]
    fn invalid_canvas_reference_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [],
            "canvas": ["missing"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "INVALID_REFERENCE"));
    }

    #[test]
    fn circular_hierarchy_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "a", "name": "A", "type": "frame", "width": 10, "height": 10, "visual": {}, "container": {"children": ["b"]}, "component_def": null},
                {"stable_id": "b", "name": "B", "type": "frame", "width": 10, "height": 10, "visual": {}, "container": {"children": ["a"]}, "component_def": null}
            ],
            "canvas": ["a"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "CIRCULAR_HIERARCHY"));
    }

    #[test]
    fn parse_error_returns_parse_failed() {
        let result = validate_json("not json at all");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "PARSE_FAILED"));
    }

    #[test]
    fn valid_layout_document_passes() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Layout Test",
            "nodes": [
                {"stable_id": "root", "name": "Root", "type": "frame", "width": 300, "height": 200,
                 "visual": {}, "container": {"layout": {"direction": "horizontal", "item-spacing": 8, "padding": {"top": 10, "right": 10, "bottom": 10, "left": 10}}}, "component_def": null}
            ],
            "canvas": ["root"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(
            result.valid,
            "Expected valid, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn negative_padding_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "root", "name": "Root", "type": "frame", "width": 100, "height": 100,
                 "visual": {}, "container": {"layout": {"padding": {"top": -5, "right": 0, "bottom": 0, "left": 0}}}, "component_def": null}
            ],
            "canvas": ["root"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "NEGATIVE_PADDING"));
    }

    #[test]
    fn negative_spacing_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "root", "name": "Root", "type": "frame", "width": 100, "height": 100,
                 "visual": {}, "container": {"layout": {"item-spacing": -3}}, "component_def": null}
            ],
            "canvas": ["root"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "NEGATIVE_SPACING"));
    }

    #[test]
    fn min_exceeds_max_warning() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "root", "name": "Root", "type": "frame", "width": 100, "height": 100,
                 "visual": {}, "container": {}, "component_def": null,
                 "layout_sizing": {"width": "fixed", "height": "fixed", "min-width": 200, "max-width": 100}}
            ],
            "canvas": ["root"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(result.valid, "min > max should be a warning, not an error");
        assert!(
            result.warnings.iter().any(|w| w.code == "MIN_EXCEEDS_MAX"),
            "Expected MIN_EXCEEDS_MAX warning, got: {:?}",
            result.warnings
        );
    }

    // ─── Override Validation Tests ───

    #[test]
    fn valid_instance_with_overrides_passes() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "comp", "name": "Comp", "type": "frame", "width": 100, "height": 50,
                 "visual": {}, "container": {}, "component_def": {"name": "Button", "description": ""}},
                {"stable_id": "inst", "name": "Inst", "type": "instance",
                 "container": {}, "source_component": "comp",
                 "overrides": [{"type": "visible", "target": "comp", "visible": false}]}
            ],
            "canvas": ["comp"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(
            result.valid,
            "Valid instance with overrides should pass, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn invalid_override_target_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "comp", "name": "Comp", "type": "frame", "width": 100, "height": 50,
                 "visual": {}, "container": {}, "component_def": {"name": "Button", "description": ""}},
                {"stable_id": "inst", "name": "Inst", "type": "instance",
                 "container": {}, "source_component": "comp",
                 "overrides": [{"type": "visible", "target": "nonexistent", "visible": false}]}
            ],
            "canvas": ["comp"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.code == "INVALID_OVERRIDE_TARGET"),
            "Expected INVALID_OVERRIDE_TARGET, got: {:?}",
            result.errors
        );
    }

    #[test]
    fn override_type_mismatch_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "comp", "name": "Comp", "type": "frame", "width": 100, "height": 50,
                 "visual": {}, "container": {"children": ["child"]},
                 "component_def": {"name": "Card", "description": ""}},
                {"stable_id": "child", "name": "Child", "type": "frame", "width": 50, "height": 50,
                 "visual": {}, "container": {}, "component_def": null},
                {"stable_id": "inst", "name": "Inst", "type": "instance",
                 "container": {}, "source_component": "comp",
                 "overrides": [{"type": "text-content", "target": "child", "content": "hello"}]}
            ],
            "canvas": ["comp"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.code == "OVERRIDE_TYPE_MISMATCH"),
            "Expected OVERRIDE_TYPE_MISMATCH, got: {:?}",
            result.errors
        );
    }

    #[test]
    fn override_target_not_in_component_subtree() {
        // CompA has child_a, CompB has child_b
        // Instance of CompA overrides child_b — which is NOT in CompA's subtree
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "child_a", "name": "ChildA", "type": "frame", "width": 30, "height": 30,
                 "visual": {}, "container": {}, "component_def": null},
                {"stable_id": "comp_a", "name": "CompA", "type": "frame", "width": 100, "height": 50,
                 "visual": {}, "container": {"children": ["child_a"]},
                 "component_def": {"name": "CompA", "description": ""}},
                {"stable_id": "child_b", "name": "ChildB", "type": "frame", "width": 30, "height": 30,
                 "visual": {}, "container": {}, "component_def": null},
                {"stable_id": "comp_b", "name": "CompB", "type": "frame", "width": 100, "height": 50,
                 "visual": {}, "container": {"children": ["child_b"]},
                 "component_def": {"name": "CompB", "description": ""}},
                {"stable_id": "inst", "name": "InstA", "type": "instance",
                 "container": {}, "source_component": "comp_a",
                 "overrides": [{"type": "visible", "target": "child_b", "visible": false}]}
            ],
            "canvas": ["comp_a", "comp_b"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(
            !result.valid,
            "Override targeting node outside component subtree should fail, got: {:?}",
            result.errors
        );
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.code == "OVERRIDE_TARGET_NOT_IN_COMPONENT"),
            "Expected OVERRIDE_TARGET_NOT_IN_COMPONENT, got: {:?}",
            result.errors
        );
    }

    #[test]
    fn size_override_on_component_root_rejected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "comp", "name": "Comp", "type": "frame", "width": 100, "height": 50,
                 "visual": {}, "container": {},
                 "component_def": {"name": "Box", "description": ""}},
                {"stable_id": "inst", "name": "Inst", "type": "instance",
                 "container": {}, "source_component": "comp",
                 "overrides": [{"type": "size", "target": "comp", "width": 200, "height": 100}]}
            ],
            "canvas": ["comp"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.code == "SIZE_OVERRIDE_ON_COMPONENT_ROOT"),
            "Expected SIZE_OVERRIDE_ON_COMPONENT_ROOT, got: {:?}",
            result.errors
        );
    }
}
