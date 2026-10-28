//! Building a scene, so the figure owns it.
//!
//! Importing kinemo's JSON worked for one knob and stopped there: a bare
//! `k.signal(...)` carries no name, so two of them cannot be told apart. A
//! figure for a mass on a spring wants a position, a stiffness and a damping
//! at once, each set by name.
//!
//! So hapto builds the scene. The pieces are kinemo's — its shapes, its
//! layout, its renderer — and what hapto adds is a value that has a name and
//! an expression language over it. There is still no timeline: an expression
//! bound at `t = 0` is a *binding*, not an animation, and nothing ever
//! advances past it.

use std::collections::BTreeMap;

use kinemo_ir::{BinOp, Entry, Expr, Object, Scene, Signal, Span, Src, UnOp, Value};

/// A number in a scene being built: either a literal or something derived
/// from a named value. `Num` is what every shape's property takes, so a
/// radius can be a constant and a position can follow a knob.
#[derive(Debug, Clone)]
pub enum Num {
    Fixed(f64),
    Of(Expr),
}

impl From<f64> for Num {
    fn from(n: f64) -> Self {
        Num::Fixed(n)
    }
}

impl From<i32> for Num {
    fn from(n: i32) -> Self {
        Num::Fixed(n as f64)
    }
}

impl Num {
    fn expr(&self) -> Expr {
        match self {
            Num::Fixed(n) => Expr::Const {
                v: Value::Float(*n),
            },
            Num::Of(e) => e.clone(),
        }
    }

    fn is_fixed(&self) -> Option<f64> {
        match self {
            Num::Fixed(n) => Some(*n),
            Num::Of(_) => None,
        }
    }

    fn bin(self, op: BinOp, other: Num) -> Num {
        // Two literals fold now rather than at every frame.
        if let (Some(a), Some(b)) = (self.is_fixed(), other.is_fixed()) {
            let folded = match op {
                BinOp::Add => Some(a + b),
                BinOp::Sub => Some(a - b),
                BinOp::Mul => Some(a * b),
                BinOp::Div if b != 0.0 => Some(a / b),
                _ => None,
            };
            if let Some(value) = folded {
                return Num::Fixed(value);
            }
        }
        Num::Of(binary(op, self.expr(), other.expr()))
    }

    /// `a + b`, where either may follow a value.
    pub fn plus(self, other: impl Into<Num>) -> Num {
        self.bin(BinOp::Add, other.into())
    }

    pub fn minus(self, other: impl Into<Num>) -> Num {
        self.bin(BinOp::Sub, other.into())
    }

    pub fn times(self, other: impl Into<Num>) -> Num {
        self.bin(BinOp::Mul, other.into())
    }

    pub fn over(self, other: impl Into<Num>) -> Num {
        self.bin(BinOp::Div, other.into())
    }

    /// `self * self`, which is how a figure says "squared" without a power.
    pub fn squared(self) -> Num {
        self.clone().times(self)
    }

    pub fn negated(self) -> Num {
        match self.is_fixed() {
            Some(n) => Num::Fixed(-n),
            None => Num::Of(Expr::Un {
                f: UnOp::Neg,
                a: Box::new(self.expr()),
            }),
        }
    }

    pub fn abs(self) -> Num {
        match self.is_fixed() {
            Some(n) => Num::Fixed(n.abs()),
            None => Num::Of(Expr::Un {
                f: UnOp::Abs,
                a: Box::new(self.expr()),
            }),
        }
    }

    pub fn sin(self) -> Num {
        Num::Of(Expr::Un {
            f: UnOp::Sin,
            a: Box::new(self.expr()),
        })
    }

    pub fn cos(self) -> Num {
        Num::Of(Expr::Un {
            f: UnOp::Cos,
            a: Box::new(self.expr()),
        })
    }

    pub fn sqrt(self) -> Num {
        Num::Of(Expr::Un {
            f: UnOp::Sqrt,
            a: Box::new(self.expr()),
        })
    }
}

fn binary(f: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Bin {
        f,
        a: Box::new(a),
        b: Box::new(b),
    }
}

