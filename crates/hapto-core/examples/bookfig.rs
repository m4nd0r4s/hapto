//! Render the book's figures from real lesson specs, to look at them.
//!
//! ```bash
//! cargo run -p hapto-core --example bookfig
//! ```

use hapto_core::book::{columns, plot, Range};
use hapto_core::{Size, Value};

fn main() -> Result<(), String> {
    let size = Size {
        width: 960,
        height: 540,
    };

    // lessons/calc-partial-derivatives: the parabola with its tangent.
    for at in [1.0, 3.0] {
        let mut f = plot(
            "x**2 - x*(2)",
            "x",
            1.0,
            Range::new(-2.0, 4.0),
            Range::new(-2.0, 4.0),
            Range::new(-2.35, 9.35),
            true,
        )?;
        f.set("x", Value::number(at))?;
        let name = format!("book-parabola-{at}.png");
        std::fs::write(&name, kinemo_render::raster::encode_png(&f.draw_now(size)?))
            .map_err(|e| e.to_string())?;
        println!("{name}");
    }

    // lessons/la-matrix-theory: columns (2,1) and (x,2), singular at x = 4.
    for at in [1.0, 4.0] {
        let mut f = columns(
            ("2", "1"),
            ("x", "2"),
            "x",
            1.0,
            Range::new(-2.0, 6.0),
            Range::new(-1.0, 9.0),
            Range::new(-1.0, 3.0),
        )?;
        f.set("x", Value::number(at))?;
        let name = format!("book-columns-{at}.png");
        std::fs::write(&name, kinemo_render::raster::encode_png(&f.draw_now(size)?))
            .map_err(|e| e.to_string())?;
        println!("{name}");
    }
    Ok(())
}
