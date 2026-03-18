use ode_core::{Renderer, Scene};
use ode_export::{PdfExporter, PngExporter, SvgExporter};
use ode_format::color::Color;
use ode_format::document::Document;
use ode_format::node::{Node, NodeKind, PathSegment, VectorPath};
use ode_format::style::*;

/// End-to-end: Build document → convert to scene → render → export PNG → verify pixels
#[test]
fn document_to_png_red_frame() {
    // 1. Build document with a red-filled frame
    let mut doc = Document::new("E2E Test");
    let mut frame = Node::new_frame("Red Box", 64.0, 64.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    // 2. Convert to scene
    let scene = Scene::from_document(&doc, &ode_core::FontDatabase::new()).unwrap();
    assert!((scene.width - 64.0).abs() < f32::EPSILON);
    assert!((scene.height - 64.0).abs() < f32::EPSILON);

    // 3. Render to pixels
    let pixmap = Renderer::render(&scene).unwrap();
    assert_eq!(pixmap.width(), 64);
    assert_eq!(pixmap.height(), 64);

    // 4. Verify center pixel is red
    let center = pixmap.pixel(32, 32).unwrap();
    assert_eq!(center.red(), 255, "Center should be red");
    assert_eq!(center.green(), 0);
    assert_eq!(center.blue(), 0);
    assert_eq!(center.alpha(), 255);

    // 5. Export to PNG bytes
    let png_bytes = PngExporter::export_bytes(&pixmap).unwrap();
    assert!(!png_bytes.is_empty());
    assert_eq!(&png_bytes[..4], &[0x89, b'P', b'N', b'G']);

    // 6. Write to temp file and verify
    let path = std::env::temp_dir().join("ode_e2e_red_frame.png");
    PngExporter::export(&pixmap, &path).unwrap();
    assert!(path.exists());
    let file_bytes = std::fs::read(&path).unwrap();
    assert_eq!(png_bytes, file_bytes);
    std::fs::remove_file(&path).ok();
}

#[test]
fn document_with_vector_path() {
    let mut doc = Document::new("Vector Test");
    // Triangle path
    let path = VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 32.0, y: 0.0 },
            PathSegment::LineTo { x: 64.0, y: 64.0 },
            PathSegment::LineTo { x: 0.0, y: 64.0 },
            PathSegment::Close,
        ],
        closed: true,
    };
    let mut frame = Node::new_frame("Container", 64.0, 64.0);
    let mut vector = Node::new_vector("Triangle", path);
    if let NodeKind::Vector(ref mut data) = vector.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let vec_id = doc.nodes.insert(vector);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.container.children.push(vec_id);
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    let scene = Scene::from_document(&doc, &ode_core::FontDatabase::new()).unwrap();
    let pixmap = Renderer::render(&scene).unwrap();

    // Bottom-center of the triangle should be blue
    let bottom_center = pixmap.pixel(32, 60).unwrap();
    assert!(
        bottom_center.blue() > 200,
        "Bottom center of triangle should be blue, got b={}",
        bottom_center.blue()
    );

    // Top-left corner should be transparent (outside triangle)
    let corner = pixmap.pixel(2, 2).unwrap();
    assert!(
        corner.alpha() < 10,
        "Top-left corner should be transparent, got alpha={}",
        corner.alpha()
    );
}