/// How a shape is painted. Every field is optional: a figure says what it
/// cares about and the theme supplies the rest.
#[derive(Debug, Clone, Default)]
pub struct Style {
    pub fill: Option<[f64; 4]>,
    pub stroke: Option<[f64; 4]>,
    pub stroke_width: Option<f64>,
    pub fill_opacity: Option<f64>,
}

impl Style {
    pub fn fill(colour: [f64; 4]) -> Self {
        Style {
            fill: Some(colour),
            // kinemo's renderer defaults `fill_opacity` to 0, so a shape with
            // a colour and nothing else is painted completely transparent.
            // Asking for a fill here means asking to see it; `and_opacity`
            // dims it again.
            fill_opacity: Some(1.0),
            ..Default::default()
        }
    }

    pub fn stroke(colour: [f64; 4], width: f64) -> Self {
        Style {
            stroke: Some(colour),
            stroke_width: Some(width),
            ..Default::default()
        }
    }

    pub fn and_fill(mut self, colour: [f64; 4]) -> Self {
        self.fill = Some(colour);
        self.fill_opacity.get_or_insert(1.0);
        self
    }

    pub fn and_opacity(mut self, opacity: f64) -> Self {
        self.fill_opacity = Some(opacity);
        self
    }
}

/// How heavy a stroke is.
///
/// Not in scene units: the renderer scales stroke width separately from
/// positions, so a line 0.05 "wide" is a line nobody can see. kinemo's own
/// scenes use 1.5 to 4, and these are those weights under names, so a figure
/// says what a line is *for* rather than guessing a number.
pub mod weight {
    /// An axis, a guide, a construction line: present but out of the way.
    pub const HAIR: f64 = 1.5;
    /// An ordinary line of the figure.
    pub const LINE: f64 = 2.5;
    /// What the figure is about: the vector, the tangent, the force.
    pub const HEAVY: f64 = 4.0;
}

/// Colours a figure reaches for, matching kinemo's palette.
pub mod colour {
    pub const RED: [f64; 4] = [0.918, 0.345, 0.306, 1.0];
    pub const BLUE: [f64; 4] = [0.325, 0.600, 0.863, 1.0];
    pub const YELLOW: [f64; 4] = [0.937, 0.733, 0.286, 1.0];
    pub const GREEN: [f64; 4] = [0.388, 0.733, 0.447, 1.0];
    pub const GREY: [f64; 4] = [0.573, 0.557, 0.533, 1.0];
    pub const FG: [f64; 4] = [0.910, 0.894, 0.863, 1.0];
}

/// A scene under construction: named values, and shapes that follow them.
pub struct Build {
    scene: Scene,
    /// Name -> the signal that holds it, for `Figure` to write into.
    values: BTreeMap<String, u32>,
}

impl Default for Build {
    fn default() -> Self {
        Self::new()
    }
}

impl Build {
    pub fn new() -> Self {
        Build {
            scene: Scene {
                // Nothing moves on its own, so the scene is one instant long.
                duration: 0.0,
                // `Scene::default()` leaves this empty; a scene that says
                // which IR it speaks can be written out and read back.
                ir_version: kinemo_ir::IR_VERSION.into(),
                ..Default::default()
            },
            values: BTreeMap::new(),
        }
    }

    /// Declare a value the reader can set, and get a handle to use in shapes.
    ///
    /// This is what importing kinemo's JSON could not do: the signal has a
    /// name, so a figure can hold as many as it likes.
    pub fn value(&mut self, name: &str, at: f64) -> Num {
        let id = self.add_signal(Value::Float(at), None);
        self.values.insert(name.to_string(), id);
        Num::Of(Expr::Sig { id })
    }

    /// A dot at `(x, y)`, either of which may follow a value.
    pub fn dot(&mut self, x: Num, y: Num, radius: f64, style: Style) -> u32 {
        let props = self.props([("r", Num::Fixed(radius))], &style);
        self.object("dot", props, x, y)
    }

    /// A line from one point to another, any of which may follow a value.
    pub fn line(&mut self, from: (Num, Num), to: (Num, Num), style: Style) -> u32 {
        let mut props = self.props([], &style);
        let start = self.vec2(from.0, from.1);
        let end = self.vec2(to.0, to.1);
        props.insert("start".into(), start);
        props.insert("end".into(), end);
        self.object("line", props, Num::Fixed(0.0), Num::Fixed(0.0))
    }

