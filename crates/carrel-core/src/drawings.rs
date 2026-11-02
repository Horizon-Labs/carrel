//! Handwritten notes on cards (FORMAT.md §5.10), drawn with Apple Pencil on iPad: each is a
//! drawing and a picture of it, kept in `_System/drawings/<item-id>/`, listed in the card's
//! `drawings` field with the text recognised in it, and shown in the card's `drawings`
//! managed section. Saving or removing one is an ordinary change in the history, so undo
//! reverses it, drawing files and all.
//!
//! The drawing itself is PencilKit's data, which only Apple's platforms can edit; the picture
//! (PNG) and the recognised text are what every other interface shows, and the text is what
//! search and the librarian find.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::actions::ActionError;
use crate::archive::{Archive, ItemRef};
use crate::card::{CardDrawing, Id};
use crate::card_file::CardFile;
use crate::coordinator::{Coordinator as _, DEFAULT_LEASE, DiskLock, Holder};
use crate::history::{Change, History, Operation};

/// Where drawings are kept, relative to the archive root: a folder per item inside.
pub const DRAWINGS_DIR: &str = "_System/drawings";

/// The card's managed section showing its drawings (FORMAT.md §5.3).
pub const DRAWINGS_SECTION: &str = "drawings";

/// The extension of a drawing's data: PencilKit's `PKDrawing` data representation.
pub const DRAWING_EXTENSION: &str = "drawing";

/// The largest drawing, or picture of one, that's kept, in bytes.
pub const MAX_DRAWING_BYTES: usize = 32 * 1024 * 1024;

/// How many characters of recognised text a drawing keeps.
pub const MAX_DRAWING_TEXT: usize = 20_000;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// A drawing to save on a card.
#[derive(Debug, Clone, Copy)]
pub struct DrawingToSave<'a> {
    /// The drawing, as PencilKit's data representation.
    pub data: &'a [u8],
    /// A PNG picture of it.
    pub picture: &'a [u8],
    /// The text recognised in it; empty when there's none.
    pub text: &'a str,
}

/// A card's drawing, as the interfaces list it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListedDrawing {
    /// The drawing's record on the card.
    #[serde(flatten)]
    pub drawing: CardDrawing,
    /// Its PencilKit data, relative to the archive root.
    pub data: String,
    /// Its picture, relative to the archive root.
    pub picture: String,
}

/// What saving or removing a drawing did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DrawingChange {
    /// The drawing.
    pub id: Id,
    /// Whether anything changed: saving a drawing exactly as it is changes nothing.
    pub changed: bool,
    /// The change's summary, as the history shows it.
    pub summary: String,
    /// The history commit, when one was made.
    pub commit: Option<String>,
}

/// The paths of drawing `drawing` of item `item`, relative to the archive root: its data and
/// its picture.
#[must_use]
pub fn card_drawing_paths(item: Id, drawing: Id) -> (String, String) {
    let folder = format!("{DRAWINGS_DIR}/{item}");
    (
        format!("{folder}/{drawing}.{DRAWING_EXTENSION}"),
        format!("{folder}/{drawing}.png"),
    )
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ActionError + '_ {
    move |source| ActionError::Io {
        path: path.to_owned(),
        source,
    }
}

/// `item`'s card, read to change its drawings.
fn read_card(archive: &Archive, item: &ItemRef) -> Result<CardFile, ActionError> {
    let path = archive.root().join(&item.card);
    let text = std::fs::read_to_string(&path).map_err(io(&path))?;
    let file = CardFile::parse(&text)
        .map_err(|e| ActionError::Invalid(format!("{}: {e}", item.card)))?;
    if file.card().id != item.id {
        return Err(ActionError::Invalid(format!(
            "{} describes another item now",
            item.card
        )));
    }
    Ok(file)
}

/// The drawings on `item`'s card, oldest first, with where their files are.
///
/// # Errors
///
/// [`ActionError::Invalid`] if the card can't be parsed, or [`ActionError::Io`].
pub fn card_drawings(archive: &Archive, item: &ItemRef) -> Result<Vec<ListedDrawing>, ActionError> {
    let file = read_card(archive, item)?;
    Ok(file
        .card()
        .drawings
        .iter()
        .map(|drawing| {
            let (data, picture) = card_drawing_paths(item.id, drawing.id);
            ListedDrawing {
                drawing: drawing.clone(),
                data,
                picture,
            }
        })
        .collect())
}

