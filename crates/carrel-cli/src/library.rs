//! `carrel library`: what every interface opens an archive on (`docs/design/library-home.md`),
//! computed by `carrel_core::library`: what waits for the owner, the top-level collections
//! with their counts, the items filed most recently, and On This Day.

use std::fmt::Write as _;
use std::path::Path;

use carrel_core::discover::Catalog;
use carrel_core::discover::today::{self, OnThisDay};
use carrel_core::library::{self, Library, Waiting};
use serde_json::json;

use crate::commands::{locate, plural};
use crate::{Failure, Report};

/// `carrel library`.
///
/// # Errors
///
/// [`Failure`] if the archive can't be found or read.
pub fn library_command(archive: Option<&Path>) -> Result<Report, Failure> {
    let archive = locate(archive)?;
    let catalog = Catalog::read(&archive)?;
    let library = library::library(&catalog, library::RECENT);
    let waiting = Waiting::read(&archive, library.flagged);
    let day = today::on_this_day(&catalog, crate::discover::today());
    Ok(Report::new(
        text(&library, &waiting, &day),
        json!({
            "library": library,
            "waiting": waiting,
            "on_this_day": day,
        }),
    ))
}

/// The report: each part only when it has something, as the apps' Library page shows them.
fn text(library: &Library, waiting: &Waiting, day: &OnThisDay) -> String {
    let mut out = String::new();
    if let Some(line) = waiting.line() {
        let _ = writeln!(out, "Waiting: {line}.\n");
    }
    if library.is_empty() {
        out.push_str("Nothing is filed yet. Add files with `carrel ingest PATH`.\n");
        return out;
    }
    out.push_str("Collections\n");
    let counts: Vec<String> = library
        .shelves
        .iter()
        .map(|shelf| {
            let mut count = plural(shelf.items, "item");
            if shelf.collections > 0 {
                let _ = write!(count, ", {}", plural(shelf.collections, "collection"));
            }
            count
        })
        .collect();
    let titles = library
        .shelves
        .iter()
        .map(|s| s.title.chars().count())
        .max()
        .unwrap_or(0);
    let widest = counts.iter().map(|c| c.chars().count()).max().unwrap_or(0);
    for (shelf, count) in library.shelves.iter().zip(&counts) {
        let line = format!(
            "  {:<titles$}  {count:<widest$}  {}",
            shelf.title,
            shelf.description.as_deref().unwrap_or("")
        );
        let _ = writeln!(out, "{}", line.trim_end());
    }
    if !library.recent.is_empty() {
        out.push_str("\nRecently added\n");
        for item in &library.recent {
            let _ = writeln!(out, "  {}  ({})", item.title, item.collection);
        }
    }
    if !day.years.is_empty() {
        out.push_str("\nOn this day\n");
        for year in &day.years {
            let titles: Vec<&str> = year.items.iter().map(|h| h.item.title.as_str()).collect();
            let _ = writeln!(
                out,
                "  {} ({} ago): {}",
                year.year,
                plural(usize::try_from(year.years_ago).unwrap_or(0), "year"),
                titles.join(", ")
            );
        }
    }
    out
}
