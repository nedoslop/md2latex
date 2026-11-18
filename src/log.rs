//! Styled console output.
//!
//! All messages pass through this module to ensure a consistent visual style:
//! info in cyan, success in green, warnings in yellow, errors in red.

use console::style;

/// Build / section header.
pub fn header(title: &str) {
    println!();
    println!("{}", style(format!("── {} ──", title)).bold().cyan());
}

/// Regular info message.
pub fn info(msg: &str) {
    println!("{} {}", style("•").cyan().bold(), msg);
}

/// Successful step completion.
pub fn success(msg: &str) {
    println!("{} {}", style("✓").green().bold(), msg);
}

/// Warning (stderr).
pub fn warn(msg: &str) {
    eprintln!("{} {}", style("⚠").yellow().bold(), style(msg).yellow());
}

/// Error (stderr).
pub fn error(msg: &str) {
    eprintln!("{} {}", style("✗").red().bold(), style(msg).red());
}

/// Aligned "Label: value" output.
pub fn field(label: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    // Pad before styling — otherwise ANSI codes count towards width.
    let padded = format!("{:<16}", format!("{}:", label));
    println!("  {} {}", style(padded).dim(), style(value).bold());
}