/// `text` tidied to keep: lines trimmed, blank runs and ends dropped, cut to
/// [`MAX_DRAWING_TEXT`], `None` when nothing's left.
fn tidy_text(text: &str) -> Option<String> {
    let mut tidy = String::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            if !tidy.is_empty() && !tidy.ends_with("\n\n") {
                tidy.push('\n');
            }
            continue;
        }
        tidy.push_str(line);
        tidy.push('\n');
    }
    let tidy: String = tidy.trim().chars().take(MAX_DRAWING_TEXT).collect();
    (!tidy.is_empty()).then_some(tidy)
}

fn check_drawing(drawing: &DrawingToSave) -> Result<(), ActionError> {
    if drawing.data.is_empty() {
        return Err(ActionError::Invalid("the drawing is empty".into()));
    }
    if !drawing.picture.starts_with(PNG_SIGNATURE) {
        return Err(ActionError::Invalid(
            "a drawing's picture must be a PNG".into(),
        ));
    }
    if drawing.data.len() > MAX_DRAWING_BYTES || drawing.picture.len() > MAX_DRAWING_BYTES {
        return Err(ActionError::Invalid(format!(
            "a drawing is at most {} MB",
            MAX_DRAWING_BYTES / 1024 / 1024
        )));
    }
    Ok(())
}

/// Saves `drawing` on `item`'s card as `holder`: a new drawing when `id` is `None`, else a
/// new version of drawing `id`. Its files are written, the card's `drawings` field and
/// section updated, and both recorded as one change in the history.
///
/// # Errors
///
/// [`ActionError::Invalid`] for an empty drawing, a picture that isn't a PNG, one that's too
/// large, an unknown `id`, or an item that's checked out (draw on it when it's returned, so
/// the return can't overwrite it); [`ActionError::Lease`] if another writer holds the
/// archive; or an error writing.
pub fn save_card_drawing(
    archive: &Archive,
    item: &ItemRef,
    id: Option<Id>,
    drawing: &DrawingToSave,
    holder: Holder,
) -> Result<DrawingChange, ActionError> {
    check_drawing(drawing)?;
    let mut file = read_card(archive, item)?;
    refuse_checked_out(&file)?;
    let now = crate::questions::now();
    let text = tidy_text(drawing.text);
    let title = file.card().title.clone();
    let (id, summary) = if let Some(id) = id {
        let record = file
            .card_mut()
            .drawings
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| ActionError::Invalid(format!("“{title}” has no drawing {id}")))?;
        let (data, picture) = card_drawing_paths(item.id, id);
        let same_files = std::fs::read(archive.root().join(&data)).ok().as_deref()
            == Some(drawing.data)
            && std::fs::read(archive.root().join(&picture)).ok().as_deref()
                == Some(drawing.picture);
        if same_files && record.text == text {
            return Ok(DrawingChange {
                id,
                changed: false,
                summary: format!("Change a drawing on “{title}”"),
                commit: None,
            });
        }
        record.text = text;
        record.modified = Some(now);
        record.device = Some(holder.device);
        (id, format!("Change a drawing on “{title}”"))
    } else {
        let id = Id::new_v7();
        file.card_mut().drawings.push(CardDrawing {
            id,
            text,
            at: Some(now),
            modified: None,
            person: Some(holder.person),
            device: Some(holder.device),
            extra: crate::card::Extra::new(),
        });
        (id, format!("Draw a note on “{title}”"))
    };
    render_section(&mut file, item.id)?;
    let commit = write(archive, item, &file, &summary, holder, |root| {
        let (data, picture) = card_drawing_paths(item.id, id);
        write_file(&root.join(data), drawing.data)?;
        write_file(&root.join(picture), drawing.picture)
    })?;
    Ok(DrawingChange {
        id,
        changed: true,
        summary,
        commit,
    })
}

/// Removes drawing `id` from `item`'s card as `holder`, with its files: one change in the
/// history, which undo reverses.
///
/// # Errors
///
/// As [`save_card_drawing`].
pub fn remove_card_drawing(
    archive: &Archive,
    item: &ItemRef,
    id: Id,
    holder: Holder,
) -> Result<DrawingChange, ActionError> {
    let mut file = read_card(archive, item)?;
    refuse_checked_out(&file)?;
    let summary = format!("Remove a drawing from “{}”", file.card().title);
    let before = file.card().drawings.len();
    file.card_mut().drawings.retain(|d| d.id != id);
    if file.card().drawings.len() == before {
        return Ok(DrawingChange {
            id,
            changed: false,
            summary,
            commit: None,
        });
    }
    render_section(&mut file, item.id)?;
    let commit = write(archive, item, &file, &summary, holder, |root| {
        let (data, picture) = card_drawing_paths(item.id, id);
        for path in [root.join(data), root.join(picture)] {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io(&path)(e)),
                _ => {}
            }
        }
        let folder = root.join(DRAWINGS_DIR).join(item.id.to_string());
        // Only when it's empty: the item's other drawings stay.
        let _ = std::fs::remove_dir(folder);
        Ok(())
    })?;
    Ok(DrawingChange {
        id,
        changed: true,
        summary,
        commit,
    })
}

