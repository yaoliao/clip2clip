use std::io::Error;

use arboard::Clipboard;
use clipboard_master::{CallbackResult, ClipboardHandler};
use log::{debug, error, info};

use crate::transport::{Msg, MsgType};

pub struct ClipboardListener {
    sender: tokio::sync::mpsc::Sender<Msg>,
}

impl ClipboardListener {
    pub fn new(sender: tokio::sync::mpsc::Sender<Msg>) -> Self {
        ClipboardListener { sender }
    }
}

impl ClipboardHandler for ClipboardListener {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        info!("clipboard change ......");
        let mut clip = Clipboard::new().unwrap();
        let data = if let Ok(img) = clip.get_image() {
            Msg {
                mag_type: MsgType::IMG { width: img.width, height: img.height },
                data: Vec::from(img.bytes),
            }
        } else if let Ok(text) = clip.get_text() {
            Msg {
                mag_type: MsgType::TXT,
                data: text.into_bytes(),
            }
        } else {
            return CallbackResult::Next;
        };

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(self.sender.send(data))
            .unwrap();

        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: Error) -> CallbackResult {
        error!("listen on clipboard error: {:?}", error);
        CallbackResult::StopWithError(error)
    }
}
