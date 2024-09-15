use clap::Parser;
use picotool::PicoTool;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    /// UF2 file to flash
    #[arg(value_name = "FILE")]
    target_file: PathBuf,
}
fn main() {
    let cli = Cli::parse();

    let mut tool = PicoTool::new();
    tool.flash_uf2(&cli.target_file);
}
