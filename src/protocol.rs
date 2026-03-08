use std::env;

use base64::Engine;

/// Auto-detect the best image protocol for the current terminal.
///
/// Returns `Kitty` if KITTY_WINDOW_ID is set or if TERM=xterm-ghostty.
/// Otherwise falls back to `Iterm2`.
pub fn auto_detect() -> ImageProtocol {
    // https://sw.kovidgoyal.net/kitty/glossary/#envvar-KITTY_WINDOW_ID
    if env::var("KITTY_WINDOW_ID").is_ok() {
        return ImageProtocol::Kitty;
    }
    // https://ghostty.org/docs/help/terminfo
    if env::var("TERM").is_ok_and(|t| t == "xterm-ghostty") {
        return ImageProtocol::Kitty;
    }
    ImageProtocol::Iterm2
}

/// Terminal inline image protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    /// iTerm2 inline image protocol (OSC 1337).
    Iterm2,
    /// Kitty graphics protocol (APC sequences, 4096-byte chunks).
    Kitty,
}

impl ImageProtocol {
    /// Encode `bytes` (PNG data) as a terminal escape sequence for inline display.
    ///
    /// `cell_width` is the number of terminal columns the image should occupy.
    pub fn encode(&self, bytes: &[u8], cell_width: usize) -> String {
        match self {
            ImageProtocol::Iterm2 => iterm2_encode(bytes, cell_width, 1),
            ImageProtocol::Kitty => kitty_encode(bytes, cell_width, 1),
        }
    }

    /// Emit a Kitty graphics delete-by-row command to clear the given terminal row.
    /// No-op for iTerm2.
    pub fn clear_line(&self, y: u16) {
        match self {
            ImageProtocol::Iterm2 => {}
            ImageProtocol::Kitty => kitty_clear_line(y),
        }
    }
}

fn to_base64_str(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

// https://iterm2.com/documentation-images.html
fn iterm2_encode(bytes: &[u8], cell_width: usize, cell_height: usize) -> String {
    format!(
        "\x1b]1337;File=size={};width={};height={};preserveAspectRatio=0;inline=1:{}\u{0007}",
        bytes.len(),
        cell_width,
        cell_height,
        to_base64_str(bytes)
    )
}

// https://sw.kovidgoyal.net/kitty/graphics-protocol/
fn kitty_encode(bytes: &[u8], cell_width: usize, cell_height: usize) -> String {
    let base64_str = to_base64_str(bytes);
    let chunk_size = 4096;

    let mut s = String::new();

    let chunks = base64_str.as_bytes().chunks(chunk_size);
    let total_chunks = chunks.len();

    s.push_str("\x1b_Ga=d,d=C;\x1b\\");
    for (i, chunk) in chunks.enumerate() {
        s.push_str("\x1b_G");
        if i == 0 {
            s.push_str(&format!("a=T,f=100,c={cell_width},r={cell_height},"));
        }
        if i < total_chunks - 1 {
            s.push_str("m=1;");
        } else {
            s.push_str("m=0;");
        }
        s.push_str(std::str::from_utf8(chunk).unwrap());
        s.push_str("\x1b\\");
    }

    s
}

fn kitty_clear_line(y: u16) {
    let y = y + 1; // 1-based
    print!("\x1b_Ga=d,d=Y,y={y};\x1b\\");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterm2_encode_contains_osc_1337() {
        let bytes = b"fake-png-data";
        let s = iterm2_encode(bytes, 2, 1);
        assert!(s.starts_with("\x1b]1337;File="));
        assert!(s.contains("inline=1:"));
    }

    #[test]
    fn kitty_encode_contains_apc() {
        let bytes = b"fake-png-data";
        let s = kitty_encode(bytes, 2, 1);
        assert!(s.contains("\x1b_G"));
        assert!(s.contains("a=T,f=100,"));
    }

    #[test]
    fn encode_dispatches_by_protocol() {
        let bytes = b"data";
        let iterm = ImageProtocol::Iterm2.encode(bytes, 1);
        let kitty = ImageProtocol::Kitty.encode(bytes, 1);
        assert!(iterm.contains("1337"));
        assert!(kitty.contains("a=T"));
    }
}
