use colored::Colorize;

pub fn checkmark() -> String {
    "✔".green().bold().to_string() // U+2714 Heavy Check Mark
}

pub fn cross() -> String {
    "✘".red().bold().to_string() // U+2718 Heavy Ballot X
}
