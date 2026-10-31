//! Bounded data preparation and finite scales, independent of the UI.
use moonkale_core::{DateTimeCell, Table, Value};
use std::collections::BTreeMap;

pub const INPUT_CAP: usize = 50_000;
pub const POINT_CAP: usize = 5_000;
pub const BIN_CAP: usize = 40;
pub const CATEGORY_CAP: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Line,
    Scatter,
    Bar,
    Histogram,
}
impl Mark {
    pub const ALL: [Self; 4] = [Self::Line, Self::Scatter, Self::Bar, Self::Histogram];
    pub fn id(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Scatter => "scatter",
            Self::Bar => "bar",
            Self::Histogram => "histogram",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scale {
    Linear,
    Log,
    Time,
}
impl Scale {
    pub const ALL: [Self; 3] = [Self::Linear, Self::Log, Self::Time];
    pub fn id(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Log => "log",
            Self::Time => "time",
        }
    }
    pub fn transform(self, v: f64) -> Option<f64> {
        if !v.is_finite() {
            return None;
        }
        match self {
            Self::Log if v > 0.0 => Some(v.log10()),
            Self::Log => None,
            Self::Time => DateTimeCell::new(v as i64, 0, None).map(|_| v),
            _ => Some(v),
        }
    }
    pub fn label(self, v: f64) -> String {
        match self {
            Self::Time => DateTimeCell::new(v as i64, 0, None)
                .map(|d| Value::DateTime(d).to_string())
                .unwrap_or_default(),
            Self::Log => format_number(10f64.powf(v)),
            _ => format_number(v),
        }
    }
}
fn format_number(n: f64) -> String {
    if n != 0.0 && !(0.001..1_000_000.0).contains(&n.abs()) {
        format!("{n:.2e}")
    } else {
        format!("{n:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    pub x: usize,
    pub y: usize,
    pub mark: Mark,
    pub xs: Scale,
    pub ys: Scale,
    pub equal: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Number,
    Time,
    Category,
    Other,
}
fn kind(table: &Table, c: usize) -> Kind {
    // Ignore nulls; mixed columns are only numeric/time when all sampled values agree.
    let mut found = Kind::Other;
    for v in table
        .rows
        .iter()
        .take(64)
        .filter_map(|r| r.get(c))
        .filter(|v| !matches!(v, Value::Null))
    {
        let next = match v {
            Value::Int(_) | Value::Float(_) => Kind::Number,
            Value::DateTime(_) => Kind::Time,
            Value::Text(_) | Value::Bool(_) | Value::Ref { .. } => Kind::Category,
            _ => Kind::Other,
        };
        if found != Kind::Other && found != next {
            return Kind::Other;
        }
        found = next;
    }
    found
}
pub fn default_spec(table: &Table) -> Spec {
    let kinds: Vec<_> = (0..table.columns.len().min(256))
        .map(|c| kind(table, c))
        .collect();
    let nums: Vec<_> = kinds
        .iter()
        .enumerate()
        .filter_map(|(i, k)| (*k == Kind::Number).then_some(i))
        .collect();
    let time = kinds.iter().position(|k| *k == Kind::Time);
    let cat = kinds.iter().position(|k| *k == Kind::Category);
    let (x, y, mark, xs) = match (time, nums.as_slice(), cat) {
        (Some(t), [n, ..], _) => (t, *n, Mark::Line, Scale::Time),
        (_, [a, b, ..], _) => (*a, *b, Mark::Scatter, Scale::Linear),
        (_, [n], Some(c)) => (c, *n, Mark::Bar, Scale::Linear),
        (_, [n], _) => (*n, *n, Mark::Histogram, Scale::Linear),
        _ => (0, 0, Mark::Scatter, Scale::Linear),
    };
    Spec {
        x,
        y,
        mark,
        xs,
        ys: Scale::Linear,
        equal: false,
    }
}
pub fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(n) if n.is_finite() => Some(*n),
        Value::DateTime(d) => Some(d.seconds() as f64),
        _ => None,
    }
}
/// Uniform deterministic sampling includes both endpoints, with no allocation
/// proportional to the input. Selection keeps the original row index.
pub fn decimate(n: usize, cap: usize) -> Vec<usize> {
    if cap == 0 || n == 0 {
        return vec![];
    }
    if n <= cap {
        return (0..n).collect();
    }
    if cap == 1 {
        return vec![0];
    }
    (0..cap).map(|i| i * (n - 1) / (cap - 1)).collect()
}
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub row: usize,
    pub members: Vec<usize>,
    pub count: usize,
    pub label: String,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prepared {
    pub points: Vec<Point>,
    pub sampled: bool,
    pub omitted: usize,
    pub categories: Vec<String>,
    pub bar_width: f64,
}
fn short(v: &Value) -> String {
    v.to_string().chars().take(80).collect()
}
pub fn prepare(table: &Table, s: Spec) -> Prepared {
    let n = table.rows.len().min(INPUT_CAP);
    let mut out = Prepared {
        sampled: table.rows.len() > INPUT_CAP,
        ..Default::default()
    };
    if s.mark == Mark::Bar {
        let mut groups: BTreeMap<String, (f64, Vec<usize>)> = BTreeMap::new();
        for (i, row) in table.rows.iter().take(n).enumerate() {
            let Some((x, y)) = row.get(s.x).zip(row.get(s.y)) else {
                out.omitted += 1;
                continue;
            };
            let Some(y) = number(y) else {
                out.omitted += 1;
                continue;
            };
            // Key the full typed value by digest, retaining only bounded display
            // text; long categories with a shared prefix must stay distinct.
            let label = format!(
                "{}\0{}",
                short(x),
                moonkale_core::NodeId::from_content(format!("{x:?}").as_bytes())
            );
            if !groups.contains_key(&label) && groups.len() >= CATEGORY_CAP {
                out.omitted += 1;
                continue;
            }
            let entry = groups.entry(label).or_insert((0.0, Vec::new()));
            entry.0 += y;
            entry.1.push(i);
        }
        out.bar_width = 0.8;
        for (label, (sum, members)) in groups {
            let row = members[0];
            let count = members.len();
            let label = label
                .split_once('\0')
                .map(|(s, _)| s)
                .unwrap_or(&label)
                .to_string();
            if s.ys.transform(sum).is_some() {
                let x = out.points.len() as f64;
                out.categories.push(label.clone());
                out.points.push(Point {
                    x,
                    y: sum,
                    row,
                    members,
                    count,
                    label,
                });
            } else {
                out.omitted += count;
            }
        }
    } else if s.mark == Mark::Histogram {
        let values: Vec<_> = table
            .rows
            .iter()
            .take(n)
            .enumerate()
            .filter_map(|(i, r)| Some((i, s.xs.transform(number(r.get(s.x)?)?)?)))
            .collect();
        out.omitted = n - values.len();
        if values.is_empty() {
            return out;
        }
        let lo = values.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
        let hi = values
            .iter()
            .map(|(_, v)| *v)
            .fold(f64::NEG_INFINITY, f64::max);
        if !(hi - lo).is_finite() {
            out.omitted += values.len();
            return out;
        }
        let bins = if hi == lo || (hi - lo) / (BIN_CAP as f64) == 0.0 {
            1
        } else {
            BIN_CAP
        };
        let width = if bins == 1 {
            1.0
        } else {
            (hi - lo) / bins as f64
        };
        out.bar_width = width * 0.9;
        let mut counts = vec![Vec::new(); bins];
        for (row, v) in values {
            let b = (((v - lo) / width) as usize).min(bins - 1);
            counts[b].push(row);
        }
        for (i, members) in counts.into_iter().enumerate() {
            let count = members.len();
            let row = members.first().copied().unwrap_or(0);
            if count > 0 && s.ys.transform(count as f64).is_some() {
                let start = lo + i as f64 * width;
                out.points.push(Point {
                    x: start + width / 2.0,
                    y: count as f64,
                    row,
                    members,
                    count,
                    label: format!("{} … {}", s.xs.label(start), s.xs.label(start + width)),
                });
            }
        }
    } else {
        let sampled = decimate(n, POINT_CAP);
        out.sampled |= sampled.len() < n;
        for i in sampled {
            let row = &table.rows[i];
            if let Some((x, y)) = row
                .get(s.x)
                .and_then(number)
                .zip(row.get(s.y).and_then(number))
            {
                if s.xs.transform(x).is_some() && s.ys.transform(y).is_some() {
                    out.points.push(Point {
                        x,
                        y,
                        row: i,
                        members: vec![i],
                        count: 1,
                        label: String::new(),
                    });
                    continue;
                }
            }
            out.omitted += 1;
        }
        if s.mark == Mark::Line {
            out.points.sort_by(|a, b| a.x.total_cmp(&b.x));
        }
    }
    out
}
#[derive(Clone, Copy, Debug)]
pub struct Axis {
    pub lo: f64,
    pub hi: f64,
}
impl Axis {
    pub fn from_values(values: impl Iterator<Item = f64>) -> Self {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in values.filter(|v| v.is_finite()) {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        if !lo.is_finite() {
            return Self { lo: 0.0, hi: 1.0 };
        }
        if lo == hi {
            let pad = (lo.abs() * 0.05).max(0.5);
            lo = (lo - pad).max(-f64::MAX);
            hi = (hi + pad).min(f64::MAX);
        }
        // Refuse ranges whose subtraction overflows rather than emitting NaN SVG.
        if !(hi - lo).is_finite() {
            return Self { lo: -1.0, hi: 1.0 };
        }
        Self { lo, hi }
    }
    pub fn unit(self, v: f64) -> f64 {
        let unit = (v * 0.5 - self.lo * 0.5) / (self.hi * 0.5 - self.lo * 0.5);
        if unit.is_nan() {
            0.5
        } else {
            unit.clamp(-10_000.0, 10_000.0)
        }
    }
    pub fn ticks(self, pixels: f64, label_width: f64) -> Vec<f64> {
        let count = (pixels / label_width.max(40.0)).floor().clamp(2.0, 8.0) as usize;
        (0..count)
            .map(|i| self.lo + (self.hi - self.lo) * (i as f64 / (count - 1) as f64))
            .collect()
    }
    pub fn viewport(self, offset: f64, zoom: f64) -> Self {
        let span = (self.hi - self.lo) / zoom;
        let middle = self.lo + (self.hi - self.lo) / 2.0 + offset * (self.hi - self.lo);
        let lo = middle - span / 2.0;
        let hi = middle + span / 2.0;
        if lo.is_finite() && hi.is_finite() && lo < hi {
            Self { lo, hi }
        } else {
            self
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn table(rows: Vec<Vec<Value>>) -> Table {
        Table {
            columns: (0..rows[0].len()).map(|i| i.to_string()).collect(),
            rows,
            truncated: false,
        }
    }
    #[test]
    fn defaults_follow_types() {
        let n = Value::Int(2);
        let d = Value::DateTime(DateTimeCell::parse("2026-01-01").unwrap());
        for (row, mark, xs) in [
            (vec![d, n.clone()], Mark::Line, Scale::Time),
            (vec![n.clone(), n.clone()], Mark::Scatter, Scale::Linear),
            (
                vec![Value::Text("a".into()), n.clone()],
                Mark::Bar,
                Scale::Linear,
            ),
            (vec![n], Mark::Histogram, Scale::Linear),
        ] {
            let s = default_spec(&table(vec![row]));
            assert_eq!((s.mark, s.xs), (mark, xs));
        }
    }
    #[test]
    fn scales_ticks_and_degenerate_ranges() {
        assert_eq!(Scale::Log.transform(100.0), Some(2.0));
        assert_eq!(Scale::Log.transform(0.0), None);
        assert_eq!(Scale::Linear.transform(f64::NAN), None);
        assert!(Scale::Time.label(0.0).starts_with("1970-01-01"));
        let a = Axis::from_values([4.0, 4.0].into_iter());
        assert_eq!(a.unit(4.0), 0.5);
        for width in [200.0, 500.0, 1000.0] {
            let ticks = a.ticks(width, 100.0);
            assert!((2..=8).contains(&ticks.len()));
            assert!(ticks.windows(2).all(|p| p[1] > p[0]));
        }
        assert_eq!(
            a.viewport(0.0, 2.0).hi - a.viewport(0.0, 2.0).lo,
            (a.hi - a.lo) / 2.0
        );
    }
    #[test]
    fn bounded_sampling_preserves_endpoints() {
        let rows = decimate(1_000_000, POINT_CAP);
        assert_eq!(rows.len(), POINT_CAP);
        assert_eq!(rows[0], 0);
        assert_eq!(*rows.last().unwrap(), 999_999);
        assert!(rows.windows(2).all(|w| w[1] > w[0]));
        assert!(decimate(0, 0).is_empty());
        assert_eq!(decimate(4, 1), vec![0]);
    }
    #[test]
    fn aggregation_keeps_counts_and_original_rows() {
        let t = table((0..1000).map(|i| vec![Value::Int(i)]).collect());
        let p = prepare(&t, default_spec(&t));
        assert!(p.points.len() <= BIN_CAP);
        assert_eq!(p.points.iter().map(|p| p.count).sum::<usize>(), 1000);
        assert!(p.points.iter().all(|p| p.row < 1000));
        let t = table(vec![
            vec![Value::Text("a".into()), Value::Int(2)],
            vec![Value::Text("a".into()), Value::Int(3)],
        ]);
        let p = prepare(&t, default_spec(&t));
        assert_eq!(p.points[0].y, 5.0);
        assert_eq!(p.points[0].count, 2);
    }
    #[test]
    fn aggregation_caps_and_long_categories_are_explicit() {
        let t = table(
            (0..INPUT_CAP + 1)
                .map(|i| vec![Value::Int(i as i64)])
                .collect(),
        );
        let p = prepare(&t, default_spec(&t));
        assert!(p.sampled);
        assert_eq!(p.points.iter().map(|p| p.count).sum::<usize>(), INPUT_CAP);
        let prefix = "x".repeat(1000);
        let t = table(vec![
            vec![Value::Text(format!("{prefix}a")), Value::Int(2)],
            vec![Value::Text(format!("{prefix}b")), Value::Int(3)],
        ]);
        let p = prepare(&t, default_spec(&t));
        assert_eq!(p.points.len(), 2);
        assert!(p.points.iter().all(|p| p.label.len() <= 80));
        let t = table(
            (0..CATEGORY_CAP + 10)
                .map(|i| vec![Value::Text(i.to_string()), Value::Int(1)])
                .collect(),
        );
        let p = prepare(&t, default_spec(&t));
        assert_eq!(p.points.len(), CATEGORY_CAP);
        assert_eq!(p.omitted, 10);
    }
    #[test]
    fn invalid_log_values_and_extreme_ranges_never_make_nan_geometry() {
        let t = table(vec![
            vec![Value::Float(-1.0)],
            vec![Value::Int(0)],
            vec![Value::Int(10)],
        ]);
        let mut s = default_spec(&t);
        s.xs = Scale::Log;
        let p = prepare(&t, s);
        assert_eq!(p.omitted, 2);
        assert_eq!(p.points.iter().map(|p| p.count).sum::<usize>(), 1);
        let t = table(vec![
            vec![Value::Float(-f64::MAX)],
            vec![Value::Float(f64::MAX)],
        ]);
        let p = prepare(&t, default_spec(&t));
        assert!(p.points.is_empty());
        assert_eq!(p.omitted, 2);
        let a = Axis::from_values([f64::MAX].into_iter()).viewport(10.0, 0.25);
        assert!(a.lo.is_finite() && a.hi.is_finite());
        assert!(a.ticks(800.0, 100.0).iter().all(|v| v.is_finite()));
    }
}
