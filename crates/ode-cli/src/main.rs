use std::path::PathBuf;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ode_core::{Renderer, Scene};
use ode_export::PngExporter;
use ode_format::Document;

#[derive(Parser)]
#[command(name = "ode", about = "Open Design Engine CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render an .ode.json file to PNG
    Render {
        /// Input .ode.json file
        input: PathBuf,
        /// Output PNG path (default: <input_stem>.png)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Show document metadata
    Info {
        /// Input .ode.json file
        input: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Render { input, output } => cmd_render(&input, output.as_deref()),
        Command::Info { input } => cmd_info(&input),
    }
}

fn load_document(path: &std::path::Path) -> Result<Document> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let doc: Document = serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(doc)
}

fn cmd_render(input: &std::path::Path, output: Option<&std::path::Path>) -> Result<()> {
    let doc = load_document(input)?;

    let scene = Scene::from_document(&doc)
        .context("Failed to convert document to scene")?;

    let pixmap = Renderer::render(&scene)
        .context("Failed to render scene")?;

    let out_path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            // Strip .ode.json double extension → stem.png
            let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            let stem = stem.strip_suffix(".ode").unwrap_or(stem);
            input.with_file_name(format!("{stem}.png"))
        }
    };

    PngExporter::export(&pixmap, &out_path)
        .with_context(|| format!("Failed to export PNG to {}", out_path.display()))?;

    println!("Rendered {} → {}", input.display(), out_path.display());
    Ok(())
}

fn cmd_info(input: &std::path::Path) -> Result<()> {
    let doc = load_document(input)?;

    println!("Name:            {}", doc.name);
    println!("Format version:  {}", doc.format_version);
    println!("Nodes:           {}", doc.nodes.len());
    println!("Canvas roots:    {}", doc.canvas.len());
    println!("Views:           {}", doc.views.len());
    println!("Color space:     {:?}", doc.working_color_space);

    Ok(())
}
