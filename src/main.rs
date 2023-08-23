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
    /// Remve value of Xenstore path
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
    /// Watch on path on Xenstore.
    Watch {
        #[arg()]
        path: String,
    },
    /// Watch on path on Xenstore (using async)
    #[cfg(feature = "async_watch")]
    WatchAsync {
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
        Command::Rm{path} => cmd_rm(&xs, &path),
        Command::Write{path, data} => cmd_write(&xs, &path, &data),
        Command::Watch { path } => cmd_watch(&xs, &path),
        #[cfg(feature = "async_watch")]
        Command::WatchAsync { path } => cmd_watch_async(&xs, &path),
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

fn cmd_rm(xs: &Xs, path: &String) {
    let value = xs.rm(XBTransaction::Null, &path)
        .expect("cannot rm xenstore path");
}

fn cmd_write(xs: &Xs, path: &String, data: &String) {
    xs.write(XBTransaction::Null, &path, &data)
        .expect("cannot write to xenstore path");
}

fn cmd_watch(xs: &Xs, path: &String) {
    let token = "xenstore-rs-token";

    xs.watch(&path, token).expect("cannot set watch");

    while let Ok(events) = xs.read_watch() {
        for event in events {
            println!(
                "{}: {:?}",
                event.path,
                xs.read(XBTransaction::Null, &event.path)
            );
        }
    }

    xs.unwatch(path, token).expect("cannot unwatch");
}

#[cfg(feature = "async_watch")]
fn cmd_watch_async(xs: &Xs, path: &String) {
    use futures::StreamExt;
    use tokio::runtime::Builder;

    let rt = Builder::new_current_thread()
        .enable_io()
        .build()
        .unwrap();

    rt.block_on(async move {
        xs.watch(path, "xenstore-rs-token")
            .expect("cannot set watch");

        let mut stream = xs.get_stream().unwrap();

        while let Some(event) = stream.next().await {
            println!(
                "{}: {:?}",
                event.path,
                xs.read(XBTransaction::Null, &event.path)
            );
        }
    });
}
