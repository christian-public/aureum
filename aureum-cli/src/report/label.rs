use colored::Colorize;

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
