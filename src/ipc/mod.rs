pub mod server;

#[derive(Debug)]
pub enum IpcCommand {
    Play,
    Pause,
    TogglePlay,
    Next,
    Prev,
    VolumeUp,
    VolumeDown,
    Status, // returns current track info
}