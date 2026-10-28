//! Render each demo at a couple of values, to look at them.
//!
//! ```bash
//! cargo run -p hapto-core --example demo
//! ```
//!
//! Looking is the point. The suite can say a figure drew something and that
//! setting a value changed it; whether the picture *teaches* is a thing only
//! eyes settle, and every defect worth fixing here was found this way.

use hapto_core::{demos, Figure, Size};

fn main() -> Result<(), String> {
    let size = Size {
        width: 960,
        height: 540,
    };

    let cases: Vec<(&str, Figure)> = vec![
        ("spring-rest", demos::mass_spring(0.0, 1.0)),
        ("spring-pulled", demos::mass_spring(2.4, 1.0)),
        ("columns-open", demos::columns(1.0, 3.0)),
        ("columns-flat", demos::columns(3.0, 1.0)),
        ("slope", demos::slope(3.0)),
        ("angle", demos::angle(0.9)),
    ];

    for (name, figure) in &cases {
        let image = figure.draw_now(size)?;
        let path = format!("hapto-{name}.png");
        std::fs::write(&path, kinemo_render::raster::encode_png(&image))
            .map_err(|e| format!("could not write {path}: {e}"))?;
        println!("{path}");
    }
    Ok(())
}
