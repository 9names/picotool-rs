use clap::Parser;
use picotool::{picotool_reset::reset_usb_device, PicoTool};
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[arg(short)]
    reset: bool,
    /// UF2 file to flash
    #[arg(value_name = "FILE")]
    target_file: Option<PathBuf>,
}
fn main() {
    let cli = Cli::parse();

    if cli.reset {
        reset_usb_device();
        return;
    }

    let mut tool = PicoTool::new();
    if let Some(file) = &cli.target_file {
        tool.flash_uf2(file);
    }
}