    /// An arrow, which is a line with a head.
    pub fn arrow(&mut self, from: (Num, Num), to: (Num, Num), style: Style) -> u32 {
        let mut props = self.props([("tip", Num::Fixed(0.18))], &style);
        let start = self.vec2(from.0, from.1);
        let end = self.vec2(to.0, to.1);
        props.insert("start".into(), start);
        props.insert("end".into(), end);
        self.object("arrow", props, Num::Fixed(0.0), Num::Fixed(0.0))
    }

    /// A rectangle centred on `(x, y)`.
    pub fn rect(&mut self, x: Num, y: Num, width: Num, height: Num, style: Style) -> u32 {
        let props = self.props([("w", width), ("h", height)], &style);
        self.object("rect", props, x, y)
    }

    /// A closed shape through fixed corners.
    ///
    /// Fails if a corner follows a value; `quad` draws one that moves.
    pub fn polygon(&mut self, points: Vec<(Num, Num)>, style: Style) -> Result<u32, String> {
        let mut props = self.props([], &style);
        let list = self.point_list(points)?;
        props.insert("points".into(), list);
        Ok(self.object("polygon", props, Num::Fixed(0.0), Num::Fixed(0.0)))
    }

    /// An open path through fixed points: a trace, a fixed zigzag.
    pub fn polyline(&mut self, points: Vec<(Num, Num)>, style: Style) -> Result<u32, String> {
        let mut props = self.props([], &style);
        let list = self.point_list(points)?;
        props.insert("points".into(), list);
        Ok(self.object("polyline", props, Num::Fixed(0.0), Num::Fixed(0.0)))
    }

    /// A four-sided outline whose corners may each follow a value.
    ///
    /// This is the parallelogram two column vectors span, and it is drawn as
    /// four lines because a polygon's corners cannot move (see `point_list`).
    /// Its area going to zero is what "the determinant is zero" looks like.
    pub fn quad(&mut self, corners: [(Num, Num); 4], style: Style) -> Vec<u32> {
        let mut drawn = Vec::with_capacity(4);
        for i in 0..4 {
            let from = corners[i].clone();
            let to = corners[(i + 1) % 4].clone();
            drawn.push(self.line(from, to, style.clone()));
        }
        drawn
    }

    /// A spring between two points, drawn as a zigzag of `coils` turns.
    ///
    /// Both ends may follow a value, so the spring stretches as the mass
    /// moves. It is many short lines rather than one path, for the same
    /// reason `quad` is.
    pub fn spring(
        &mut self,
        from: (Num, Num),
        to: (Num, Num),
        coils: usize,
        width: f64,
        style: Style,
    ) -> Vec<u32> {
        let coils = coils.max(1);
        let steps = coils * 2;
        let mut drawn = Vec::with_capacity(steps + 1);

        // Each joint sits a fraction along the line, offset alternately above
        // and below it. With a moving end, that fraction is an expression.
        let along = |build: &Self, i: usize| -> (Num, Num) {
            let _ = build;
            let fraction = i as f64 / steps as f64;
            let x = from
                .0
                .clone()
                .times(1.0 - fraction)
                .plus(to.0.clone().times(fraction));
            let y = from
                .1
                .clone()
                .times(1.0 - fraction)
                .plus(to.1.clone().times(fraction));
            // The first and last joints stay on the line, so the spring meets
            // the wall and the block squarely.
            let offset = if i == 0 || i == steps {
                0.0
            } else if i % 2 == 1 {
                width
            } else {
                -width
            };
            (x, y.plus(offset))
        };

        for i in 0..steps {
            let a = along(self, i);
            let b = along(self, i + 1);
            drawn.push(self.line(a, b, style.clone()));
        }
        drawn
    }

    /// The scene, ready to draw, and where each named value lives.
    pub fn finish(self) -> (Scene, BTreeMap<String, u32>) {
        (self.scene, self.values)
    }

    // --- the plumbing ---

