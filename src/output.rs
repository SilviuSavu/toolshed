/// Truncate output to fit within a character budget.
///
/// - Counts characters, not bytes
/// - Prefers truncating at last newline within budget (if within last 20%)
/// - Never splits UTF-8 characters
/// - Detects binary output
pub fn truncate(input: &str, max_chars: usize) -> String {
    if is_binary(input) {
        return format!("[binary output detected: {} bytes]", input.len());
    }

    let char_count = input.chars().count();
    if char_count <= max_chars {
        return input.to_string();
    }

    // Try to find a newline in the last 20% of the budget for cleaner cut
    let search_start = max_chars - (max_chars / 5);
    let search_region: String = input
        .chars()
        .skip(search_start)
        .take(max_chars - search_start)
        .collect();

    let cut_point = if let Some(nl_pos) = search_region.rfind('\n') {
        // Found a newline — cut there
        let char_offset = search_start + search_region[..nl_pos].chars().count();
        char_offset + 1 // include the newline
    } else {
        max_chars
    };

    let result: String = input.chars().take(cut_point).collect();

    format!(
        "{result}\n[output truncated: {char_count} chars total, showing first {cut_point}. Rerun with --full for complete output]"
    )
}

fn is_binary(input: &str) -> bool {
    let sample: Vec<char> = input.chars().take(8192).collect();
    if sample.is_empty() {
        return false;
    }
    let non_printable = sample
        .iter()
        .filter(|c| {
            !c.is_ascii_graphic()
                && !c.is_ascii_whitespace()
                && **c != '\n'
                && **c != '\r'
                && **c != '\t'
        })
        .count();
    let ratio = non_printable as f64 / sample.len() as f64;
    ratio > 0.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_needed() {
        let input = "hello world";
        assert_eq!(truncate(input, 100), "hello world");
    }

    #[test]
    fn truncates_long_output() {
        let input = "a".repeat(1000);
        let result = truncate(&input, 100);
        assert!(result.contains("[output truncated:"));
        assert!(result.contains("1000 chars total"));
    }

    #[test]
    fn prefers_newline_boundary() {
        // 100 char budget. Put a newline at position 85 (within last 20%).
        let mut input = String::new();
        for _ in 0..85 {
            input.push('a');
        }
        input.push('\n');
        for _ in 0..200 {
            input.push('b');
        }
        let result = truncate(&input, 100);
        // Should cut at the newline (position 86)
        assert!(result.starts_with(&"a".repeat(85)));
    }

    #[test]
    fn handles_unicode() {
        let input = "a]c".repeat(200); // each char is 1 unicode char
        let result = truncate(&input, 100);
        assert!(result.contains("[output truncated:"));
    }

    #[test]
    fn empty_output() {
        assert_eq!(truncate("", 100), "");
    }

    #[test]
    fn binary_detection() {
        let mut input = String::new();
        for i in 0..100u8 {
            input.push(i as char);
        }
        let result = truncate(&input, 50);
        assert!(result.contains("[binary output detected:"));
    }
}
