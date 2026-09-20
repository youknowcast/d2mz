use anyhow::{Result, bail};

use crate::cli::{TagAddArgs, TagArgs, TagCommand, TagListArgs, TagRemoveArgs};
use d2mz_archive::Archive;
use d2mz_archive::list::EntryRecord;

pub fn run(archive: &Archive, args: TagArgs) -> Result<()> {
    match args.command {
        TagCommand::Add(args) => add(archive, args),
        TagCommand::Rm(args) => remove(archive, args),
        TagCommand::List(args) => list(archive, args),
    }
}

fn add(archive: &Archive, args: TagAddArgs) -> Result<()> {
    let tags = parse_tags(&args.tags)?;
    let targets = resolve_targets(archive, &args.targets, args.prefix.as_deref())?;
    if targets.is_empty() {
        bail!("no entries matched the given target");
    }

    let mut changed = 0usize;
    for entry in &targets {
        for tag in &tags {
            if archive.index().add_tag(entry.id, tag)? {
                changed += 1;
            }
        }
    }

    if args.json {
        let records: Vec<EntryRecord> = targets.iter().map(EntryRecord::from).collect();
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else {
        eprintln!(
            "added {} tag(s) across {} entry(ies)",
            changed,
            targets.len()
        );
    }
    Ok(())
}

fn remove(archive: &Archive, args: TagRemoveArgs) -> Result<()> {
    let tags = parse_tags(&args.tags)?;
    let targets = resolve_targets(archive, &args.targets, args.prefix.as_deref())?;
    let mut changed = 0usize;
    for entry in &targets {
        for tag in &tags {
            if archive.index().remove_tag(entry.id, tag)? {
                changed += 1;
            }
        }
    }
    eprintln!("removed {} tag(s)", changed);
    Ok(())
}

fn list(archive: &Archive, args: TagListArgs) -> Result<()> {
    if let Some(tag) = &args.tag {
        let entries = archive.index().entries_with_tag(tag)?;
        let records: Vec<EntryRecord> = entries.iter().map(EntryRecord::from).collect();
        return print_records(&records, args.long, args.json, |record| {
            crate::output::entry_line(record)
        });
    }

    let tags = archive.index().tags()?;
    if args.json {
        let payload: Vec<_> = tags
            .iter()
            .map(|stat| serde_json::json!({ "tag": stat.tag, "entries": stat.entries }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if tags.is_empty() {
        eprintln!("no tags");
    } else {
        for stat in &tags {
            println!("{}\t{}", stat.tag, stat.entries);
        }
    }
    Ok(())
}

/// Turn a comma-separated tag string into trimmed, non-empty tags.
fn parse_tags(raw: &str) -> Result<Vec<String>> {
    let tags: Vec<String> = raw
        .split(',')
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect();
    if tags.is_empty() {
        bail!("no tags given");
    }
    Ok(tags)
}

/// Resolve target selectors to entries.
fn resolve_targets(
    archive: &Archive,
    selectors: &[String],
    prefix: Option<&str>,
) -> Result<Vec<d2mz_archive::db::Entry>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(prefix) = prefix {
        for entry in archive.index().entries_matching_prefix(prefix)? {
            if seen.insert(entry.id) {
                out.push(entry);
            }
        }
    }
    for selector in selectors {
        if let Ok(id) = selector.parse::<i64>()
            && let Some(entry) = archive.index().entry(id)?
            && seen.insert(entry.id)
        {
            out.push(entry);
            continue;
        }
        for entry in archive.index().find_entries(selector)? {
            if seen.insert(entry.id) {
                out.push(entry);
            }
        }
    }
    Ok(out)
}

fn print_records<F>(records: &[EntryRecord], long: bool, json: bool, plain: F) -> Result<()>
where
    F: Fn(&EntryRecord) -> String,
{
    if json {
        println!("{}", serde_json::to_string_pretty(records)?);
    } else if long {
        for record in records {
            println!("{}", crate::output::entry_long(record));
        }
    } else {
        for record in records {
            println!("{}", plain(record));
        }
    }
    Ok(())
}
