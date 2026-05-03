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