    fn add_signal(&mut self, initial: Value, bind: Option<Expr>) -> u32 {
        let id = self.scene.signals.len() as u32;
        let timeline = match bind {
            // One entry at t = 0 is a binding, not an animation: the value is
            // recomputed whenever what it reads changes, and nothing advances.
            Some(e) => vec![Entry::Set {
                t: 0.0,
                src: Src::Expr { e },
                span: Span::default(),
            }],
            None => Vec::new(),
        };
        self.scene.signals.push(Signal {
            id,
            initial,
            lerp: Default::default(),
            timeline,
            owner: None,
            span: Span::default(),
        });
        id
    }

    /// A signal holding a number, bound to an expression when it has one.
    fn number(&mut self, n: Num) -> u32 {
        match n {
            Num::Fixed(v) => self.add_signal(Value::Float(v), None),
            Num::Of(e) => self.add_signal(Value::Float(0.0), Some(e)),
        }
    }

    fn vec2(&mut self, x: Num, y: Num) -> u32 {
        match (x.is_fixed(), y.is_fixed()) {
            (Some(a), Some(b)) => self.add_signal(Value::Vec2([a, b]), None),
            _ => {
                let e = Expr::Vec2 {
                    x: Box::new(x.expr()),
                    y: Box::new(y.expr()),
                };
                self.add_signal(Value::Vec2([0.0, 0.0]), Some(e))
            }
        }
    }

    /// A `points` prop, which kinemo reads as a plain list of positions.
    ///
    /// The IR has no expression that builds a list, so the corners of a
    /// polygon cannot each follow a value: `Expr` can make a `Vec2`, but
    /// nothing can make a `List` of them. A shape that has to deform with a
    /// value is drawn as lines instead, whose endpoints are ordinary bound
    /// signals — see `Build::quad`.
    fn point_list(&mut self, points: Vec<(Num, Num)>) -> Result<u32, String> {
        let fixed: Option<Vec<Value>> = points
            .iter()
            .map(|(x, y)| match (x.is_fixed(), y.is_fixed()) {
                (Some(a), Some(b)) => Some(Value::Vec2([a, b])),
                _ => None,
            })
            .collect();
        match fixed {
            Some(values) => Ok(self.add_signal(Value::List(values), None)),
            None => Err("a polygon's corners cannot follow a value: the IR has no \
                         expression that builds a list. Use `quad` or `line` for a \
                         shape that has to move."
                .into()),
        }
    }

    fn props<const N: usize>(
        &mut self,
        numbers: [(&str, Num); N],
        style: &Style,
    ) -> BTreeMap<String, u32> {
        let mut props = BTreeMap::new();
        for (name, value) in numbers {
            let id = self.number(value);
            props.insert(name.to_string(), id);
        }
        if let Some(fill) = style.fill {
            let id = self.add_signal(Value::Color(fill), None);
            props.insert("fill".into(), id);
        }
        if let Some(stroke) = style.stroke {
            let id = self.add_signal(Value::Color(stroke), None);
            props.insert("stroke".into(), id);
        }
        if let Some(width) = style.stroke_width {
            let id = self.add_signal(Value::Float(width), None);
            props.insert("stroke_width".into(), id);
        }
        if let Some(opacity) = style.fill_opacity {
            let id = self.add_signal(Value::Float(opacity), None);
            props.insert("fill_opacity".into(), id);
        }
        props
    }

    fn object(&mut self, kind: &str, mut props: BTreeMap<String, u32>, x: Num, y: Num) -> u32 {
        let id = self.scene.objects.len() as u32;
        let x_id = self.number(x);
        let y_id = self.number(y);
        props.insert("x".into(), x_id);
        props.insert("y".into(), y_id);

        self.scene.objects.push(Object {
            id,
            kind: kind.to_string(),
            props,
            // An object is present only where a toggle says so: kinemo's
            // `present` starts at false, so an empty list never turns it on
            // and the frame comes back blank. Nothing here enters or leaves,
            // so everything is present from the one instant this scene has.
            presence: vec![(0.0, true)],
            ..Default::default()
        });
        self.scene.roots.push(id);
        id
    }
}
