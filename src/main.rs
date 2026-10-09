use clap::{CommandFactory, Parser};

fn main() -> anyhow::Result<()> {
    let cli = aiyou::Cli::parse();
    if let Some(shell) = cli.shell {
        let mut cmd = aiyou::Cli::command();
        clap_complete::generate(shell, &mut cmd, "aiyou", &mut std::io::stdout());
        return Ok(());
    }
    let root = cli.root.unwrap_or_else(aiyou::store::default_root);
    match cli.cmd {
        Some(cmd) => aiyou::run(&root, cmd),
        None => {
            aiyou::Cli::command().print_help()?;
            Ok(())
        }
    }
}