#[test]
fn document_with_gradient_fill() {
    let mut doc = Document::new("Gradient Test");
    let mut frame = Node::new_frame("Gradient Box", 100.0, 10.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::LinearGradient {
                stops: vec![
                    GradientStop {
                        position: 0.0,
                        color: StyleValue::Raw(Color::black()),
                    },
                    GradientStop {
                        position: 1.0,
                        color: StyleValue::Raw(Color::white()),
                    },
                ],
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 100.0, y: 0.0 },
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    let scene = Scene::from_document(&doc, &ode_core::FontDatabase::new()).unwrap();
    let pixmap = Renderer::render(&scene).unwrap();

    // Left should be dark, right should be light
    let left = pixmap.pixel(5, 5).unwrap();
    let right = pixmap.pixel(95, 5).unwrap();
    assert!(left.red() < 50, "Left should be dark, got r={}", left.red());
    assert!(
        right.red() > 200,
        "Right should be light, got r={}",
        right.red()
    );
}

/// End-to-end: Build document → convert to scene → export PDF → verify magic bytes & file
#[test]
fn document_to_pdf_red_frame() {
    // 1. Build document with a red-filled frame
    let mut doc = Document::new("PDF E2E Test");
    let mut frame = Node::new_frame("Red Box", 64.0, 64.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    // 2. Convert to scene
    let scene = Scene::from_document(&doc, &ode_core::FontDatabase::new()).unwrap();
    assert!((scene.width - 64.0).abs() < f32::EPSILON);

    // 3. Export to PDF bytes
    let pdf_bytes = PdfExporter::export_bytes(&scene).unwrap();
    assert!(!pdf_bytes.is_empty());
    assert_eq!(&pdf_bytes[..5], b"%PDF-");

    // 4. Write to temp file and verify
    let path = std::env::temp_dir().join("ode_e2e_red_frame.pdf");
    PdfExporter::export(&scene, &path).unwrap();
    assert!(path.exists());
    let file_bytes = std::fs::read(&path).unwrap();
    assert_eq!(pdf_bytes, file_bytes);
    std::fs::remove_file(&path).ok();
}

/// End-to-end: Build document with mask → convert to scene → render PNG/SVG/PDF
#[test]
fn mask_e2e_renders_without_panic() {
    // Build a document with a mask node + masked sibling
    let mut doc = Document::new("MaskE2E");

    let mut mask = Node::new_vector(
        "Mask",
        VectorPath {
            segments: vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: 80.0, y: 0.0 },
                PathSegment::LineTo { x: 80.0, y: 80.0 },
                PathSegment::LineTo { x: 0.0, y: 80.0 },
                PathSegment::Close,
            ],
            closed: true,
        },
    );
    mask.is_mask = true;
    let mask_id = doc.nodes.insert(mask);

    let mut rect = Node::new_vector(
        "Rect",
        VectorPath {
            segments: vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: 200.0, y: 0.0 },
                PathSegment::LineTo { x: 200.0, y: 200.0 },
                PathSegment::LineTo { x: 0.0, y: 200.0 },
                PathSegment::Close,
            ],
            closed: true,
        },
    );
    if let NodeKind::Vector(ref mut data) = rect.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let rect_id = doc.nodes.insert(rect);

    let mut frame = Node::new_frame("Root", 200.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.container.children = vec![mask_id, rect_id];
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    let font_db = ode_core::FontDatabase::new();
    let scene = Scene::from_document(&doc, &font_db).unwrap();

    // PNG render
    let pixmap = Renderer::render(&scene).unwrap();
    let png_bytes = PngExporter::export_bytes(&pixmap).unwrap();
    assert!(png_bytes.len() > 100, "PNG should have content");

    // SVG render — mask produces a clipPath element
    let svg = SvgExporter::export_string(&scene).unwrap();
    assert!(
        svg.contains("clipPath"),
        "SVG should contain a clipPath for the mask"
    );

    // PDF render
    let pdf_bytes = PdfExporter::export_bytes(&scene).unwrap();
    assert!(
        pdf_bytes.starts_with(b"%PDF"),
        "Should produce valid PDF"
    );
}

#[test]
fn grid_layout_e2e_renders() {
    use ode_format::node::*;

    let mut doc = Document::new("GridE2E");

    let mut frame = Node::new_frame("Grid", 320.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.width_sizing = SizingMode::Fixed;
        data.height_sizing = SizingMode::Fixed;
        data.container.layout = Some(LayoutConfig {
            mode: LayoutMode::Grid,
            direction: LayoutDirection::Horizontal,
            primary_axis_align: PrimaryAxisAlign::Start,
            counter_axis_align: CounterAxisAlign::Start,
            padding: LayoutPadding::default(),
            item_spacing: 10.0,
            counter_axis_spacing: 10.0,
            wrap: LayoutWrap::Wrap,
        });

        let child_ids: Vec<NodeId> = (0..4)
            .map(|i| {
                let mut child = Node::new_frame(&format!("Cell{i}"), 100.0, 50.0);
                if let NodeKind::Frame(ref mut cd) = child.kind {
                    cd.width_sizing = SizingMode::Fixed;
                    cd.height_sizing = SizingMode::Fixed;
                    cd.visual.fills.push(Fill {
                        paint: Paint::Solid {
                            color: StyleValue::Raw(Color::Srgb {
                                r: 1.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                        },
                        opacity: StyleValue::Raw(1.0),
                        blend_mode: BlendMode::Normal,
                        visible: true,
                    });
                }
                doc.nodes.insert(child)
            })
            .collect();
        data.container.children = child_ids;
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    let font_db = ode_core::FontDatabase::new();
    let scene = Scene::from_document(&doc, &font_db).unwrap();

    // PNG should render
    let pixmap = Renderer::render(&scene).unwrap();
    let png_bytes = PngExporter::export_bytes(&pixmap).unwrap();
    assert!(png_bytes.len() > 100, "PNG should have content");

    // SVG should render
    let svg = SvgExporter::export_string(&scene).unwrap();
    assert!(!svg.is_empty(), "SVG should have content");
}
