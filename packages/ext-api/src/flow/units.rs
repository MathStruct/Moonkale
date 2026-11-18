//! Spaces and units of flow values (ADR-0032 §3): the domain a variable
//! lives in (`real`, `positive`, `simplex(3)`, …), ordered by
//! containment, and a small SI unit parser (`kg*m/s^2`, `km`, `Ω`) that
//! compares dimension and scale. No dependency, bounded input.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// The longest space or unit text accepted (a bounded parser).
const MAX_TEXT: usize = 128;

/// The domain of a value (ADR-0032 §3). Spaces are ordered by
/// containment: `binary ⊂ count ⊂ integer ⊂ real`, `binary ⊂ unit
/// interval ⊂ real`, `positive ⊂ real`; a parametric family meets only
/// itself, and an absent dimension matches any.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    /// Any real number.
    Real,
    /// A real number > 0 (a scale, a rate, a precision).
    Positive,
    /// A number in [0, 1] (a probability).
    UnitInterval,
    /// A whole number.
    Integer,
    /// A whole number ≥ 0.
    Count,
    /// 0 or 1.
    Binary,
    /// A probability vector of `K` entries.
    Simplex(Option<u64>),
    /// A positive definite `D × D` matrix.
    PosDef(Option<u64>),
    /// One of `K` categories.
    Categorical(Option<u64>),
}

impl Space {
    /// The spaces offered in a choice, without dimensions (the parameter
    /// card's list; a dimension is typed as `simplex(3)`).
    pub const CHOICES: [&'static str; 9] = [
        "real",
        "positive",
        "unit interval",
        "integer",
        "count",
        "binary",
        "simplex",
        "posdef",
        "categorical",
    ];

    /// Does `self` contain every value of `other`?
    pub fn contains(&self, other: &Space) -> bool {
        use Space::*;
        let dim = |a: &Option<u64>, b: &Option<u64>| a.is_none() || b.is_none() || a == b;
        match (self, other) {
            (a, b) if a == b => true,
            (Real, Positive | UnitInterval | Integer | Count | Binary) => true,
            (Integer, Count | Binary) => true,
            (Count, Binary) => true,
            (UnitInterval, Binary) => true,
            (Simplex(a), Simplex(b))
            | (PosDef(a), PosDef(b))
            | (Categorical(a), Categorical(b)) => dim(a, b),
            _ => false,
        }
    }

    /// The smaller of two spaces when one contains the other; `None`
    /// when neither does (the wire is refused). A family's dimension is
    /// the known one.
    pub fn meet(&self, other: &Space) -> Option<Space> {
        use Space::*;
        let d = |a: &Option<u64>, b: &Option<u64>| a.or(*b);
        match (self, other) {
            (Simplex(a), Simplex(b)) if self.contains(other) => Some(Simplex(d(a, b))),
            (PosDef(a), PosDef(b)) if self.contains(other) => Some(PosDef(d(a, b))),
            (Categorical(a), Categorical(b)) if self.contains(other) => Some(Categorical(d(a, b))),
            _ if self.contains(other) => Some(*other),
            _ if other.contains(self) => Some(*self),
            _ => None,
        }
    }

    /// One number per value (not a vector or a matrix).
    pub fn is_scalar(&self) -> bool {
        !matches!(self, Space::Simplex(_) | Space::PosDef(_))
    }
}

impl fmt::Display for Space {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fam = |f: &mut fmt::Formatter<'_>, name: &str, d: &Option<u64>| match d {
            Some(d) => write!(f, "{name}({d})"),
            None => f.write_str(name),
        };
        match self {
            Space::Real => f.write_str("real"),
            Space::Positive => f.write_str("positive"),
            Space::UnitInterval => f.write_str("unit interval"),
            Space::Integer => f.write_str("integer"),
            Space::Count => f.write_str("count"),
            Space::Binary => f.write_str("binary"),
            Space::Simplex(d) => fam(f, "simplex", d),
            Space::PosDef(d) => fam(f, "posdef", d),
            Space::Categorical(d) => fam(f, "categorical", d),
        }
    }
}

