use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::Sender;
use std::thread;

use crate::ipc::IpcCommand;

pub const SOCKET_PATH: &str = "/tmp/aghani.sock";

pub fn start_ipc(tx: Sender<IpcCommand>) {
    let _ = std::fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind IPC socket");

    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                let tx = tx.clone();
                thread::spawn(move || handle_client(stream, tx));
            }
        }
    });
}

fn handle_client(mut stream: UnixStream, tx: Sender<IpcCommand>) {
    let mut buf = String::new();
    if stream.read_to_string(&mut buf).is_err() {
        return;
    }

    let cmd = match buf.trim() {
        "play" => Some(IpcCommand::Play),
        "pause" => Some(IpcCommand::Pause),
        "toggle" => Some(IpcCommand::TogglePlay),
        "next" => Some(IpcCommand::Next),
        "prev" => Some(IpcCommand::Prev),
        "volume_up" => Some(IpcCommand::VolumeUp),
        "volume_down" => Some(IpcCommand::VolumeDown),
        "status" => Some(IpcCommand::Status),
        _ => None,
    };

    if let Some(cmd) = cmd {
        let _ = tx.send(cmd);
        let _ = stream.write_all(b"OK\n");
    } else {
        let _ = stream.write_all(b"ERR unknown command\n");
    }
}
