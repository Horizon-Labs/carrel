//! `carrel drawings`: the handwritten notes on an item's card (FORMAT.md §5.10), drawn with
//! Apple Pencil on iPad or the pointer in the Mac app. A terminal can't draw them, so it lists
//! each with the text recognised in it and the path to its picture, and can remove one; `carrel
//! show` lists them too. Removing one is a change undo can reverse.

use std::fmt::Write as _;
use std::path::Path;

use carrel_core::archive::{Archive, ItemRef};
use carrel_core::card::Id;
use carrel_core::drawings::{self, ListedDrawing};
use serde_json::{Value, json};

use crate::commands::{find_item, holder, locate};
use crate::config::Identity;
use crate::{Failure, Report};

/// `carrel drawings ITEM [--remove ID]`.
///
/// # Errors
///
/// [`Failure`] if nothing matches, the drawing isn't on the card, the item is checked out, or
/// the change can't be written.
pub fn drawings_command(
    archive: Option<&Path>,
    query: &str,
    remove: Option<&str>,
) -> Result<Report, Failure> {
    let archive = locate(archive)?;
    let items = archive.items()?;
    let item = find_item(&archive, &items, query)?.clone();
    if let Some(remove) = remove {
        let id = remove
            .trim()
            .parse()
            .map(Id)
            .map_err(|_| Failure::new(format!("{remove} isn't a drawing's identifier")))?;
        let identity = Identity::load_or_create()?;
        let done = drawings::remove_card_drawing(&archive, &item, id, holder(&identity))?;
        if !done.changed {
            return Err(Failure::new(format!(
                "{} has no drawing {id}; `carrel drawings {query}` lists them",
                item.card
            )));
        }
        return Ok(Report::new(
            format!(
                "{}; undo it from History or with `carrel undo`.\n",
                done.summary
            ),
            json!({ "removed": id, "commit": done.commit }),
        ));
    }
    let (text, json) = listing(&archive, &item)?;
    let text = if text.is_empty() {
        format!(
            "{} has no drawings. Write one on its card with Apple Pencil in Carrel on iPad, or draw one in the Mac app.\n",
            item.card
        )
    } else {
        text.trim_start().to_owned()
    };
    Ok(Report::new(text, json!({ "drawings": json })))
}

/// The part of `carrel show` about the item's drawings, empty when it has none, and them as
/// JSON.
///
/// # Errors
///
/// [`Failure`] if the card can't be read.
pub fn listing(archive: &Archive, item: &ItemRef) -> Result<(String, Value), Failure> {
    let listed = drawings::card_drawings(archive, item)?;
    let mut text = String::new();
    if !listed.is_empty() {
        writeln!(text, "\nDrawings:").unwrap();
    }
    for drawing in &listed {
        let when = drawing
            .drawing
            .modified
            .as_ref()
            .or(drawing.drawing.at.as_ref())
            .map_or("", |at| at.0.get(..10).unwrap_or(&at.0));
        writeln!(text, "  {when}  {}", drawing.drawing.id).unwrap();
        match &drawing.drawing.text {
            Some(said) => {
                for line in said.lines() {
                    writeln!(text, "    {line}").unwrap();
                }
            }
            None => writeln!(text, "    (no words recognised)").unwrap(),
        }
        writeln!(
            text,
            "    picture: {}",
            archive.root().join(&drawing.picture).display()
        )
        .unwrap();
    }
    let json = listed
        .iter()
        .map(|ListedDrawing { drawing, data, picture }| {
            json!({
                "id": drawing.id,
                "text": drawing.text,
                "at": drawing.at,
                "modified": drawing.modified,
                "data": data,
                "picture": picture,
            })
        })
        .collect();
    Ok((text, Value::Array(json)))
}
