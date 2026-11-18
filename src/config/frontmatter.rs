//! Frontmatter extraction.

/// Splits a Markdown document into `(frontmatter, body)`.
///
/// Frontmatter is recognised as the region between the leading `---` line
/// and the next standalone `---` line. If the file doesn't start with `---`,
/// or the closing delimiter is missing, everything is treated as body.
pub(crate) fn split_frontmatter(md: &str) -> (Option<&str>, &str) {
    let md = md.strip_prefix('\u{feff}').unwrap_or(md);
    // First line must be exactly `---`.
    let first_line_end = md.find('\n').unwrap_or(md.len());
    let first_line = md[..first_line_end].trim_end_matches('\r');
    if first_line != "---" {
        return (None, md);
    }
    let after_first = &md[(first_line_end + 1).min(md.len())..];
    // Look for a closing `---` on its own line.
    let bytes = after_first.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let line_end = after_first[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        let line = after_first[pos..line_end].trim_end_matches('\r');
        if line == "---" {
            let fm = after_first[..pos].trim_end_matches(|c| c == '\r' || c == '\n');
            let body_start = (line_end + 1).min(bytes.len());
            return (Some(fm), &after_first[body_start..]);
        }
        pos = line_end + 1;
    }
    (None, md)
}
