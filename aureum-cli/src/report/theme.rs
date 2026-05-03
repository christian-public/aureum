use aureum::string::{self, TextBlockOptions};
use colored::Colorize;

pub fn checkmark() -> String {
    "✔".green().bold().to_string() // U+2714 Heavy Check Mark
}

pub fn cross() -> String {
    "✘".red().bold().to_string() // U+2718 Heavy Ballot X
}

pub fn error() -> String {
    "error:".red().bold().to_string()
}

pub fn warning() -> String {
    "warning:".yellow().bold().to_string()
}

pub fn hint() -> String {
    "hint:".cyan().bold().to_string()
}

pub fn watch() -> String {
    "watch:".yellow().bold().to_string()
}

pub fn dimmed_border_text_block(content: &str) -> String {
    let options = TextBlockOptions {
        top_line: TextBlockOptions::CORNER_TOP.dimmed().to_string(),
        bottom_line: TextBlockOptions::CORNER_BOTTOM.dimmed().to_string(),
        format_line: |line| {
            if line.is_empty() {
                TextBlockOptions::BORDER.dimmed().to_string()
            } else {
                format!("{} {line}", TextBlockOptions::BORDER.dimmed())
            }
        },
    };
    string::text_block_with_options(content, &options)
}
