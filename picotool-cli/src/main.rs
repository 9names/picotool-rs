use camino::Utf8PathBuf;
use clap::{Args, Parser};
use picotool::{picotool_reset::reset_usb_device, PicoTool};

#[derive(Parser)]
struct Cli {
    /// Force-reset a USB device that supports the "Reset to Bootsel" instruction
    #[arg(short, global = true)]
    force_reset: bool,
    #[command(subcommand)]
    cmd: Subcommand,
}

#[derive(Debug, Args)]
struct WriteArgs {
    /// UF2 file to flash
    target_file: Utf8PathBuf,
}

#[derive(clap::Subcommand)]
enum Subcommand {
    /// Load data into flash on your RP microcontroller
    Load(WriteArgs),
}

fn main() {
    let cli = Cli::parse();

    if cli.force_reset {
        reset_usb_device();
        return;
    }

    let mut tool = PicoTool::new();
    match cli.cmd {
        Subcommand::Load(write_args) => {
            tool.flash_uf2(write_args.target_file.as_std_path());
        }
    }
}
