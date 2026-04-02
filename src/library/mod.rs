pub mod scanner;
pub mod track;
pub mod playlist;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistScope {
    All,
    Open(usize), // index into app.playlists
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryTab {
    Songs,
    Albums,
    Artists,
    Playlists,
}
// ADD: tracks the current "drill-down" scope
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryScope {
    All,
    Album(String),  // viewing songs inside an album
    Artist(String), // viewing albums by an artist
}
impl LibraryTab {
    pub fn all() -> &'static [LibraryTab] {
        &[
            LibraryTab::Songs,
            LibraryTab::Albums,
            LibraryTab::Artists,
            LibraryTab::Playlists,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            LibraryTab::Songs => "Songs",
            LibraryTab::Albums => "Albums",
            LibraryTab::Artists => "Artists",
            LibraryTab::Playlists => "Playlists",
        }
    }

    pub fn index(&self) -> usize {
        Self::all().iter().position(|t| t == self).unwrap_or(0)
    }
}
