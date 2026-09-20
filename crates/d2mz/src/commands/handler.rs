use anyhow::Result;

use crate::cli::{HandlerArgs, HandlerCommand, HandlerListArgs, HandlerRemoveArgs, HandlerSetArgs};
use d2mz_archive::Archive;

pub fn run(archive: &Archive, args: HandlerArgs) -> Result<()> {
    match args.command {
        HandlerCommand::Set(args) => set(archive, args),
        HandlerCommand::Rm(args) => remove(archive, args),
        HandlerCommand::List(args) => list(archive, args),
    }
}

fn set(archive: &Archive, args: HandlerSetArgs) -> Result<()> {
    archive
        .index()
        .set_handler(&args.kind, &args.matcher, &args.app)?;
    eprintln!("{} {} -> {}", args.kind, args.matcher, args.app);
    Ok(())
}

fn remove(archive: &Archive, args: HandlerRemoveArgs) -> Result<()> {
    let removed = archive.index().remove_handler(&args.kind, &args.matcher)?;
    if removed {
        eprintln!("removed {} {}", args.kind, args.matcher);
    } else {
        eprintln!("no handler for {} {}", args.kind, args.matcher);
    }
    Ok(())
}

fn list(archive: &Archive, args: HandlerListArgs) -> Result<()> {
    let handlers = archive.index().handlers()?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&handlers)?);
    } else if handlers.is_empty() {
        eprintln!("no handlers configured");
    } else {
        for handler in &handlers {
            println!("{:<8} {:<8} {}", handler.kind, handler.matcher, handler.app);
        }
    }
    Ok(())
}
