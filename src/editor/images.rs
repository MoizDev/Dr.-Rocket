//! Images embedded in program text (DrRacket's pasted images).
//!
//! Each image is one character from Supplementary Private Use Area-A in the buffer, so undo,
//! copy/paste within a file and deletion need no bookkeeping; the document maps the
//! character to the image. The backends receive the images as [`SourceImages`] and read the
//! text as if the images were there.

use crate::racket::protocol::SourceImages;
use base64::Engine;
use std::collections::HashMap;
use std::sync::Arc;

/// Images in the most recent copy or cut, for pasting into another document.
static CLIPBOARD: std::sync::LazyLock<std::sync::Mutex<HashMap<char, EmbeddedImage>>> = std::sync::LazyLock::new(Default::default);

pub const FIRST: u32 = 0xF0000;
pub const LAST: u32 = 0xFFFFD;

pub fn is_image_char(c: char) -> bool {
    (FIRST..=LAST).contains(&(c as u32))
}

#[derive(Clone, Debug)]
pub struct EmbeddedImage {
    pub png: Arc<[u8]>,
    pub b64: Arc<str>,
    pub width: u32,
    pub height: u32,
}

impl EmbeddedImage {
    pub fn from_png(png: Vec<u8>) -> Option<EmbeddedImage> {
        // PNG header: 8-byte signature, then the IHDR chunk with width and height.
        if png.len() < 24 || &png[1..4] != b"PNG" {
            return None;
        }
        let width = u32::from_be_bytes(png[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(png[20..24].try_into().ok()?);
        let b64: Arc<str> = base64::engine::general_purpose::STANDARD.encode(&png).into();
        Some(EmbeddedImage { png: png.into(), b64, width, height })
    }

    pub fn from_b64(b64: &str) -> Option<EmbeddedImage> {
        let png = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
        Self::from_png(png)
    }

    /// Any image file the `image` crate can decode, re-encoded as PNG.
    pub fn from_file_bytes(bytes: &[u8]) -> Option<EmbeddedImage> {
        if bytes.starts_with(b"\x89PNG") {
            return Self::from_png(bytes.to_vec());
        }
        let img = image::load_from_memory(bytes).ok()?;
        Self::from_rgba(img.to_rgba8())
    }

    pub fn from_rgba(img: image::RgbaImage) -> Option<EmbeddedImage> {
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
        Self::from_png(png)
    }
}

/// The images of one document.
#[derive(Clone, Debug, Default)]
pub struct ImageStore {
    images: HashMap<char, EmbeddedImage>,
}

impl ImageStore {
    pub fn get(&self, c: char) -> Option<&EmbeddedImage> {
        self.images.get(&c)
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Register an image and return the character that stands for it.
    pub fn add(&mut self, image: EmbeddedImage) -> char {
        let code = (FIRST..=LAST).find(|c| char::from_u32(*c).is_some_and(|ch| !self.images.contains_key(&ch))).unwrap_or(LAST);
        let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
        self.images.insert(ch, image);
        ch
    }

    /// Replace the store with images keyed by code point (from a WXME read).
    pub fn load(&mut self, images: &SourceImages) {
        self.images.clear();
        for (code, b64) in images {
            if let (Some(ch), Some(img)) = (code.parse::<u32>().ok().and_then(char::from_u32), EmbeddedImage::from_b64(b64)) {
                self.images.insert(ch, img);
            }
        }
    }

    /// Remember the images in copied text, so pasting into another document brings them.
    pub fn shelve_for_clipboard(&self, text: &str) {
        let used: HashMap<char, EmbeddedImage> = text.chars().filter(|c| is_image_char(*c)).filter_map(|c| self.images.get(&c).map(|i| (c, i.clone()))).collect();
        if !used.is_empty()
            && let Ok(mut shelf) = CLIPBOARD.lock()
        {
            *shelf = used;
        }
    }

    /// Take in the images of pasted text: characters this document doesn't know are looked up
    /// on the clipboard shelf, and renumbered if the character already means another image.
    pub fn adopt_from_clipboard(&mut self, text: &str) -> String {
        if !text.chars().any(is_image_char) {
            return text.to_string();
        }
        let shelf = CLIPBOARD.lock().map(|s| s.clone()).unwrap_or_default();
        let mut renamed: HashMap<char, char> = HashMap::new();
        text.chars()
            .map(|c| {
                if !is_image_char(c) {
                    return c;
                }
                if let Some(&r) = renamed.get(&c) {
                    return r;
                }
                let Some(img) = shelf.get(&c) else { return c };
                let mine = match self.images.get(&c) {
                    Some(existing) if std::sync::Arc::ptr_eq(&existing.png, &img.png) || existing.png == img.png => c,
                    Some(_) => self.add(img.clone()),
                    None => {
                        self.images.insert(c, img.clone());
                        c
                    }
                };
                renamed.insert(c, mine);
                mine
            })
            .collect()
    }

    /// The images that `text` uses, for sending to the backends.
    pub fn for_text(&self, text: &str) -> SourceImages {
        if self.images.is_empty() {
            return SourceImages::new();
        }
        text.chars()
            .filter(|c| is_image_char(*c))
            .filter_map(|c| self.images.get(&c).map(|img| ((c as u32).to_string(), img.b64.to_string())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, c: u8) -> EmbeddedImage {
        EmbeddedImage::from_rgba(image::RgbaImage::from_pixel(w, h, image::Rgba([c, 0, 0, 255]))).unwrap()
    }

    #[test]
    fn images_are_characters_with_sizes() {
        let mut store = ImageStore::default();
        let a = store.add(png(4, 2, 1));
        let b = store.add(png(1, 1, 2));
        assert_ne!(a, b);
        assert!(is_image_char(a) && is_image_char(b));
        assert_eq!((store.get(a).unwrap().width, store.get(a).unwrap().height), (4, 2));
        // Only images still in the text go to the backends.
        let sent = store.for_text(&format!("(beside {a} {a})"));
        assert_eq!(sent.keys().collect::<Vec<_>>(), vec![&(a as u32).to_string()]);
    }

    #[test]
    fn pasting_into_another_document_brings_the_images() {
        let mut from = ImageStore::default();
        let a = from.add(png(2, 2, 7));
        from.shelve_for_clipboard(&format!("(define x {a})"));
        // The other document already uses that character for a different image.
        let mut to = ImageStore::default();
        let taken = to.add(png(5, 5, 9));
        assert_eq!(taken, a);
        let pasted = to.adopt_from_clipboard(&format!("(list {a} {a})"));
        let c = pasted.chars().find(|c| is_image_char(*c)).unwrap();
        assert_ne!(c, a, "renumbered");
        assert_eq!(pasted, format!("(list {c} {c})"));
        assert_eq!(to.get(c).unwrap().width, 2);
        assert_eq!(to.get(a).unwrap().width, 5, "the existing image is untouched");
    }
}
