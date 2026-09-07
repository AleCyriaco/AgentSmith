use crate::{model::Snapshot, remote::Action};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::io::Cursor;

pub fn model_frame(frame: &Snapshot, max_width: u32) -> Result<Snapshot, String> {
    if max_width == 0 || frame.width <= max_width {
        return Ok(frame.clone());
    }
    let bytes = STANDARD
        .decode(
            frame
                .data_url
                .strip_prefix("data:image/png;base64,")
                .ok_or("Captura inválida.")?,
        )
        .map_err(|_| "Captura inválida.")?;
    let source =
        image::load_from_memory(&bytes).map_err(|_| "Não foi possível preparar a imagem da IA.")?;
    let scaled = source.resize(
        max_width,
        frame.height,
        image::imageops::FilterType::Triangle,
    );
    let mut png = Cursor::new(Vec::new());
    scaled
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "Não foi possível reduzir a captura.")?;
    Ok(Snapshot {
        width: scaled.width(),
        height: scaled.height(),
        data_url: format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png.into_inner())
        ),
        ..frame.clone()
    })
}

pub fn original_action(
    action: Action,
    sent: &Snapshot,
    original: &Snapshot,
) -> Result<Action, String> {
    let point = |x: u32, y: u32| {
        if sent.width == 0 || sent.height == 0 || x >= sent.width || y >= sent.height {
            return Err("O clique ficou fora da imagem enviada à IA.".to_string());
        }
        Ok((
            (x as u64 * original.width as u64 / sent.width as u64) as u32,
            (y as u64 * original.height as u64 / sent.height as u64) as u32,
        ))
    };
    Ok(match action {
        Action::Click { x, y } => {
            let (x, y) = point(x, y)?;
            Action::Click { x, y }
        }
        Action::DoubleClick { x, y } => {
            let (x, y) = point(x, y)?;
            Action::DoubleClick { x, y }
        }
        Action::RightClick { x, y } => {
            let (x, y) = point(x, y)?;
            Action::RightClick { x, y }
        }
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reduced_capture_preserves_aspect_and_remaps_clicks() {
        let source = image::DynamicImage::new_rgb8(1920, 1080);
        let mut png = Cursor::new(vec![]);
        source.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let original = Snapshot {
            width: 1920,
            height: 1080,
            data_url: format!(
                "data:image/png;base64,{}",
                STANDARD.encode(png.into_inner())
            ),
            sequence: 7,
            captured_at: 42,
        };
        let sent = model_frame(&original, 1280).unwrap();
        assert_eq!((sent.width, sent.height, sent.sequence), (1280, 720, 7));
        match original_action(Action::DoubleClick { x: 640, y: 360 }, &sent, &original).unwrap() {
            Action::DoubleClick { x, y } => assert_eq!((x, y), (960, 540)),
            _ => panic!("tipo de clique alterado"),
        }
        assert!(original_action(Action::Click { x: 1280, y: 100 }, &sent, &original).is_err());
        assert_eq!(
            model_frame(&original, 2560).unwrap().data_url,
            original.data_url
        );
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl Region {
    pub fn full(frame: &Snapshot) -> Self {
        Self {
            x: 0,
            y: 0,
            width: frame.width,
            height: frame.height,
        }
    }
    pub fn validate(&self, w: u32, h: u32) -> Result<(), String> {
        if self.width < 8
            || self.height < 8
            || self.x.checked_add(self.width).is_none_or(|v| v > w)
            || self.y.checked_add(self.height).is_none_or(|v| v > h)
        {
            return Err("Selecione uma região válida dentro da tela.".into());
        }
        Ok(())
    }
}
pub fn crop(frame: &Snapshot, region: Region) -> Result<Snapshot, String> {
    region.validate(frame.width, frame.height)?;
    if region == Region::full(frame) {
        return Ok(frame.clone());
    }
    let source = decode(frame)?;
    let cropped = source.crop_imm(region.x, region.y, region.width, region.height);
    let mut png = Cursor::new(Vec::new());
    cropped
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "Falha ao recortar a imagem.")?;
    Ok(Snapshot {
        width: region.width,
        height: region.height,
        data_url: format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png.into_inner())
        ),
        ..frame.clone()
    })
}
fn decode(frame: &Snapshot) -> Result<image::DynamicImage, String> {
    let bytes = STANDARD
        .decode(
            frame
                .data_url
                .strip_prefix("data:image/png;base64,")
                .ok_or("Captura inválida.")?,
        )
        .map_err(|_| "Captura inválida.")?;
    let image = image::load_from_memory(&bytes).map_err(|_| "Captura inválida.")?;
    if image.width() != frame.width || image.height() != frame.height {
        return Err("Dimensões da captura inválidas.".into());
    }
    Ok(image)
}
pub struct Prepared {
    pub frame: Snapshot,
    pub region: Region,
}
pub fn prepare(
    frame: &Snapshot,
    max_width: u32,
    region: Option<Region>,
) -> Result<Prepared, String> {
    let region = region.unwrap_or_else(|| Region::full(frame));
    Ok(Prepared {
        frame: model_frame(&crop(frame, region)?, max_width)?,
        region,
    })
}
impl Prepared {
    pub fn action(&self, action: Action, original: &Snapshot) -> Result<Action, String> {
        self.region.validate(original.width, original.height)?;
        let local = Snapshot {
            width: self.region.width,
            height: self.region.height,
            ..original.clone()
        };
        let point = |x: u32, y: u32| -> Result<(u32, u32), String> {
            match original_action(Action::Click { x, y }, &self.frame, &local)? {
                Action::Click { x, y } => Ok((x + self.region.x, y + self.region.y)),
                _ => unreachable!(),
            }
        };
        Ok(match action {
            Action::Click { x, y } => {
                let (x, y) = point(x, y)?;
                Action::Click { x, y }
            }
            Action::DoubleClick { x, y } => {
                let (x, y) = point(x, y)?;
                Action::DoubleClick { x, y }
            }
            Action::RightClick { x, y } => {
                let (x, y) = point(x, y)?;
                Action::RightClick { x, y }
            }
            Action::Inspect {
                x,
                y,
                width,
                height,
            } => {
                Region {
                    x,
                    y,
                    width,
                    height,
                }
                .validate(self.frame.width, self.frame.height)?;
                let (left, top) = point(x, y)?;
                let right = self.region.x
                    + ((x + width) as u64 * self.region.width as u64)
                        .div_ceil(self.frame.width as u64) as u32;
                let bottom = self.region.y
                    + ((y + height) as u64 * self.region.height as u64)
                        .div_ceil(self.frame.height as u64) as u32;
                Action::Inspect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                }
            }
            other => other,
        })
    }
}
pub fn region_unchanged(a: &Snapshot, b: &Snapshot, region: Region) -> bool {
    if a.width != b.width || a.height != b.height {
        return false;
    }
    if a.data_url == b.data_url {
        return true;
    }
    let (Ok(a), Ok(b)) = (
        crop(a, region).and_then(|f| decode(&f)),
        crop(b, region).and_then(|f| decode(&f)),
    ) else {
        return false;
    };
    a.to_rgb8() == b.to_rgb8()
}

