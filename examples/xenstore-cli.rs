use clap::{Parser, Subcommand};
use xenstore_rs::{Xs, XsOpenFlags, XBTransaction};

/// Demo/test tool for xenstore Rust bindings
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List Xenstore keys in path
    List {
        #[arg()]
        path: String,
    },
    /// Read value of Xenstore path
    Read {
        #[arg()]
        path: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let xs = Xs::new(XsOpenFlags::ReadOnly)
        .expect("xenstore should open");

    match cli.command {
        Command::List{path} => cmd_list(&xs, &path),
        Command::Read{path} => cmd_read(&xs, &path),
    }
}

fn cmd_list(xs: &Xs, path: &String) {
    let values = xs.directory(XBTransaction::Null, &path)
        .expect("path should be readable");
    for value in values {
        println!("{}", value);
    }
}

fn cmd_read(xs: &Xs, path: &String) {
    let value = xs.read(XBTransaction::Null, &path)
        .expect("path should be readable");
    println!("{}", value);
}
