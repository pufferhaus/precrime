use pretty_assertions::assert_eq;
use report::titler::page::{parse_page, Rgba};
use report::titler::render::render_page;

#[test]
fn render_produces_canvas_sized_buffer() {
    let toml = r##"
schema = 1
name = "t"
[render]
canvas = { w = 320, h = 240 }
"##;
    let page = parse_page(toml).unwrap();
    let rendered = render_page(&page).expect("render");

    assert_eq!(rendered.width, 320);
    assert_eq!(rendered.height, 240);
    assert_eq!(rendered.rgba.len(), rendered.stride as usize * 240);
}

#[test]
fn render_with_no_layers_is_fully_transparent() {
    let toml = r##"
schema = 1
name = "t"
[render]
canvas = { w = 16, h = 16 }
"##;
    let page = parse_page(toml).unwrap();
    let rendered = render_page(&page).expect("render");

    // ARGB32 layout in Cairo: little-endian 4 bytes per pixel (B, G, R, A).
    // All bytes must be zero on an unwritten surface.
    assert!(
        rendered.rgba.iter().all(|b| *b == 0),
        "expected fully transparent buffer"
    );
}

#[test]
fn text_layer_writes_non_transparent_pixels_near_position() {
    let toml = r##"
schema = 1
name = "t"
[render]
canvas = { w = 256, h = 64 }

[[layer]]
kind = "text"
text = "HELLO"
size = 24.0
color = "#FFFFFF"
position = { x = 10, y = 40 }
"##;
    let page = parse_page(toml).unwrap();
    let rendered = render_page(&page).expect("render");

    let stride = rendered.stride as usize;
    let data = &rendered.rgba;
    // Sample a 40x20 box around the expected baseline (10, 40).
    let mut any_nonzero_alpha = false;
    for y in 20..50 {
        for x in 10..80 {
            let offset = y * stride + x * 4;
            let a = data[offset + 3]; // ARGB32 alpha is highest byte → byte 3 little-endian
            if a > 0 {
                any_nonzero_alpha = true;
                break;
            }
        }
    }
    assert!(any_nonzero_alpha, "expected some text pixels in the sample region");
}

#[test]
fn text_color_round_trips_through_render() {
    // Render a magenta glyph and verify the most opaque sampled pixel decodes
    // back to a roughly-magenta color (R high, G low, B high). Premultiplied
    // alpha is divided out before checking channels.
    let toml = r##"
schema = 1
name = "t"
[render]
canvas = { w = 128, h = 32 }

[[layer]]
kind = "text"
text = "M"
size = 20.0
color = "#FF00FF"
position = { x = 30, y = 24 }
"##;
    let page = parse_page(toml).unwrap();
    let rendered = render_page(&page).expect("render");

    let stride = rendered.stride as usize;
    let data = &rendered.rgba;
    let mut best_alpha = 0u8;
    let mut best_rgba = Rgba(0, 0, 0, 0);
    for y in 0..32 {
        for x in 25..70 {
            let offset = y * stride + x * 4;
            let a = data[offset + 3];
            if a > best_alpha {
                best_alpha = a;
                // ARGB32 little-endian: bytes B G R A, premultiplied.
                let b = data[offset];
                let g = data[offset + 1];
                let r = data[offset + 2];
                let inv = 255.0 / a as f32;
                best_rgba = Rgba(
                    ((r as f32 * inv).min(255.0)) as u8,
                    ((g as f32 * inv).min(255.0)) as u8,
                    ((b as f32 * inv).min(255.0)) as u8,
                    a,
                );
            }
        }
    }
    assert!(best_alpha > 200, "expected an opaque pixel somewhere in the glyph");
    let Rgba(r, g, b, _) = best_rgba;
    assert!(r > 150, "expected high red, got {r}");
    assert!(g < 80, "expected low green, got {g}");
    assert!(b > 150, "expected high blue, got {b}");
}

#[test]
fn rendered_page_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<report::titler::render::RenderedPage>();
}
