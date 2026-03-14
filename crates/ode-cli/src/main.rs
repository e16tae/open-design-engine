use clap::{Parser, Subcommand};

mod commands;
mod output;
mod validate;

#[derive(Parser)]
#[command(name = "ode", about = "Open Design Engine CLI — Agent-native design tool")]
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
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Command::New { file, name, width, height } => {
            commands::cmd_new(&file, name.as_deref(), width, height)
        }
        Command::Validate { file } => {
            commands::cmd_validate(&file)
        }
        Command::Build { file, output, format } => {
            commands::cmd_build(&file, &output, format.as_deref())
        }
        Command::Render { file, output, format } => {
            commands::cmd_render(&file, &output, format.as_deref())
        }
        Command::Inspect { file, full } => {
            commands::cmd_inspect(&file, full)
        }
        Command::Schema { topic } => {
            commands::cmd_schema(topic.as_deref())
        }
    };

    std::process::exit(exit_code);
}
