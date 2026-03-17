use std::path::PathBuf;

/// Locate the `design-knowledge/` directory at runtime.
///
/// Search order:
/// 1. `ODE_KNOWLEDGE_PATH` environment variable
/// 2. Build-time: relative to the crate manifest directory
/// 3. Relative to the current executable
/// 4. Current working directory
/// 5. `$HOME/.ode/design-knowledge/`
///
/// Returns the first path that is an existing directory, or `None`.
pub fn find_knowledge_dir() -> Option<PathBuf> {
    // 1. Explicit env var
    if let Ok(val) = std::env::var("ODE_KNOWLEDGE_PATH") {
        let p = PathBuf::from(val);
        if p.is_dir() {
            return Some(p);
        }
    }

    // 2. Build-time path: only useful during `cargo run` development.
    // In installed binaries, this path no longer exists and falls through to step 3.
    {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Some(root) = manifest.parent().and_then(|p| p.parent()) {
            let p = root.join("design-knowledge");
            if p.is_dir() {
                return Some(p);
            }
        }
    }

    // 3. Relative to executable: exe/../design-knowledge
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent().and_then(|p| p.parent()) {
            let p = parent.join("design-knowledge");
            if p.is_dir() {
                return Some(p);
            }
        }
    }

    // 4. Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("design-knowledge");
        if p.is_dir() {
            return Some(p);
        }
    }

    // 5. Home directory: ~/.ode/design-knowledge/
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join(".ode").join("design-knowledge");
        if p.is_dir() {
            return Some(p);
        }
    }

    None
}
