use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::cli::Cli;

pub fn run(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "pjman", &mut std::io::stdout());
}
