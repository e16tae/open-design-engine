use clap::{Parser, Subcommand};

mod commands;
mod knowledge;
mod mutate;
mod output;
mod validate;

#[derive(Parser)]
#[command(
    name = "ode",
    about = "Open Design Engine CLI — Agent-native design tool"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new empty .ode.json document
    New {
        /// Output file path
        file: String,
        /// Document name
        #[arg(long)]
        name: Option<String>,
        /// Root frame width (requires --height)
        #[arg(long, requires = "height")]
        width: Option<f32>,
        /// Root frame height (requires --width)
        #[arg(long, requires = "width")]
        height: Option<f32>,
    },
    /// Validate an .ode.json document
    Validate {
        /// Input file (or "-" for stdin)
        file: String,
    },
    /// Validate, render, and export in one step
    Build {
        /// Input file (or "-" for stdin)
        file: String,
        /// Output file path (PNG, SVG, or PDF)
        #[arg(short, long)]
        output: String,
        /// Output format: png, svg, pdf (auto-detected from extension if omitted)
        #[arg(long)]
        format: Option<String>,
        /// Resize the root frame (e.g., 1920x1080)
        #[arg(long, value_name = "WxH")]
        resize: Option<String>,
    },
    /// Render without validation (fast path)
    Render {
        /// Input file (or "-" for stdin)
        file: String,
        /// Output file path (PNG, SVG, or PDF)
        #[arg(short, long)]
        output: String,
        /// Output format: png, svg, pdf (auto-detected from extension if omitted)
        #[arg(long)]
        format: Option<String>,
        /// Resize the root frame (e.g., 1920x1080)
        #[arg(long, value_name = "WxH")]
        resize: Option<String>,
    },
    /// Inspect document structure
    Inspect {
        /// Input file (or "-" for stdin)
        file: String,
        /// Show full properties (not just tree summary)
        #[arg(long)]
        full: bool,
    },
    /// Output JSON Schema for the .ode.json format
    Schema {
        /// Schema topic: document, node, paint, token, color
        topic: Option<String>,
    },
    /// Import a design file from an external format
    Import {
        #[command(subcommand)]
        source: ImportSource,
    },
    /// Manage design tokens
    Tokens {
        #[command(subcommand)]
        action: TokenAction,
    },
    /// Query design knowledge guides
    Guide {
        /// Guide layer ID (e.g., "accessibility", "spatial-composition")
        layer_id: Option<String>,
        /// Filter by context (e.g., "web", "print")
        #[arg(long)]
        context: Option<String>,
        /// Show only a specific section
        #[arg(long)]
        section: Option<String>,
        /// List guides related to a layer
        #[arg(long)]
        related: Option<String>,
    },
    /// Review a design against knowledge-based rules
    Review {
        /// Input file (.ode.json) or - for stdin
        file: String,
        /// Override context detection
        #[arg(long)]
        context: Option<String>,
        /// Only check rules from a specific layer
        #[arg(long)]
        layer: Option<String>,
    },
    /// Set properties on an existing node
    Set {
        /// Document file path
        file: String,
        /// Node stable_id
        stable_id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        visible: Option<bool>,
        #[arg(long)]
        opacity: Option<f32>,
        #[arg(long)]
        blend_mode: Option<String>,
        #[arg(long)]
        x: Option<f32>,
        #[arg(long)]
        y: Option<f32>,
        #[arg(long)]
        width: Option<f32>,
        #[arg(long)]
        height: Option<f32>,
        #[arg(long)]
        fill: Option<String>,
        #[arg(long)]
        fill_opacity: Option<f32>,
        #[arg(long)]
        stroke: Option<String>,
        #[arg(long)]
        stroke_width: Option<f32>,
        #[arg(long)]
        stroke_position: Option<String>,
        #[arg(long, value_name = "R or TL,TR,BR,BL")]
        corner_radius: Option<String>,
        #[arg(long)]
        clips_content: Option<bool>,
        #[arg(long)]
        layout: Option<String>,
        #[arg(long, value_name = "P or T,R,B,L")]
        padding: Option<String>,
        #[arg(long)]
        gap: Option<f32>,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        font_size: Option<f32>,
        #[arg(long)]
        font_family: Option<String>,
        #[arg(long)]
        font_weight: Option<u16>,
        #[arg(long)]
        text_align: Option<String>,
        #[arg(long)]
        line_height: Option<String>,
    },
    /// Add a node to a document
    Add {
        /// Node kind: frame, group, text, vector, image
        kind: String,
        /// Document file path
        file: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        index: Option<usize>,
        #[arg(long)]
        width: Option<f32>,
        #[arg(long)]
        height: Option<f32>,
        #[arg(long)]
        fill: Option<String>,
        #[arg(long, value_name = "R or TL,TR,BR,BL")]
        corner_radius: Option<String>,
        #[arg(long)]
        clips_content: Option<bool>,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        font_size: Option<f32>,
        #[arg(long)]
        font_family: Option<String>,
        #[arg(long)]
        shape: Option<String>,
        #[arg(long)]
        sides: Option<u32>,
        #[arg(long)]
        src: Option<String>,
    },
}