impl FromStr for Space {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if s.len() > MAX_TEXT {
            return Err("a space is a short name".into());
        }
        let t = s.trim().to_lowercase();
        let (name, dim) = match t.split_once('(') {
            Some((n, rest)) => {
                let inner = rest
                    .strip_suffix(')')
                    .ok_or_else(|| format!("{s:?}: a dimension closes with )"))?;
                let d: u64 = inner
                    .trim()
                    .parse()
                    .ok()
                    .filter(|d| *d >= 1)
                    .ok_or_else(|| format!("{s:?}: the dimension is a whole number ≥ 1"))?;
                (n.trim().to_string(), Some(d))
            }
            None => (t.clone(), None),
        };
        let plain = |sp: Space| {
            if dim.is_some() {
                Err(format!("{name} takes no dimension"))
            } else {
                Ok(sp)
            }
        };
        match name.as_str() {
            "real" => plain(Space::Real),
            "positive" => plain(Space::Positive),
            "unit interval" | "unit_interval" | "probability" => plain(Space::UnitInterval),
            "integer" => plain(Space::Integer),
            "count" => plain(Space::Count),
            "binary" => plain(Space::Binary),
            "simplex" => Ok(Space::Simplex(dim)),
            "posdef" => Ok(Space::PosDef(dim)),
            "categorical" => Ok(Space::Categorical(dim)),
            _ => Err(format!(
                "unknown space {s:?} (one of {})",
                Space::CHOICES.join(", ")
            )),
        }
    }
}

impl Serialize for Space {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Space {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A unit as a dimension over the seven SI base units (m, kg, s, A, K,
/// mol, cd) and a scale to SI (`km` = 1000 · m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    /// Exponents of m, kg, s, A, K, mol, cd.
    pub dims: [i8; 7],
    /// The factor to SI.
    pub scale: f64,
}

const BASE: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];

impl Unit {
    /// The largest exponent of a base unit (`m^60`).
    pub const MAX_EXPONENT: i32 = 60;

    /// Dimensionless, scale 1.
    pub const ONE: Unit = Unit {
        dims: [0; 7],
        scale: 1.0,
    };

    /// The product, or `None` when an exponent or the scale leaves the
    /// bounds ([`Unit::MAX_EXPONENT`], a finite non-zero scale): never
    /// a saturated or infinite unit that compares equal to another.
    fn mul(self, o: Unit) -> Option<Unit> {
        let mut dims = self.dims;
        for (d, e) in dims.iter_mut().zip(o.dims) {
            let n = *d as i32 + e as i32;
            if n.abs() > Unit::MAX_EXPONENT {
                return None;
            }
            *d = n as i8;
        }
        Unit {
            dims,
            scale: self.scale * o.scale,
        }
        .bounded()
    }

    /// `self` raised to `k`, or `None` when an exponent passes
    /// [`Unit::MAX_EXPONENT`] or the scale leaves 1e±200.
    pub fn pow(self, k: i32) -> Option<Unit> {
        let mut dims = self.dims;
        for d in dims.iter_mut() {
            let n = (*d as i32).checked_mul(k)?;
            if n.abs() > Unit::MAX_EXPONENT {
                return None;
            }
            *d = n as i8;
        }
        Unit {
            dims,
            scale: self.scale.powi(k),
        }
        .bounded()
    }

    fn bounded(self) -> Option<Unit> {
        let a = self.scale.abs();
        (self.scale.is_finite() && (1e-200..=1e200).contains(&a)).then_some(self)
    }

    /// The `k`-th root, when every exponent divides (`m^2` → `m`).
    pub fn root(self, k: i32) -> Option<Unit> {
        if k == 0 {
            return None;
        }
        let mut dims = self.dims;
        for d in dims.iter_mut() {
            if *d as i32 % k != 0 {
                return None;
            }
            *d = (*d as i32 / k) as i8;
        }
        Some(Unit {
            dims,
            scale: self.scale.powf(1.0 / k as f64),
        })
    }

    /// Same dimension.
    pub fn same_dimension(&self, o: &Unit) -> bool {
        self.dims == o.dims
    }

