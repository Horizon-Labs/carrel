//! Handwritten notes on cards (FORMAT.md §5.10): the drawings an item's card has, saved with
//! Apple Pencil on iPad and shown everywhere by their picture and recognised text. Saving or
//! removing one is one change undo can reverse.

use carrel_core::card::Id;
use carrel_core::drawings::{DrawingToSave, ListedDrawing};

use crate::{ArchiveHandle, CarrelError};

/// A handwritten note on a card.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CardDrawingInfo {
    /// Its identifier.
    pub id: String,
    /// The text recognised in it, if any.
    pub text: Option<String>,
    /// When it was first drawn, as written (FORMAT.md §2.4).
    pub at: Option<String>,
    /// When it was last changed, if it has been since.
    pub modified: Option<String>,
    /// Its `PencilKit` data, as a full path.
    pub data_path: String,
    /// Its PNG picture, as a full path.
    pub picture_path: String,
}

/// What saving or removing a drawing did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CardDrawingChange {
    /// The drawing's identifier: a new drawing's is made when it's saved.
    pub id: String,
    /// Whether anything changed.
    pub changed: bool,
    /// The change, as History shows it.
    pub summary: String,
    /// The history commit, when one was made.
    pub commit: Option<String>,
}

impl From<carrel_core::drawings::DrawingChange> for CardDrawingChange {
    fn from(done: carrel_core::drawings::DrawingChange) -> Self {
        Self {
            id: done.id.to_string(),
            changed: done.changed,
            summary: done.summary,
            commit: done.commit,
        }
    }
}

fn drawing_id(id: &str) -> Result<Id, CarrelError> {
    id.trim()
        .parse()
        .map(Id)
        .map_err(|_| CarrelError::invalid(format!("{id} isn't a drawing's identifier")))
}

#[uniffi::export]
impl ArchiveHandle {
    /// The handwritten notes on the item `item`'s card, oldest first.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] for an unknown item or a card that can't be read.
    pub fn card_drawings(&self, item: String) -> Result<Vec<CardDrawingInfo>, CarrelError> {
        let archive = self.fresh()?;
        let item = Self::item(&archive, &item)?;
        let root = archive.root();
        Ok(carrel_core::drawings::card_drawings(&archive, &item)?
            .into_iter()
            .map(
                |ListedDrawing {
                     drawing,
                     data,
                     picture,
                 }| CardDrawingInfo {
                    id: drawing.id.to_string(),
                    text: drawing.text,
                    at: drawing.at.map(|at| at.0),
                    modified: drawing.modified.map(|at| at.0),
                    data_path: root.join(data).to_string_lossy().into_owned(),
                    picture_path: root.join(picture).to_string_lossy().into_owned(),
                },
            )
            .collect())
    }

    /// Saves a handwritten note on the item `item`'s card: `data` is the `PKDrawing`'s data
    /// representation, `picture` a PNG of it, and `text` what was recognised in it. A new
    /// drawing when `drawing` is `None`, else a new version of that drawing.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] for an unknown item or drawing, an empty drawing, a picture
    /// that isn't a PNG, or an item that's checked out; [`CarrelError::Busy`] if another
    /// writer holds the archive; or [`CarrelError::Failed`] if it can't be written.
    pub fn save_card_drawing(
        &self,
        item: String,
        drawing: Option<String>,
        data: Vec<u8>,
        picture: Vec<u8>,
        text: String,
    ) -> Result<CardDrawingChange, CarrelError> {
        let archive = self.fresh()?;
        let item = Self::item(&archive, &item)?;
        let id = drawing.as_deref().map(drawing_id).transpose()?;
        let to_save = DrawingToSave {
            data: &data,
            picture: &picture,
            text: &text,
        };
        Ok(
            carrel_core::drawings::save_card_drawing(
                &archive,
                &item,
                id,
                &to_save,
                self.holder()?,
            )?
            .into(),
        )
    }

    /// Removes the handwritten note `drawing` from the item `item`'s card.
    ///
    /// # Errors
    ///
    /// As [`ArchiveHandle::save_card_drawing`].
    pub fn remove_card_drawing(
        &self,
        item: String,
        drawing: String,
    ) -> Result<CardDrawingChange, CarrelError> {
        let archive = self.fresh()?;
        let item = Self::item(&archive, &item)?;
        Ok(carrel_core::drawings::remove_card_drawing(
            &archive,
            &item,
            drawing_id(&drawing)?,
            self.holder()?,
        )?
        .into())
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::Sample;

    #[test]
    fn a_drawing_is_saved_found_by_its_text_and_removed() {
        let sample = Sample::new();
        let item = sample.handle.items().unwrap()[0].id.clone();
        let saved = sample
            .handle
            .save_card_drawing(
                item.clone(),
                None,
                b"strokes".to_vec(),
                b"\x89PNG\r\n\x1a\npicture".to_vec(),
                "zanzibar marmalade".into(),
            )
            .unwrap();
        assert!(saved.changed);
        let drawings = sample.handle.card_drawings(item.clone()).unwrap();
        assert_eq!(drawings.len(), 1);
        assert!(std::path::Path::new(&drawings[0].picture_path).is_file());

        let found = sample.handle.search("zanzibar".into(), 5, None).unwrap();
        assert!(
            found.hits.iter().any(|h| h.item_id.as_ref() == Some(&item)),
            "{found:?}"
        );

        let removed = sample
            .handle
            .remove_card_drawing(item.clone(), saved.id)
            .unwrap();
        assert!(removed.changed);
        assert_eq!(sample.handle.card_drawings(item).unwrap(), Vec::new());
    }
}
