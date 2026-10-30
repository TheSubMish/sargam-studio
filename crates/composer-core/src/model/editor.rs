use super::{Clip, PatternNote, Song, Track};

const MAX_UNDO: usize = 100;

/// Undo/redo wrapper around a [`Song`].
///
/// Snapshot-based: every edit clones the song onto the undo stack. Songs are
/// small (notes, not audio), so this stays cheap while making *every* edit —
/// including future UI gestures — trivially undoable. The typed helpers below
/// cover common operations; arbitrary edits go through [`SongEditor::edit`].
pub struct SongEditor {
    song: Song,
    undo_stack: Vec<Song>,
    redo_stack: Vec<Song>,
}

impl SongEditor {
    pub fn new(song: Song) -> Self {
        Self {
            song,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn song(&self) -> &Song {
        &self.song
    }

    /// Apply any mutation as one undoable step.
    pub fn edit(&mut self, mutate: impl FnOnce(&mut Song)) {
        self.undo_stack.push(self.song.clone());
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        mutate(&mut self.song);
    }

    pub fn undo(&mut self) -> bool {
        match self.undo_stack.pop() {
            Some(prev) => {
                self.redo_stack
                    .push(std::mem::replace(&mut self.song, prev));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo_stack.pop() {
            Some(next) => {
                self.undo_stack
                    .push(std::mem::replace(&mut self.song, next));
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    // ---- typed conveniences ------------------------------------------------

    pub fn add_track(&mut self, track: Track) {
        self.edit(|s| s.tracks.push(track));
    }

    pub fn add_note(&mut self, track: usize, pattern: usize, note: PatternNote) {
        self.edit(|s| {
            if let Some(p) = s
                .tracks
                .get_mut(track)
                .and_then(|t| t.patterns.get_mut(pattern))
            {
                p.notes.push(note);
                p.notes.sort_by_key(|n| n.start);
            }
        });
    }

    /// Remove the note at `note_index` (in pattern order).
    pub fn remove_note(&mut self, track: usize, pattern: usize, note_index: usize) {
        self.edit(|s| {
            if let Some(p) = s
                .tracks
                .get_mut(track)
                .and_then(|t| t.patterns.get_mut(pattern))
            {
                if note_index < p.notes.len() {
                    p.notes.remove(note_index);
                }
            }
        });
    }

    pub fn add_clip(&mut self, track: usize, clip: Clip) {
        self.edit(|s| {
            if let Some(t) = s.tracks.get_mut(track) {
                t.clips.push(clip);
                t.clips.sort_by_key(|c| c.start);
            }
        });
    }

    pub fn set_tempo(&mut self, tick: super::Ticks, bpm: f32) {
        self.edit(|s| s.tempo.set_change(tick, bpm));
    }
}