    /// Same dimension and scale (to a relative 1e-9).
    pub fn same(&self, o: &Unit) -> bool {
        self.same_dimension(o)
            && (self.scale - o.scale).abs() <= 1e-9 * self.scale.abs().max(o.scale.abs())
    }

    /// Parse `kg*m/s^2`, `m s^-1`, `km`, `Ω`, `1`. Everything after a `/`
    /// is in the denominator (`J/kg/K` = `J/(kg·K)`); no parentheses.
    pub fn parse(text: &str) -> Result<Unit, String> {
        if text.len() > MAX_TEXT {
            return Err("a unit is a short expression".into());
        }
        let t = text.trim();
        if t.is_empty() {
            return Err("an empty unit".into());
        }
        let mut out = Unit::ONE;
        let mut denom = false;
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut Unit, denom: bool| -> Result<(), String> {
            if word.is_empty() {
                return Ok(());
            }
            let (sym, exp) = match word.split_once('^') {
                Some((s, e)) => (
                    s.to_string(),
                    e.parse::<i32>()
                        .ok()
                        .filter(|e| (-12..=12).contains(e))
                        .ok_or_else(|| format!("{word:?}: the power is a small whole number"))?,
                ),
                None => (word.clone(), 1),
            };
            let u = symbol(&sym).ok_or_else(|| format!("unknown unit {sym:?}"))?;
            *out = u
                .pow(if denom { -exp } else { exp })
                .and_then(|p| out.mul(p))
                .ok_or_else(|| format!("{text:?} is too large a unit"))?;
            word.clear();
            Ok(())
        };
        for c in t.chars() {
            match c {
                '*' | '·' | ' ' => flush(&mut word, &mut out, denom)?,
                '/' => {
                    flush(&mut word, &mut out, denom)?;
                    denom = true;
                }
                '²' => word.push_str("^2"),
                '³' => word.push_str("^3"),
                _ => word.push(c),
            }
        }
        flush(&mut word, &mut out, denom)?;
        Ok(out)
    }
}

impl fmt::Display for Unit {
    /// The SI form (`kg·m·s^-2`), for messages.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = BASE
            .iter()
            .zip(self.dims)
            .filter(|(_, e)| *e != 0)
            .map(|(b, e)| {
                if e == 1 {
                    b.to_string()
                } else {
                    format!("{b}^{e}")
                }
            })
            .collect();
        if parts.is_empty() {
            f.write_str("1")
        } else {
            f.write_str(&parts.join("·"))
        }
    }
}

fn dims(m: i8, kg: i8, s: i8, a: i8, k: i8, mol: i8, cd: i8) -> [i8; 7] {
    [m, kg, s, a, k, mol, cd]
}

