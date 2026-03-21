use humu::pty::terminal::{Color, Parser};

#[test]
fn parser_preserves_dim_hidden_and_strike_attributes() {
    let mut parser = Parser::new(24, 80, 0);
    parser.process(b"\x1b[2mD\x1b[8mH\x1b[9mS");

    let d = parser.screen().cell(0, 0).unwrap();
    assert!(d.dim());

    let h = parser.screen().cell(0, 1).unwrap();
    assert!(h.dim());
    assert!(h.hidden());

    let s = parser.screen().cell(0, 2).unwrap();
    assert!(s.dim());
    assert!(s.hidden());
    assert!(s.strike());
}

#[test]
fn parser_resets_bold_and_dim_with_normal_intensity() {
    let mut parser = Parser::new(24, 80, 0);
    parser.process(b"\x1b[1;2mA\x1b[22mB");

    let a = parser.screen().cell(0, 0).unwrap();
    assert!(a.bold());
    assert!(a.dim());

    let b = parser.screen().cell(0, 1).unwrap();
    assert!(!b.bold());
    assert!(!b.dim());
}

#[test]
fn parser_preserves_truecolor_background_with_colon_colorspace_form() {
    let mut parser = Parser::new(24, 80, 0);
    parser.process(b"\x1b[48:2::60:60:60m \x1b[0m");

    let cell = parser.screen().cell(0, 0).unwrap();
    assert_eq!(cell.bgcolor(), Color::Rgb(60, 60, 60));
}

#[test]
fn parser_preserves_truecolor_foreground_with_colon_colorspace_form() {
    let mut parser = Parser::new(24, 80, 0);
    parser.process(b"\x1b[38:2::12:34:56mX\x1b[0m");

    let cell = parser.screen().cell(0, 0).unwrap();
    assert_eq!(cell.fgcolor(), Color::Rgb(12, 34, 56));
}