#[derive(Subcommand)]
enum ImportSource {
    /// Import from Figma REST API
    Figma {
        /// Figma Personal Access Token (or set FIGMA_TOKEN env var)
        #[arg(short, long, env = "FIGMA_TOKEN")]
        token: Option<String>,
        /// Figma file key
        #[arg(short = 'k', long)]
        file_key: Option<String>,
        /// Local Figma JSON file (alternative to API)
        #[arg(short, long)]
        input: Option<String>,
        /// Output .ode.json file path
        #[arg(short, long)]
        output: String,
        /// Include Figma Variables as DesignTokens
        #[arg(long)]
        with_variables: bool,
        /// Skip downloading images
        #[arg(long)]
        skip_images: bool,
    },
}

#[derive(Subcommand)]
enum TokenAction {
    /// List all token collections and tokens
    List {
        /// Input .ode.json file
        file: String,
    },
    /// Resolve a token value in the current active mode
    Resolve {
        /// Input .ode.json file
        file: String,
        /// Collection name or ID
        #[arg(long)]
        collection: String,
        /// Token name or ID
        #[arg(long)]
        token: String,
    },
    /// Set active mode for a collection
    SetMode {
        /// Input .ode.json file
        file: String,
        /// Collection name or ID
        #[arg(long)]
        collection: String,
        /// Mode name or ID
        #[arg(long)]
        mode: String,
        /// Output file (defaults to overwriting input)
        #[arg(short, long)]
        output: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Command::New {
            file,
            name,
            width,
            height,
        } => commands::cmd_new(&file, name.as_deref(), width, height),
        Command::Validate { file } => commands::cmd_validate(&file),
        Command::Build {
            file,
            output,
            format,
            resize,
        } => commands::cmd_build(&file, &output, format.as_deref(), resize.as_deref()),
        Command::Render {
            file,
            output,
            format,
            resize,
        } => commands::cmd_render(&file, &output, format.as_deref(), resize.as_deref()),
        Command::Inspect { file, full } => commands::cmd_inspect(&file, full),
        Command::Schema { topic } => commands::cmd_schema(topic.as_deref()),
        Command::Import { source } => match source {
            ImportSource::Figma {
                token,
                file_key,
                input,
                output,
                with_variables,
                skip_images,
            } => commands::cmd_import_figma(
                token,
                file_key,
                input,
                &output,
                with_variables,
                skip_images,
            ),
        },
        Command::Tokens { action } => match action {
            TokenAction::List { file } => commands::cmd_tokens_list(&file),
            TokenAction::Resolve {
                file,
                collection,
                token,
            } => commands::cmd_tokens_resolve(&file, &collection, &token),
            TokenAction::SetMode {
                file,
                collection,
                mode,
                output,
            } => commands::cmd_tokens_set_mode(&file, &collection, &mode, output.as_deref()),
        },
        Command::Guide {
            layer_id,
            context,
            section,
            related,
        } => commands::cmd_guide(
            layer_id.as_deref(),
            context.as_deref(),
            section.as_deref(),
            related.as_deref(),
        ),
        Command::Review {
            file,
            context,
            layer,
        } => commands::cmd_review(&file, context.as_deref(), layer.as_deref()),
        Command::Set {
            file,
            stable_id,
            name,
            visible,
            opacity,
            blend_mode,
            x,
            y,
            width,
            height,
            fill,
            fill_opacity,
            stroke,
            stroke_width,
            stroke_position,
            corner_radius,
            clips_content,
            layout,
            padding,
            gap,
            content,
            font_size,
            font_family,
            font_weight,
            text_align,
            line_height,
        } => mutate::cmd_set(
            &file,
            &stable_id,
            name.as_deref(),
            visible,
            opacity,
            blend_mode.as_deref(),
            x,
            y,
            width,
            height,
            fill.as_deref(),
            fill_opacity,
            stroke.as_deref(),
            stroke_width,
            stroke_position.as_deref(),
            corner_radius.as_deref(),
            clips_content,
            layout.as_deref(),
            padding.as_deref(),
            gap,
            content.as_deref(),
            font_size,
            font_family.as_deref(),
            font_weight,
            text_align.as_deref(),
            line_height.as_deref(),
        ),
        Command::Add {
            kind,
            file,
            name,
            parent,
            index,
            width,
            height,
            fill,
            corner_radius,
            clips_content,
            content,
            font_size,
            font_family,
            shape,
            sides,
            src,
        } => mutate::cmd_add(
            &kind,
            &file,
            name.as_deref(),
            parent.as_deref(),
            index,
            width,
            height,
            fill.as_deref(),
            corner_radius.as_deref(),
            clips_content,
            content.as_deref(),
            font_size,
            font_family.as_deref(),
            shape.as_deref(),
            sides,
            src.as_deref(),
        ),
    };

    std::process::exit(exit_code);
}
