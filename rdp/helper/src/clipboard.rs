//! The text clipboard of a session (`CLIPRDR`, ADR 0034). Only Unicode text goes either way.
//!
//! IronRDP calls the backend while it processes the channel, when it can't send anything itself:
//! the backend only notes what happened in a channel the session reads, and the session answers
//! (offering our text, fetching the server's). Clipboard text is never logged.

use ironrdp::cliprdr::backend::CliprdrBackend;
use ironrdp::cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest,
    FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
};
use tokio::sync::mpsc::UnboundedSender;

/// What the session must do for the clipboard.
#[derive(Debug)]
pub(crate) enum ClipboardEvent {
    /// The channel is ready, or the server asks for our formats: offer our text, if any.
    Offer,
    /// The server has text: fetch it.
    Fetch,
    /// The server wants our text.
    Send,
    /// The server's text arrived.
    Received(String),
}

impl ClipboardEvent {
    /// Its name, for logs (never the text).
    fn name(&self) -> &'static str {
        match self {
            Self::Offer => "offer",
            Self::Fetch => "fetch",
            Self::Send => "send",
            Self::Received(_) => "received",
        }
    }
}

/// The session's clipboard backend.
#[derive(Debug)]
pub(crate) struct Backend {
    events: UnboundedSender<ClipboardEvent>,
}

impl Backend {
    pub(crate) fn new(events: UnboundedSender<ClipboardEvent>) -> Self {
        Self { events }
    }

    fn send(&self, event: ClipboardEvent) {
        tracing::debug!(event = event.name(), "clipboard");
        // The session may be ending; nothing to do then.
        let _ = self.events.send(event);
    }
}

impl ironrdp::core::AsAny for Backend {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl CliprdrBackend for Backend {
    fn temporary_directory(&self) -> &str {
        // No files are transferred.
        ""
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::empty()
    }

    fn on_ready(&mut self) {
        self.send(ClipboardEvent::Offer);
    }

    fn on_request_format_list(&mut self) {
        self.send(ClipboardEvent::Offer);
    }

    fn on_process_negotiated_capabilities(
        &mut self,
        _capabilities: ClipboardGeneralCapabilityFlags,
    ) {
    }

    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        tracing::debug!(formats = available_formats.len(), "the server copied");
        if available_formats
            .iter()
            .any(|format| format.id == ClipboardFormatId::CF_UNICODETEXT)
        {
            self.send(ClipboardEvent::Fetch);
        }
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        if request.format == ClipboardFormatId::CF_UNICODETEXT {
            self.send(ClipboardEvent::Send);
        }
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if response.is_error() {
            return;
        }
        if let Ok(text) = response.to_unicode_string() {
            self.send(ClipboardEvent::Received(text));
        }
    }

    fn on_file_contents_request(&mut self, _request: FileContentsRequest) {}

    fn on_file_contents_response(&mut self, _response: FileContentsResponse<'_>) {}

    fn on_lock(&mut self, _data_id: LockDataId) {}

    fn on_unlock(&mut self, _data_id: LockDataId) {}
}

/// Our text, as the format list offers it.
pub(crate) fn text_format() -> ClipboardFormat {
    ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)
}