/// A unit symbol, with an SI prefix where the bare symbol is not itself
/// a unit (`m` is the metre, `mm` the millimetre, `min` the minute).
fn symbol(sym: &str) -> Option<Unit> {
    let u = |d: [i8; 7], scale: f64| Some(Unit { dims: d, scale });
    let named = match sym {
        "1" => u([0; 7], 1.0),
        "m" => u(dims(1, 0, 0, 0, 0, 0, 0), 1.0),
        "g" => u(dims(0, 1, 0, 0, 0, 0, 0), 1e-3),
        "s" => u(dims(0, 0, 1, 0, 0, 0, 0), 1.0),
        "A" => u(dims(0, 0, 0, 1, 0, 0, 0), 1.0),
        "K" => u(dims(0, 0, 0, 0, 1, 0, 0), 1.0),
        "mol" => u(dims(0, 0, 0, 0, 0, 1, 0), 1.0),
        "cd" => u(dims(0, 0, 0, 0, 0, 0, 1), 1.0),
        "N" => u(dims(1, 1, -2, 0, 0, 0, 0), 1.0),
        "J" => u(dims(2, 1, -2, 0, 0, 0, 0), 1.0),
        "W" => u(dims(2, 1, -3, 0, 0, 0, 0), 1.0),
        "Pa" => u(dims(-1, 1, -2, 0, 0, 0, 0), 1.0),
        "Hz" => u(dims(0, 0, -1, 0, 0, 0, 0), 1.0),
        "C" => u(dims(0, 0, 1, 1, 0, 0, 0), 1.0),
        "V" => u(dims(2, 1, -3, -1, 0, 0, 0), 1.0),
        "Ω" | "ohm" => u(dims(2, 1, -3, -2, 0, 0, 0), 1.0),
        "F" => u(dims(-2, -1, 4, 2, 0, 0, 0), 1.0),
        "H" => u(dims(2, 1, -2, -2, 0, 0, 0), 1.0),
        "L" | "l" => u(dims(3, 0, 0, 0, 0, 0, 0), 1e-3),
        "min" => u(dims(0, 0, 1, 0, 0, 0, 0), 60.0),
        "h" => u(dims(0, 0, 1, 0, 0, 0, 0), 3600.0),
        "d" => u(dims(0, 0, 1, 0, 0, 0, 0), 86400.0),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    let prefixes: [(&str, f64); 10] = [
        ("G", 1e9),
        ("M", 1e6),
        ("k", 1e3),
        ("c", 1e-2),
        ("m", 1e-3),
        ("µ", 1e-6),
        ("μ", 1e-6),
        ("u", 1e-6),
        ("n", 1e-9),
        ("p", 1e-12),
    ];
    for (p, f) in prefixes {
        if let Some(rest) = sym.strip_prefix(p) {
            // One prefix only, and never on `1`, `min`, `h` or `d`.
            if !rest.is_empty() && !matches!(rest, "1" | "min" | "h" | "d") {
                let base = match rest {
                    "m" | "g" | "s" | "A" | "K" | "mol" | "cd" | "N" | "J" | "W" | "Pa" | "Hz"
                    | "C" | "V" | "Ω" | "F" | "H" | "L" | "l" => symbol(rest),
                    _ => None,
                };
                if let Some(b) = base {
                    return Some(Unit {
                        dims: b.dims,
                        scale: b.scale * f,
                    });
                }
            }
        }
    }
    None
}

/// A pin's unit as a kind writes it: a concrete unit, or a unit
/// variable `'u` raised to a power (`'u^2`), bound per block
/// (ADR-0032 §3).
#[derive(Clone, Debug, PartialEq)]
pub enum UnitTerm {
    /// A concrete unit.
    Fixed(Unit),
    /// A unit variable and its power.
    Var(String, i32),
}

/// Read a pin's unit text: `'u`, `'u^2`, or a unit expression.
pub fn unit_term(text: &str) -> Result<UnitTerm, String> {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix('\'') {
        let (name, k) = match rest.split_once('^') {
            Some((n, e)) => (
                n,
                e.parse::<i32>()
                    .ok()
                    .filter(|e| *e != 0 && (-12..=12).contains(e))
                    .ok_or_else(|| format!("{t:?}: the power is a small whole number"))?,
            ),
            None => (rest, 1),
        };
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!("{t:?} is not a unit variable"));
        }
        return Ok(UnitTerm::Var(name.to_string(), k));
    }
    Unit::parse(t).map(UnitTerm::Fixed)
}

/// Why two unit texts do not unify, or `None` when they do (equal in
/// dimension and scale). A unit variable is not checked here: it is
/// bound per block.
pub fn unit_refusal(a: &str, b: &str) -> Option<String> {
    let (ta, tb) = match (unit_term(a), unit_term(b)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return Some(e),
    };
    match (ta, tb) {
        (UnitTerm::Fixed(x), UnitTerm::Fixed(y)) => mismatch(a, &x, b, &y),
        _ => None,
    }
}

