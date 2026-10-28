//! Draw the parabola figure at a few values, to look at it.
//!
//! ```bash
//! cargo run -p hapto-core --example draw -- 0.5 2 4.5
//! ```
//!
//! Writes `hapto-x<value>.png` per value in the current directory. The dot
//! should sit on the curve at each one, and nothing else should differ.

use hapto_core::{Domain, Figure, Knob, Size, State, Value};

fn main() -> Result<(), String> {
    let values: Vec<f64> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("a number"))
        .collect();
    let values = if values.is_empty() {
        vec![0.5, 2.0, 4.5]
    } else {
        values
    };

    let mut figure = Figure::from_json(include_str!("../tests/parabola.json"))?;
    figure.bind_free("x")?;

    for x in values {
        let mut state = State::new();
        state.declare(Knob {
            name: "x".into(),
            domain: Domain::Number {
                low: -1.0,
                high: 5.0,
                step: None,
            },
            value: Value::number(x),
            label: String::new(),
        });

        let image = figure.draw(&state, Size::default())?;
        let path = format!("hapto-x{x}.png");
        std::fs::write(&path, kinemo_render::raster::encode_png(&image))
            .map_err(|e| format!("could not write {path}: {e}"))?;
        println!("{path}  ({}x{})", image.width, image.height);
    }
    Ok(())
}