fn refuse_checked_out(file: &CardFile) -> Result<(), ActionError> {
    if file.card().checked_out.is_some() {
        return Err(ActionError::Invalid(
            "it's checked out; draw on its card when it's returned, so the return can't overwrite it"
                .into(),
        ));
    }
    Ok(())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), ActionError> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(io(folder))?;
    }
    let written = PathBuf::from(format!("{}.tmp-drawing", path.display()));
    std::fs::write(&written, bytes).map_err(io(&written))?;
    std::fs::rename(&written, path).map_err(io(path))
}

/// The `drawings` section for `drawings` of item `item`: each drawing's picture, embedded by
/// its path from the archive root so it survives the card moving, and its text.
#[must_use]
pub fn drawings_section(item: Id, drawings: &[CardDrawing]) -> String {
    let mut text = String::from("## Drawings\n");
    for drawing in drawings {
        let (_, picture) = card_drawing_paths(item, drawing.id);
        let when = drawing
            .modified
            .as_ref()
            .or(drawing.at.as_ref())
            .map_or("Drawing", |at| at.0.get(..10).unwrap_or(&at.0));
        write!(text, "\n### {when}\n![[{picture}]]\n").unwrap();
        if let Some(said) = &drawing.text {
            text.push('\n');
            for line in said.lines() {
                if crate::sections::safe_inside(line) {
                    writeln!(text, "> {line}").unwrap();
                }
            }
        }
    }
    text
}

/// Writes `file`'s `drawings` section from its field: before the history section when it's
/// added, and removed when there are no drawings left.
fn render_section(file: &mut CardFile, item: Id) -> Result<(), ActionError> {
    let invalid = |e: crate::sections::SectionError| ActionError::Invalid(e.to_string());
    let body = file.body().to_owned();
    let drawings = &file.card().drawings;
    let sections = crate::sections::find_all(&body).map_err(invalid)?;
    let existing = sections.iter().find(|s| s.name == DRAWINGS_SECTION);
    let updated = match (existing, drawings.is_empty()) {
        (None, true) => return Ok(()),
        (Some(_), false) => {
            crate::sections::replace(&body, DRAWINGS_SECTION, &drawings_section(item, drawings))
                .map_err(invalid)?
        }
        (Some(section), true) => {
            let mut end = section.whole.end;
            if body[end..].starts_with('\n') {
                end += 1;
            }
            format!("{}{}", &body[..section.whole.start], &body[end..])
        }
        (None, false) => {
            let content = drawings_section(item, drawings);
            match sections.iter().find(|s| s.name == "history") {
                Some(history) => {
                    let mut out = body[..history.whole.start].to_owned();
                    write!(
                        out,
                        "<!-- carrel:{DRAWINGS_SECTION} -->\n{content}<!-- /carrel:{DRAWINGS_SECTION} -->\n\n"
                    )
                    .unwrap();
                    out.push_str(&body[history.whole.start..]);
                    out
                }
                None => crate::sections::append_section(&body, DRAWINGS_SECTION, &content)
                    .map_err(invalid)?,
            }
        }
    };
    file.set_body(&updated);
    Ok(())
}

