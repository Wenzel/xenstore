use clap::{Parser, Subcommand};
use xenstore_rs::{unix::XsUnix, Xs};

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
    /// Remove value of Xenstore path
    Rm {
        #[arg()]
        path: String,
    },
    /// Write value to Xenstore path
    Write {
        #[arg()]
        path: String,
        #[arg()]
        data: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let mut xs = XsUnix::new().expect("xenstore should open");

    match cli.command {
        Command::List { path } => cmd_list(&mut xs, &path),
        Command::Read { path } => cmd_read(&mut xs, &path),
        Command::Rm { path } => cmd_rm(&mut xs, &path),
        Command::Write { path, data } => cmd_write(&mut xs, &path, &data),
    }
}

fn cmd_list(xs: &mut impl Xs, path: &String) {
    let values = xs.directory(&path).expect("path should be readable");
    for value in values {
        println!("{}", value);
    }
}

fn cmd_read(xs: &mut impl Xs, path: &String) {
    let value = xs.read(&path).expect("path should be readable");
    println!("{}", value);
}

fn cmd_rm(xs: &mut impl Xs, path: &String) {
    xs.rm(&path).expect("cannot rm xenstore path");
}

fn cmd_write(xs: &mut impl Xs, path: &String, data: &String) {
    xs.write(&path, &data)
        .expect("cannot write to xenstore path");
}
