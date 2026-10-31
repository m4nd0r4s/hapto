//! Render a 2.0 figure document to a PNG, to look at it.
//!
//! ```bash
//! cargo run -p hapto-core --example render -- figure.yaml out.png [dark]
//! ```
//!
//! The `hapto render` command (SPEC-2.0 §17) replaces this once the CLI
//! exists; until then this is how a new mark gets looked at.

use hapto_core::render::Figure;
use hapto_core::scene::Canvas;
use hapto_core::theme::Theme;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: render <figure.yaml> <out.png> [dark]");
        std::process::exit(2);
    };
    let theme = Theme::by_name(args.get(3).map(String::as_str).unwrap_or("light"));
    let text = std::fs::read_to_string(input).expect("read the figure");
    let mut figure = Figure::from_text(&text).unwrap_or_else(|errors| {
        for e in errors {
            eprintln!("{}", e.render());
        }
        std::process::exit(1);
    });
    let image = figure
        .render(Canvas::new(720.0, 405.0, 2.0), &theme)
        .unwrap_or_else(|e| {
            eprintln!("{}", e.render());
            std::process::exit(1);
        });
    std::fs::write(output, kinemo_render::raster::encode_png(&image)).expect("write the png");
}