#[cfg(test)]
mod crop_tests {
    use super::*;
    fn snapshot(image: image::RgbImage) -> Snapshot {
        let (width, height) = image.dimensions();
        let mut out = Cursor::new(vec![]);
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        Snapshot {
            width,
            height,
            data_url: format!(
                "data:image/png;base64,{}",
                STANDARD.encode(out.into_inner())
            ),
            sequence: 1,
            captured_at: 1,
        }
    }
    #[test]
    fn cropped_scaled_coordinates_map_to_original_and_reject_outside() {
        let frame = snapshot(image::RgbImage::from_pixel(
            800,
            600,
            image::Rgb([10, 20, 30]),
        ));
        let region = Region {
            x: 200,
            y: 100,
            width: 400,
            height: 200,
        };
        let sent = prepare(&frame, 200, Some(region)).unwrap();
        assert_eq!((sent.frame.width, sent.frame.height), (200, 100));
        for action in [
            Action::Click { x: 100, y: 50 },
            Action::DoubleClick { x: 100, y: 50 },
            Action::RightClick { x: 100, y: 50 },
        ] {
            match sent.action(action, &frame).unwrap() {
                Action::Click { x, y }
                | Action::DoubleClick { x, y }
                | Action::RightClick { x, y } => assert_eq!((x, y), (400, 200)),
                _ => panic!(),
            }
        }
        assert!(sent.action(Action::Click { x: 200, y: 0 }, &frame).is_err());
        assert!(sent
            .action(
                Action::Inspect {
                    x: 195,
                    y: 0,
                    width: 10,
                    height: 10
                },
                &frame
            )
            .is_err());
        assert!(Region {
            x: u32::MAX,
            y: 0,
            width: 20,
            height: 20
        }
        .validate(800, 600)
        .is_err());
        match sent
            .action(
                Action::Inspect {
                    x: 10,
                    y: 20,
                    width: 50,
                    height: 30,
                },
                &frame,
            )
            .unwrap()
        {
            Action::Inspect {
                x,
                y,
                width,
                height,
            } => assert_eq!((x, y, width, height), (220, 140, 100, 60)),
            _ => panic!(),
        }
        assert_eq!(
            decode(&sent.frame).unwrap().to_rgb8().get_pixel(1, 1).0,
            [10, 20, 30]
        );
    }
    #[test]
    fn freshness_compares_only_selected_pixels_and_detects_resolution_change() {
        let mut pixels = image::RgbImage::from_pixel(100, 100, image::Rgb([0, 0, 0]));
        let a = snapshot(pixels.clone());
        let r = Region {
            x: 10,
            y: 10,
            width: 20,
            height: 20,
        };
        pixels.put_pixel(90, 90, image::Rgb([255, 0, 0]));
        let b = snapshot(pixels.clone());
        assert!(region_unchanged(&a, &b, r));
        pixels.put_pixel(15, 15, image::Rgb([255, 0, 0]));
        let c = snapshot(pixels);
        assert!(!region_unchanged(&a, &c, r));
        let mut resized = a.clone();
        resized.width = 200;
        assert!(!region_unchanged(&a, &resized, r));
    }
}