/// Writes the card `file` for `item`, and its drawing files with `files`, holding the writer
/// lease as `holder`, and records them as one change.
fn write(
    archive: &Archive,
    item: &ItemRef,
    file: &CardFile,
    summary: &str,
    holder: Holder,
    files: impl FnOnce(&Path) -> Result<(), ActionError>,
) -> Result<Option<String>, ActionError> {
    // TODO(#497): while another device is the writer, send the drawing through this device's
    // inbox, with its files beside the entry, once the inbox carries every operation.
    let card_path = archive.root().join(&item.card);
    let text = file
        .to_text()
        .map_err(|e| ActionError::Invalid(format!("{}: {e}", item.card)))?;
    let lease = DiskLock::for_archive(archive).acquire(holder, "drawing", DEFAULT_LEASE)?;
    let result = (|| -> Result<Option<String>, ActionError> {
        files(archive.root())?;
        let written = card_path.with_extension("md.tmp-drawing");
        std::fs::write(&written, text).map_err(io(&written))?;
        std::fs::rename(&written, &card_path).map_err(io(&card_path))?;
        let mut change = Change::new(
            Operation::Edit,
            summary,
            lease.info().person,
            lease.info().device,
        );
        change.items = vec![item.id];
        Ok(History::open(archive)?.commit(archive, &change, &lease)?)
    })();
    lease.release()?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const PICTURE: &[u8] = b"\x89PNG\r\n\x1a\nfake picture";

    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                copy(&e.path(), &to.join(e.file_name()));
            } else {
                std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
            }
        }
    }

    fn sample() -> (tempfile::TempDir, Archive, ItemRef, Holder) {
        let dir = tempfile::tempdir().unwrap();
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-archive"),
            &dir.path().join("a"),
        );
        let archive = Archive::open(&dir.path().join("a")).unwrap();
        let holder = Holder {
            person: Id::new_v7(),
            device: Id::new_v7(),
        };
        let lease = DiskLock::for_archive(&archive)
            .acquire(holder, "test", DEFAULT_LEASE)
            .unwrap();
        History::init(&archive, &lease).unwrap();
        lease.release().unwrap();
        let item = archive
            .items()
            .unwrap()
            .into_iter()
            .find(|i| i.item.is_some())
            .unwrap();
        (dir, archive, item, holder)
    }

    fn undo(archive: &Archive, commit: &str, holder: Holder) {
        let lease = DiskLock::for_archive(archive)
            .acquire(holder, "test", DEFAULT_LEASE)
            .unwrap();
        History::open(archive)
            .unwrap()
            .undo(archive, commit, &lease)
            .unwrap();
        lease.release().unwrap();
    }

    #[test]
    fn a_drawing_is_saved_listed_changed_and_removed() {
        let (_dir, archive, item, holder) = sample();
        let drawing = DrawingToSave {
            data: b"strokes",
            picture: PICTURE,
            text: "  call Ana about\n\n\n the figures  \n",
        };
        let saved = save_card_drawing(&archive, &item, None, &drawing, holder).unwrap();
        assert!(saved.changed);
        assert!(saved.commit.is_some());

        let listed = card_drawings(&archive, &item).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].drawing.text.as_deref(),
            Some("call Ana about\n\nthe figures")
        );
        assert_eq!(
            std::fs::read(archive.root().join(&listed[0].data)).unwrap(),
            b"strokes"
        );
        let card = std::fs::read_to_string(archive.root().join(&item.card)).unwrap();
        let section = crate::sections::get(&card, DRAWINGS_SECTION).unwrap().unwrap();
        assert!(section.contains(&format!("![[{}]]", listed[0].picture)));
        assert!(section.contains("> call Ana about"));
        // Before the history section.
        assert!(card.find("carrel:drawings") < card.find("carrel:history"));

        let again = save_card_drawing(&archive, &item, Some(saved.id), &drawing, holder).unwrap();
        assert!(!again.changed);
        let changed = DrawingToSave {
            text: "call Ana",
            ..drawing
        };
        let changed = save_card_drawing(&archive, &item, Some(saved.id), &changed, holder).unwrap();
        assert!(changed.changed);
        let listed = card_drawings(&archive, &item).unwrap();
        assert!(listed[0].drawing.modified.is_some());

        let removed = remove_card_drawing(&archive, &item, saved.id, holder).unwrap();
        assert!(removed.changed);
        assert!(card_drawings(&archive, &item).unwrap().is_empty());
        assert!(!archive.root().join(&listed[0].picture).exists());
        let card_after = std::fs::read_to_string(archive.root().join(&item.card)).unwrap();
        assert!(!card_after.contains("carrel:drawings"));
    }

    #[test]
    fn undo_brings_a_removed_drawing_back() {
        let (_dir, archive, item, holder) = sample();
        let drawing = DrawingToSave {
            data: b"strokes",
            picture: PICTURE,
            text: "",
        };
        let saved = save_card_drawing(&archive, &item, None, &drawing, holder).unwrap();
        let listed = card_drawings(&archive, &item).unwrap();
        let removed = remove_card_drawing(&archive, &item, saved.id, holder).unwrap();
        undo(&archive, removed.commit.as_deref().unwrap(), holder);
        assert_eq!(card_drawings(&archive, &item).unwrap().len(), 1);
        assert_eq!(
            std::fs::read(archive.root().join(&listed[0].picture)).unwrap(),
            PICTURE
        );
    }

    #[test]
    fn a_picture_that_isnt_a_png_is_refused() {
        let (_dir, archive, item, holder) = sample();
        let drawing = DrawingToSave {
            data: b"strokes",
            picture: b"GIF89a",
            text: "",
        };
        assert!(matches!(
            save_card_drawing(&archive, &item, None, &drawing, holder),
            Err(ActionError::Invalid(_))
        ));
    }
}
