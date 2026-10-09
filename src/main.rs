use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = aiyou::Cli::parse();
    let root = cli.root.unwrap_or_else(aiyou::store::default_root);
    aiyou::run(&root, cli.cmd)
}
