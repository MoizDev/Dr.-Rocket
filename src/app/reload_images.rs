//! Merge incoming WXME images without reusing identifiers referenced by undo history.
use crate::editor::images::{EmbeddedImage, FIRST, ImageStore, LAST, is_image_char};
use crate::racket::protocol::SourceImages;
use std::collections::HashMap;

pub fn merge(store: &ImageStore, body: &str, incoming: &SourceImages) -> Result<(String, ImageStore), String> {
    let mut store = store.clone();
    let mut remap = HashMap::new();
    for (code, data) in incoming {
        let code = code
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .filter(|c| is_image_char(*c))
            .ok_or("Invalid image identifier in reloaded file")?;
        let image = EmbeddedImage::from_b64(data).ok_or("Invalid image in reloaded file")?;
        let destination = match store.get(code) {
            Some(previous) if previous.png == image.png => code,
            _ => {
                if !(FIRST..=LAST).any(|c| store.get(char::from_u32(c).unwrap()).is_none()) {
                    return Err("Image identifier space is full; reopen the file to release undo history".into());
                }
                store.add(image)
            }
        };
        remap.insert(code, destination);
    }
    // A literal reserved character must not accidentally acquire an image from old history.
    if body
        .chars()
        .any(|c| is_image_char(c) && !remap.contains_key(&c) && store.get(c).is_some())
    {
        return Err(
            "Reload contains a reserved image character without image data; reopen the file to load it safely".into(),
        );
    }
    Ok((
        body.chars().map(|c| remap.get(&c).copied().unwrap_or(c)).collect(),
        store,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reload_does_not_replace_images_retained_for_undo() {
        let image =
            |red| EmbeddedImage::from_rgba(image::RgbaImage::from_pixel(2, 2, image::Rgba([red, 0, 0, 255]))).unwrap();
        let mut old = ImageStore::default();
        let previous = image(1);
        let next = image(2);
        let code = old.add(previous.clone());
        let incoming = SourceImages::from([((code as u32).to_string(), next.b64.to_string())]);
        let (body, merged) = merge(&old, &code.to_string(), &incoming).unwrap();
        let new_code = body.chars().next().unwrap();
        assert_ne!(code, new_code);
        assert_eq!(merged.get(code).unwrap().png, previous.png);
        assert_eq!(merged.get(new_code).unwrap().png, next.png);
        assert!(merge(&old, &code.to_string(), &SourceImages::new()).is_err());
    }
}
