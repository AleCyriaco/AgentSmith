//! Turns the peer's VP8/VP9 stream into the RGBA images the rest of AgentSmith
//! already works with. libvpx decodes; the colour conversion is here so it can
//! be tested without a bitstream.
use crate::rustdesk::session::Codec;
use std::os::raw::{c_int, c_void};

/// Planes as libvpx handed them over, valid until the next call on the decoder.
#[repr(C)]
#[derive(Default)]
struct RawFrame {
    width: c_int,
    height: c_int,
    y: *const u8,
    u: *const u8,
    v: *const u8,
    y_stride: c_int,
    u_stride: c_int,
    v_stride: c_int,
    x_shift: c_int,
    y_shift: c_int,
    full_range: c_int,
}

extern "C" {
    fn agentsmith_vpx_new(vp9: c_int) -> *mut c_void;
    fn agentsmith_vpx_free(decoder: *mut c_void);
    fn agentsmith_vpx_decode(
        decoder: *mut c_void,
        data: *const u8,
        length: usize,
        out: *mut RawFrame,
    ) -> c_int;
}

/// A decoded screen, in the same RGBA layout the RDP transport produces.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One plane, described independently of libvpx so conversion can be tested.
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
}

/// Largest picture accepted from a peer, so a bad header cannot ask for an
/// unbounded allocation.
const MAX_SIDE: u32 = 8192;

pub struct Decoder {
    handle: *mut c_void,
}

// The handle is owned exclusively and never shared between threads at once.
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new(codec: Codec) -> Result<Self, String> {
        let handle = unsafe { agentsmith_vpx_new(matches!(codec, Codec::Vp9) as c_int) };
        if handle.is_null() {
            return Err("Não foi possível iniciar o decodificador de vídeo.".into());
        }
        Ok(Self { handle })
    }

    /// Decodes one encoded frame. `Ok(None)` means the frame carried no
    /// picture, which is normal for a stream still waiting on a key frame.
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<Picture>, String> {
        if data.is_empty() {
            return Ok(None);
        }
        let mut frame = RawFrame::default();
        let decoded =
            unsafe { agentsmith_vpx_decode(self.handle, data.as_ptr(), data.len(), &mut frame) };
        if decoded == 0 {
            return Ok(None);
        }
        let (width, height) = (frame.width.max(0) as u32, frame.height.max(0) as u32);
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err("O par enviou um quadro com dimensões inaceitáveis.".into());
        }
        let (x_shift, y_shift) = (frame.x_shift.clamp(0, 2) as u32, frame.y_shift.clamp(0, 2) as u32);
        let chroma_rows = ((height + (1 << y_shift) - 1) >> y_shift) as usize;
        let plane = |data: *const u8, stride: c_int, rows: usize| -> Result<Plane<'_>, String> {
            let stride = usize::try_from(stride)
                .map_err(|_| "O par enviou um quadro malformado.".to_string())?;
            if data.is_null() || stride == 0 {
                return Err("O par enviou um quadro incompleto.".into());
            }
            Ok(Plane {
                data: unsafe { std::slice::from_raw_parts(data, stride * rows) },
                stride,
            })
        };
        Ok(Some(Picture {
            width,
            height,
            rgba: to_rgba(
                &plane(frame.y, frame.y_stride, height as usize)?,
                &plane(frame.u, frame.u_stride, chroma_rows)?,
                &plane(frame.v, frame.v_stride, chroma_rows)?,
                width,
                height,
                x_shift,
                y_shift,
                frame.full_range != 0,
            ),
        }))
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { agentsmith_vpx_free(self.handle) }
    }
}

