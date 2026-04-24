use colored::Colorize;

pub fn checkmark() -> String {
    "✔".green().bold().to_string() // U+2714 HEAVY CHECK MARK
}

pub fn cross() -> String {
    "✘".red().bold().to_string() // U+2718 HEAVY BALLOT X
}
