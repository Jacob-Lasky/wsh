use avt::{Line, Pen};

use super::state::{Color, FormattedLine, Span, Style};

/// Convert an avt Line to a FormattedLine based on format
pub fn format_line(line: &Line, styled: bool) -> FormattedLine {
    if styled {
        FormattedLine::Styled(line_to_spans(line))
    } else {
        FormattedLine::Plain(line.text())
    }
}

/// Convert an avt Line to styled spans
fn line_to_spans(line: &Line) -> Vec<Span> {
    let cells = line.cells();
    if cells.is_empty() {
        return vec![];
    }

    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<Style> = None;

    for cell in cells {
        let ch = cell.char();
        if ch == '\0' || cell.width() == 0 {
            continue;
        }

        let style = pen_to_style(cell.pen());

        match &current_style {
            None => {
                current_style = Some(style);
                current_text.push(ch);
            }
            Some(s) if *s == style => {
                current_text.push(ch);
            }
            Some(_) => {
                // Style changed, emit current span
                if !current_text.is_empty() {
                    spans.push(Span {
                        text: std::mem::take(&mut current_text),
                        style: current_style.take().unwrap(),
                    });
                }
                current_style = Some(style);
                current_text.push(ch);
            }
        }
    }

    // Emit final span
    if !current_text.is_empty() {
        if let Some(style) = current_style {
            spans.push(Span {
                text: current_text,
                style,
            });
        }
    }

    spans
}

fn pen_to_style(pen: &Pen) -> Style {
    Style {
        fg: pen.foreground().map(color_to_color),
        bg: pen.background().map(color_to_color),
        bold: pen.is_bold(),
        faint: pen.is_faint(),
        italic: pen.is_italic(),
        underline: pen.is_underline(),
        strikethrough: pen.is_strikethrough(),
        blink: pen.is_blink(),
        inverse: pen.is_inverse(),
    }
}

fn color_to_color(c: avt::Color) -> Color {
    match c {
        avt::Color::Indexed(i) => Color::Indexed(i),
        avt::Color::RGB(rgb) => Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
    }
}
