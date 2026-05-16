use pretty_assertions::assert_eq;
use report::titler::page::{parse_page, Layer, Page, Position, Rgba};

#[test]
fn parses_minimal_text_page() {
    let toml = r##"
schema = 1
name = "test_page"

[render]
canvas = { w = 1920, h = 1080 }

[[layer]]
kind = "text"
text = "TEST TITLE"
size = 64.0
color = "#FFFFFF"
position = { x = 100, y = 900 }
"##;

    let page = parse_page(toml).expect("parse");

    assert_eq!(page.schema, 1);
    assert_eq!(page.name, "test_page");
    assert_eq!(page.canvas, (1920, 1080));
    assert_eq!(page.layers.len(), 1);

    match &page.layers[0] {
        Layer::Text(t) => {
            assert_eq!(t.text, "TEST TITLE");
            #[allow(clippy::float_cmp)]
            {
                assert_eq!(t.size, 64.0);
            }
            assert_eq!(t.color, Rgba(0xFF, 0xFF, 0xFF, 0xFF));
            assert_eq!(t.position, Position { x: 100, y: 900 });
        }
    }
}

#[test]
fn rejects_wrong_schema_version() {
    let toml = r#"
schema = 99
name = "future_page"
[render]
canvas = { w = 1920, h = 1080 }
"#;
    let err = parse_page(toml).unwrap_err();
    assert!(format!("{err}").contains("schema"));
}

#[test]
fn rejects_missing_required_fields() {
    let toml = r#"
schema = 1
"#;
    parse_page(toml).unwrap_err();
}

#[test]
fn parses_color_with_alpha() {
    let toml = r##"
schema = 1
name = "p"
[render]
canvas = { w = 1920, h = 1080 }

[[layer]]
kind = "text"
text = "X"
size = 10.0
color = "#80402040"
position = { x = 0, y = 0 }
"##;
    let page = parse_page(toml).expect("parse");
    #[allow(irrefutable_let_patterns)]
    let Layer::Text(t) = &page.layers[0];
    assert_eq!(t.color, Rgba(0x80, 0x40, 0x20, 0x40));
}

// `Page` is part of the public API; reference it so the import doesn't warn.
#[allow(dead_code)]
fn _ensure_page_type(_p: Page) {}
