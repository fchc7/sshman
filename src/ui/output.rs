use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⦦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

pub fn print_success(msg: &str) {
    eprintln!("{} {}", ">>".green().bold(), msg);
}

pub fn print_error(msg: &str) {
    eprintln!("{} {}", "!!".red().bold(), msg);
}

pub fn print_warn(msg: &str) {
    eprintln!("{} {}", "??".yellow().bold(), msg);
}

pub fn print_info(msg: &str) {
    eprintln!("{} {}", "--".cyan().bold(), msg);
}