/// Why two concrete units differ, worded for a refusal.
pub fn mismatch(a: &str, x: &Unit, b: &str, y: &Unit) -> Option<String> {
    if x.same(y) {
        None
    } else if x.same_dimension(y) {
        Some(format!(
            "{a} and {b} measure the same at another scale — convert explicitly"
        ))
    } else {
        Some(format!("{a} ({x}) and {b} ({y}) are different dimensions"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_round_trip_and_order_by_containment() {
        for s in [
            "real",
            "positive",
            "unit interval",
            "integer",
            "count",
            "binary",
            "simplex(3)",
            "posdef",
            "categorical(4)",
        ] {
            assert_eq!(s.parse::<Space>().unwrap().to_string(), s);
        }
        let p = |s: &str| s.parse::<Space>().unwrap();
        assert_eq!(p("real").meet(&p("binary")), Some(Space::Binary));
        assert_eq!(p("count").meet(&p("unit interval")), None);
        assert_eq!(p("positive").meet(&p("unit interval")), None);
        assert_eq!(
            p("simplex").meet(&p("simplex(3)")),
            Some(Space::Simplex(Some(3)))
        );
        assert_eq!(p("simplex(4)").meet(&p("simplex(3)")), None);
        assert_eq!(p("posdef(2)").meet(&p("real")), None);
        assert!("simplex(0)".parse::<Space>().is_err());
        assert!("real(2)".parse::<Space>().is_err());
        assert!("reals".parse::<Space>().is_err());
        assert!("x".repeat(500).parse::<Space>().is_err());
    }

    #[test]
    fn units_compare_by_dimension_and_scale() {
        let u = |s: &str| Unit::parse(s).unwrap();
        assert!(u("m/s").same(&u("m*s^-1")));
        assert!(u("N").same(&u("kg*m/s^2")));
        assert!(u("kg m s^-2").same(&u("N")));
        assert!(u("J/kg/K").same(&u("m^2/s^2/K")));
        assert!(u("km").same_dimension(&u("m")) && !u("km").same(&u("m")));
        assert!(u("mm").same(
            &u("m")
                .pow(1)
                .unwrap()
                .mul(Unit {
                    dims: [0; 7],
                    scale: 1e-3
                })
                .unwrap()
        ));
        assert!(u("min").same(
            &u("s")
                .mul(Unit {
                    dims: [0; 7],
                    scale: 60.0
                })
                .unwrap()
        ));
        assert!(u("Ω").same(&u("V/A")));
        assert!(u("m²").same(&u("m^2")));
        assert!(u("1").same(&Unit::ONE));
        assert!(Unit::parse("furlong").is_err());
        assert!(Unit::parse("m^x").is_err());
        assert!(Unit::parse("m^99").is_err());
        assert!(Unit::parse("").is_err());
        assert!(Unit::parse("kkm").is_err());
        assert_eq!(u("m^2").root(2), Some(u("m")));
        assert!(Unit::parse("m^61").is_err() || Unit::parse("m^12*m^12*m^12*m^12*m^12*m").is_err());
        assert_eq!(u("m^3").root(2), None);
    }

    #[test]
    fn refusals_name_the_reason() {
        assert!(unit_refusal("m", "m").is_none());
        assert!(unit_refusal("m/s", "s")
            .unwrap()
            .contains("different dimensions"));
        assert!(unit_refusal("km", "m")
            .unwrap()
            .contains("convert explicitly"));
        assert!(unit_refusal("'u", "s").is_none());
        assert_eq!(unit_term("'u^2"), Ok(UnitTerm::Var("u".into(), 2)));
        assert!(unit_term("'").is_err());
        assert!(unit_term("'u^0").is_err());
    }

    #[test]
    fn hostile_units_are_refused_not_wrapped() {
        // Review (Codex) #1: the most negative power must not overflow.
        assert!(Unit::parse("m^-2147483648").is_err());
        assert!(unit_term("'u^-2147483648").is_err());
        // #2: a scale beyond f64 is refused, never ∞ = ∞.
        assert!(Unit::parse("Gm^12*Gm^12*Gm^12").is_err());
        assert!(Unit::parse("pm^12*pm^12*pm^12*pm^12").is_err());
        // #3: exponents never saturate into a different dimension.
        let eleven = ["m^12"; 11].join("*");
        let other = format!("{}*m^7", ["m^12"; 10].join("*"));
        // Refusing either is fine; equating them is not.
        if let (Ok(a), Ok(b)) = (Unit::parse(&eleven), Unit::parse(&other)) {
            assert!(!a.same(&b));
        }
    }
}