/// Converts planar luma/chroma to RGBA. The chroma shifts cover 4:2:0, 4:2:2
/// and 4:4:4 without a separate path for each.
#[allow(clippy::too_many_arguments)]
pub fn to_rgba(
    luma: &Plane,
    blue: &Plane,
    red: &Plane,
    width: u32,
    height: u32,
    x_shift: u32,
    y_shift: u32,
    full_range: bool,
) -> Vec<u8> {
    let mut out = vec![255u8; width as usize * height as usize * 4];
    for y in 0..height as usize {
        let luma_row = y * luma.stride;
        let chroma_row = (y >> y_shift) * blue.stride;
        let red_row = (y >> y_shift) * red.stride;
        for x in 0..width as usize {
            let chroma_column = x >> x_shift;
            let (Some(&value), Some(&u), Some(&v)) = (
                luma.data.get(luma_row + x),
                blue.data.get(chroma_row + chroma_column),
                red.data.get(red_row + chroma_column),
            ) else {
                continue;
            };
            let (u, v) = (u as i32 - 128, v as i32 - 128);
            let (r, g, b) = if full_range {
                let c = value as i32;
                (
                    c + ((359 * v) >> 8),
                    c - ((88 * u + 183 * v) >> 8),
                    c + ((454 * u) >> 8),
                )
            } else {
                let c = 298 * (value as i32 - 16);
                (
                    (c + 409 * v + 128) >> 8,
                    (c - 100 * u - 208 * v + 128) >> 8,
                    (c + 516 * u + 128) >> 8,
                )
            };
            let pixel = (y * width as usize + x) * 4;
            out[pixel] = r.clamp(0, 255) as u8;
            out[pixel + 1] = g.clamp(0, 255) as u8;
            out[pixel + 2] = b.clamp(0, 255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(luma: u8, u: u8, v: u8, full_range: bool) -> (u8, u8, u8) {
        let rgba = to_rgba(
            &Plane { data: &[luma; 4], stride: 2 },
            &Plane { data: &[u], stride: 1 },
            &Plane { data: &[v], stride: 1 },
            2,
            2,
            1,
            1,
            full_range,
        );
        (rgba[0], rgba[1], rgba[2])
    }

    #[test]
    fn studio_and_full_range_grey_and_primaries_land_where_expected() {
        // Studio-range black and white sit at 16 and 235, not 0 and 255.
        assert_eq!(convert(16, 128, 128, false), (0, 0, 0));
        assert_eq!(convert(235, 128, 128, false), (255, 255, 255));
        assert_eq!(convert(0, 128, 128, true), (0, 0, 0));
        assert_eq!(convert(255, 128, 128, true), (255, 255, 255));
        // Chroma pushes its own channel to saturation and clamps instead of
        // wrapping, while the channel it does not carry stays near mid grey.
        let (r, g, b) = convert(128, 255, 128, false);
        assert_eq!(b, 255, "azul deveria saturar");
        assert!((110..=150).contains(&r) && g < r, "vermelho {r} verde {g}");
        let (r, g, b) = convert(128, 128, 255, false);
        assert_eq!(r, 255, "vermelho deveria saturar");
        assert!((110..=150).contains(&b) && g < b, "azul {b} verde {g}");
    }

    #[test]
    fn every_pixel_is_written_opaque_across_chroma_layouts() {
        // 4:2:0, 4:2:2 and 4:4:4 as the peer may send them.
        for (x_shift, y_shift) in [(1u32, 1u32), (1, 0), (0, 0)] {
            let columns = (2 + (1 << x_shift) - 1) >> x_shift;
            let rows = (2 + (1 << y_shift) - 1) >> y_shift;
            let chroma = vec![128u8; (columns * rows) as usize];
            let rgba = to_rgba(
                &Plane { data: &[128; 4], stride: 2 },
                &Plane { data: &chroma, stride: columns as usize },
                &Plane { data: &chroma, stride: columns as usize },
                2,
                2,
                x_shift,
                y_shift,
                false,
            );
            assert_eq!(rgba.len(), 16);
            assert!(rgba.chunks_exact(4).all(|p| p[3] == 255));
            // Neutral chroma over mid luma is grey in every layout.
            assert!(rgba.chunks_exact(4).all(|p| p[0] == p[1] && p[1] == p[2]));
        }
    }

    #[test]
    fn short_planes_do_not_read_out_of_bounds() {
        // A peer whose strides disagree with its dimensions must not panic.
        let rgba = to_rgba(
            &Plane { data: &[200], stride: 8 },
            &Plane { data: &[], stride: 4 },
            &Plane { data: &[], stride: 4 },
            4,
            4,
            1,
            1,
            false,
        );
        assert_eq!(rgba.len(), 64);
        assert!(rgba.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn hostile_bitstreams_are_refused_without_a_picture() {
        let mut decoder = Decoder::new(Codec::Vp9).unwrap();
        assert!(decoder.decode(&[]).unwrap().is_none());
        assert!(decoder.decode(&[0xFF; 64]).unwrap().is_none());
        assert!(decoder.decode(b"nao e um quadro vp9").unwrap().is_none());
        let mut vp8 = Decoder::new(Codec::Vp8).unwrap();
        assert!(vp8.decode(&[0x00; 3]).unwrap().is_none());
    }
}
