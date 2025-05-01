#[derive(clap::Parser)]
#[command(
    name = "Bundler",
    version = "0.1.0",
    about = "Example CLI",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

impl Cli {
    pub fn command(&self) -> Option<&Command> {
        self.command.as_ref()
    }
}

#[derive(clap::Subcommand)]
pub enum Command {
    Install,
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    Exec {
        args: Vec<String>,
    },
    Lock,
}
