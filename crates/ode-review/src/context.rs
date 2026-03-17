use ode_format::Document;
use ode_format::document::ViewKind;

/// Detect review contexts from a document's views.
///
/// Maps each `ViewKind` to a context string:
/// - `Print` → `"print"`
/// - `Web` → `"web"`
/// - `Presentation` → `"presentation"`
/// - `Export` → skipped (not a design context)
///
/// If no contexts are detected, defaults to `["web"]`.
/// The result is deduplicated.
pub fn detect_context(doc: &Document) -> Vec<String> {
    let mut contexts: Vec<String> = doc
        .views
        .iter()
        .filter_map(|view| match &view.kind {
            ViewKind::Print { .. } => Some("print".to_string()),
            ViewKind::Web { .. } => Some("web".to_string()),
            ViewKind::Presentation { .. } => Some("presentation".to_string()),
            ViewKind::Export { .. } => None,
        })
        .collect();

    // Deduplicate while preserving order.
    let mut seen = std::collections::HashSet::new();
    contexts.retain(|c| seen.insert(c.clone()));

    if contexts.is_empty() {
        contexts.push("web".to_string());
    }

    contexts
}

#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::document::{View, ViewId, ViewKind};

    #[test]
    fn no_views_defaults_to_web() {
        let doc = Document::new("Empty");
        let ctx = detect_context(&doc);
        assert_eq!(ctx, vec!["web"]);
    }

    #[test]
    fn export_view_falls_back_to_web() {
        let mut doc = Document::new("Export Only");
        doc.views.push(View {
            id: ViewId(1),
            name: "Export".to_string(),
            kind: ViewKind::Export { targets: vec![] },
        });
        let ctx = detect_context(&doc);
        assert_eq!(ctx, vec!["web"]);
    }

    #[test]
    fn print_view_detected() {
        let mut doc = Document::new("Print Doc");
        doc.views.push(View {
            id: ViewId(1),
            name: "Pages".to_string(),
            kind: ViewKind::Print { pages: vec![] },
        });
        let ctx = detect_context(&doc);
        assert_eq!(ctx, vec!["print"]);
    }

    #[test]
    fn duplicate_contexts_are_deduplicated() {
        let mut doc = Document::new("Multi Web");
        let dummy_id = ode_format::NodeId::default();
        doc.views.push(View {
            id: ViewId(1),
            name: "Web 1".to_string(),
            kind: ViewKind::Web { root: dummy_id },
        });
        doc.views.push(View {
            id: ViewId(2),
            name: "Web 2".to_string(),
            kind: ViewKind::Web { root: dummy_id },
        });
        let ctx = detect_context(&doc);
        assert_eq!(ctx, vec!["web"]);
    }

    #[test]
    fn mixed_views_produce_multiple_contexts() {
        let mut doc = Document::new("Mixed");
        let dummy_id = ode_format::NodeId::default();
        doc.views.push(View {
            id: ViewId(1),
            name: "Web".to_string(),
            kind: ViewKind::Web { root: dummy_id },
        });
        doc.views.push(View {
            id: ViewId(2),
            name: "Print".to_string(),
            kind: ViewKind::Print { pages: vec![] },
        });
        doc.views.push(View {
            id: ViewId(3),
            name: "Export".to_string(),
            kind: ViewKind::Export { targets: vec![] },
        });
        let ctx = detect_context(&doc);
        assert_eq!(ctx, vec!["web", "print"]);
    }
}
