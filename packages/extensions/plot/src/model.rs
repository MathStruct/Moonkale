//! Bounded data preparation, finite scales, declarative plot specs, and SVG rendering, independent of the UI.
use moonkale_core::{
    DateTimeCell, Table, TableAggregateResult, TableFilter, TablePredicate, Value,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const INPUT_CAP: usize = 50_000;
pub const POINT_CAP: usize = 5_000;
pub const BIN_CAP: usize = 40;
pub const CATEGORY_CAP: usize = 100;
pub const PLOT_SPEC_EXTENSION: &str = ".plot.json";
pub const PLOT_SPEC_VERSION: u32 = 1;
pub const MAX_PLOT_SPEC_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    #[default]
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

fn category_key(v: &Value) -> String {
    format!(
        "{}\0{}",
        short(v),
        moonkale_core::NodeId::from_content(format!("{v:?}").as_bytes())
    )
}

/// Convert a source-computed [`TableAggregateResult`] into [`Prepared`], linking
/// buckets to any matching rows in the currently loaded `table` page.
pub fn prepare_aggregate(table: &Table, s: Spec, agg: &TableAggregateResult) -> Prepared {
    let mut out = Prepared {
        sampled: false,
        omitted: agg.omitted as usize,
        ..Default::default()
    };
    let n = table.rows.len().min(INPUT_CAP);
    if s.mark == Mark::Bar {
        let mut loaded_members: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, row) in table.rows.iter().take(n).enumerate() {
            let Some((x, y)) = row.get(s.x).zip(row.get(s.y)) else {
                continue;
            };
            if number(y).is_some() {
                loaded_members.entry(category_key(x)).or_default().push(i);
            }
        }
        out.bar_width = 0.8;
        for b in &agg.buckets {
            let key = category_key(&b.key);
            let members = loaded_members.remove(&key).unwrap_or_default();
            let row = members.first().copied().unwrap_or(0);
            let count = b.count as usize;
            let label = short(&b.key);
            if s.ys.transform(b.value).is_some() {
                let x = out.points.len() as f64;
                out.categories.push(label.clone());
                out.points.push(Point {
                    x,
                    y: b.value,
                    row,
                    members,
                    count,
                    label,
                });
            } else {
                out.omitted = out.omitted.saturating_add(count);
            }
        }
    } else if s.mark == Mark::Histogram {
        let (Some(lo), Some(hi)) = (agg.lo, agg.hi) else {
            return out;
        };
        if !(hi - lo).is_finite() || agg.buckets.is_empty() {
            // As in the client path: an axis cannot span a range that
            // overflows f64, so its rows are reported as omitted.
            let counted: u64 = agg.buckets.iter().map(|b| b.count).sum();
            out.omitted = out.omitted.saturating_add(counted as usize);
            return out;
        }
        let bins = agg.buckets.len();
        let width = if bins == 1 {
            1.0
        } else {
            (hi - lo) / bins as f64
        };
        out.bar_width = width * 0.9;
        let mut loaded_bins = vec![Vec::new(); bins];
        for (i, row) in table.rows.iter().take(n).enumerate() {
            if let Some(v) = row
                .get(s.x)
                .and_then(number)
                .and_then(|v| s.xs.transform(v))
            {
                let b = if bins == 1 {
                    0
                } else {
                    (((v - lo) / width).floor() as isize).clamp(0, (bins - 1) as isize) as usize
                };
                loaded_bins[b].push(i);
            }
        }
        for (i, b) in agg.buckets.iter().enumerate() {
            let count = b.count as usize;
            let members = std::mem::take(&mut loaded_bins[i]);
            let row = members.first().copied().unwrap_or(0);
            if count > 0 {
                if s.ys.transform(count as f64).is_some() {
                    let start = lo + i as f64 * width;
                    out.points.push(Point {
                        x: start + width / 2.0,
                        y: count as f64,
                        row,
                        members,
                        count,
                        label: format!("{} … {}", s.xs.label(start), s.xs.label(start + width)),
                    });
                } else {
                    out.omitted = out.omitted.saturating_add(count);
                }
            }
        }
    }
    out
}

/// Source reference inside a declarative `.plot.json` specification (PLT-04).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotSource {
    /// Relative path to the database or data source within the workspace folder.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// Table name in the source, if plotting a table.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub table: String,
    /// Optional source identifier hint.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
}

/// Predicate inside a [`PlotFilter`], with unknown fields refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "op",
    content = "value",
    rename_all = "snake_case"
)]
pub enum PlotPredicate {
    Eq(Value),
    Lt(Value),
    Le(Value),
    Gt(Value),
    Ge(Value),
    Contains(String),
    Prefix(String),
    IsNull,
}

impl From<&TablePredicate> for PlotPredicate {
    fn from(p: &TablePredicate) -> Self {
        match p {
            TablePredicate::Eq(v) => Self::Eq(v.clone()),
            TablePredicate::Lt(v) => Self::Lt(v.clone()),
            TablePredicate::Le(v) => Self::Le(v.clone()),
            TablePredicate::Gt(v) => Self::Gt(v.clone()),
            TablePredicate::Ge(v) => Self::Ge(v.clone()),
            TablePredicate::Contains(s) => Self::Contains(s.clone()),
            TablePredicate::Prefix(s) => Self::Prefix(s.clone()),
            TablePredicate::IsNull => Self::IsNull,
        }
    }
}

impl From<&PlotPredicate> for TablePredicate {
    fn from(p: &PlotPredicate) -> Self {
        match p {
            PlotPredicate::Eq(v) => Self::Eq(v.clone()),
            PlotPredicate::Lt(v) => Self::Lt(v.clone()),
            PlotPredicate::Le(v) => Self::Le(v.clone()),
            PlotPredicate::Gt(v) => Self::Gt(v.clone()),
            PlotPredicate::Ge(v) => Self::Ge(v.clone()),
            PlotPredicate::Contains(s) => Self::Contains(s.clone()),
            PlotPredicate::Prefix(s) => Self::Prefix(s.clone()),
            PlotPredicate::IsNull => Self::IsNull,
        }
    }
}

/// Filter inside a [`PlotQuery`], with unknown fields refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotFilter {
    pub column: String,
    pub predicate: PlotPredicate,
}

impl PlotFilter {
    pub fn from_core(filters: &[TableFilter]) -> Vec<Self> {
        filters
            .iter()
            .map(|f| Self {
                column: f.column.clone(),
                predicate: PlotPredicate::from(&f.predicate),
            })
            .collect()
    }

    pub fn to_core(filters: &[Self]) -> Vec<TableFilter> {
        filters
            .iter()
            .map(|f| TableFilter {
                column: f.column.clone(),
                predicate: TablePredicate::from(&f.predicate),
            })
            .collect()
    }
}

/// Query and active row filters inside a declarative `.plot.json` specification.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotQuery {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dialect: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<PlotFilter>,
}

/// Column encodings for X and Y axes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotEncodings {
    pub x: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub y: String,
}

/// Axis scale choices and aspect lock.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotScales {
    #[serde(default)]
    pub x: Scale,
    #[serde(default)]
    pub y: Scale,
    #[serde(default)]
    pub equal: bool,
}

/// Human-readable plot title and axis labels.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotLabels {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub x: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub y: String,
}

/// Versioned declarative plot specification stored as `*.plot.json` (PLT-04).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotFile {
    pub version: u32,
    #[serde(default)]
    pub source: PlotSource,
    #[serde(default)]
    pub query: PlotQuery,
    pub mark: Mark,
    pub encodings: PlotEncodings,
    #[serde(default)]
    pub scales: PlotScales,
    #[serde(default)]
    pub labels: PlotLabels,
}

impl PlotFile {
    /// Bounded parser for `*.plot.json`. Refuses oversized inputs, unknown fields,
    /// unsupported versions, path traversal, and invalid column/filter bounds.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw.len() > MAX_PLOT_SPEC_BYTES {
            return Err(format!("plot spec exceeds {MAX_PLOT_SPEC_BYTES} bytes"));
        }
        let spec: Self = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        spec.validate()?;
        Ok(spec)
    }

    /// Validate all structural and size bounds on this specification.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PLOT_SPEC_VERSION {
            return Err(format!(
                "unsupported plot spec version {} (expected {})",
                self.version, PLOT_SPEC_VERSION
            ));
        }
        let check_str = |s: &str, max: usize, name: &str| -> Result<(), String> {
            if s.len() > max || s.contains('\0') {
                return Err(format!("invalid {name}"));
            }
            Ok(())
        };
        check_str(&self.source.path, 1024, "source.path")?;
        check_str(&self.source.table, 256, "source.table")?;
        check_str(&self.source.id, 1024, "source.id")?;
        if self.source.path.split(['/', '\\']).any(|seg| seg == "..") {
            return Err("source.path must not contain '..'".into());
        }
        if self.source.path.is_empty()
            && self.source.table.is_empty()
            && self.source.id.is_empty()
            && self.query.text.is_empty()
        {
            return Err("plot spec requires a source or query".into());
        }
        check_str(&self.query.dialect, 64, "query.dialect")?;
        check_str(&self.query.text, 16 * 1024, "query.text")?;
        if self.query.filters.len() > 32 {
            return Err("too many query filters".into());
        }
        for f in &self.query.filters {
            if f.column.is_empty() {
                return Err("empty filter column".into());
            }
            check_str(&f.column, 256, "filter.column")?;
            match &f.predicate {
                PlotPredicate::Contains(s) | PlotPredicate::Prefix(s) => {
                    if s.is_empty() {
                        return Err("empty filter text".into());
                    }
                    check_str(s, 1024, "filter.value")?;
                }
                PlotPredicate::Eq(v)
                | PlotPredicate::Lt(v)
                | PlotPredicate::Le(v)
                | PlotPredicate::Gt(v)
                | PlotPredicate::Ge(v) => match v {
                    Value::Int(_) | Value::Bool(_) | Value::DateTime(_) => {}
                    Value::Text(s) => check_str(s, 1024, "filter.value")?,
                    Value::Float(n) if n.is_finite() => {}
                    _ => return Err("unsupported filter scalar".into()),
                },
                PlotPredicate::IsNull => {}
            }
        }
        if self.encodings.x.is_empty() {
            return Err("encodings.x is required".into());
        }
        check_str(&self.encodings.x, 256, "encodings.x")?;
        if self.mark != Mark::Histogram && self.encodings.y.is_empty() {
            return Err("encodings.y is required for this mark".into());
        }
        check_str(&self.encodings.y, 256, "encodings.y")?;
        check_str(&self.labels.title, 256, "labels.title")?;
        check_str(&self.labels.x, 256, "labels.x")?;
        check_str(&self.labels.y, 256, "labels.y")?;
        Ok(())
    }

    /// Format as canonical pretty JSON for git-friendly storage.
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        let mut out = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        out.push('\n');
        if out.len() > MAX_PLOT_SPEC_BYTES {
            return Err(format!("plot spec exceeds {MAX_PLOT_SPEC_BYTES} bytes"));
        }
        Ok(out)
    }

    /// Resolve a [`PlotFile`] against `table`'s columns to produce a runtime [`Spec`].
    pub fn to_spec(&self, table: &Table) -> Spec {
        let fallback = default_spec(table);
        let x = table
            .columns
            .iter()
            .position(|c| c == &self.encodings.x)
            .unwrap_or(fallback.x);
        let y = if self.encodings.y.is_empty() {
            x
        } else {
            table
                .columns
                .iter()
                .position(|c| c == &self.encodings.y)
                .unwrap_or(fallback.y)
        };
        Spec {
            x: x.min(table.columns.len().saturating_sub(1)),
            y: y.min(table.columns.len().saturating_sub(1)),
            mark: self.mark,
            xs: if self.mark == Mark::Bar {
                Scale::Linear
            } else {
                self.scales.x
            },
            ys: self.scales.y,
            equal: self.scales.equal,
        }
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Render a standalone SVG string from our own plot layout and renderer (no external dependencies).
pub fn render_svg(
    table: &Table,
    s: Spec,
    prep: &Prepared,
    labels: &PlotLabels,
    default_y_label: &str,
) -> String {
    let (w, h, l, r, t, b) = (640.0_f64, 360.0_f64, 64.0_f64, 22.0_f64, 18.0_f64, 44.0_f64);
    let (mut pw, mut ph) = (w - l - r, h - t - b);
    if s.equal {
        let side = pw.min(ph);
        pw = side;
        ph = side;
    }
    let tx = |v: f64| {
        if s.mark == Mark::Bar || s.mark == Mark::Histogram {
            Some(v)
        } else {
            s.xs.transform(v)
        }
    };
    let xa = if s.mark == Mark::Bar {
        Axis {
            lo: -0.6,
            hi: (prep.points.len().max(1) as f64) - 0.4,
        }
    } else if s.mark == Mark::Histogram {
        let half = prep.bar_width / 2.0;
        Axis::from_values(prep.points.iter().flat_map(|p| [p.x - half, p.x + half]))
    } else {
        Axis::from_values(prep.points.iter().filter_map(|p| tx(p.x)))
    };
    let ya = Axis::from_values(
        prep.points
            .iter()
            .filter_map(|p| s.ys.transform(p.y))
            .chain(
                (s.mark == Mark::Bar || s.mark == Mark::Histogram)
                    .then(|| s.ys.transform(0.0))
                    .flatten(),
            ),
    );
    let map_x = |v: f64| l + xa.unit(v) * pw;
    let map_y = |v: f64| t + (1.0 - ya.unit(v)) * ph;
    let zero_y = map_y(s.ys.transform(0.0).unwrap_or(ya.lo)).clamp(t, t + ph);
    let xticks = if s.mark == Mark::Bar {
        let step = (prep.categories.len() / 6).max(1);
        prep.categories
            .iter()
            .enumerate()
            .step_by(step)
            .map(|(i, c)| (map_x(i as f64), c.clone()))
            .collect::<Vec<_>>()
    } else {
        xa.ticks(pw, 84.0)
            .into_iter()
            .map(|v| (map_x(v), s.xs.label(v)))
            .collect()
    };
    let yticks: Vec<_> = ya
        .ticks(ph, 42.0)
        .into_iter()
        .map(|v| (map_y(v), s.ys.label(v)))
        .collect();
    let xlabel = if !labels.x.is_empty() {
        labels.x.clone()
    } else {
        table.columns.get(s.x).cloned().unwrap_or_default()
    };
    let ylabel = if !labels.y.is_empty() {
        labels.y.clone()
    } else if s.mark == Mark::Histogram {
        default_y_label.to_string()
    } else {
        table.columns.get(s.y).cloned().unwrap_or_default()
    };
    let line_pts = if s.mark == Mark::Line {
        prep.points
            .iter()
            .filter_map(|p| {
                Some(format!(
                    "{:.2},{:.2}",
                    map_x(tx(p.x)?),
                    map_y(s.ys.transform(p.y)?)
                ))
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        String::new()
    };

    let mut svg = String::with_capacity(4096);
    svg.push_str(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"mk-plot-svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" data-mark=\"{}\" data-points=\"{}\">\n",
        s.mark.id(),
        prep.points.len()
    ));
    if !labels.title.is_empty() {
        svg.push_str(&format!("  <title>{}</title>\n", xml_escape(&labels.title)));
    }
    svg.push_str(&format!(
        "  <rect class=\"mk-plot-frame\" x=\"{l}\" y=\"{t}\" width=\"{pw}\" height=\"{ph}\" fill=\"none\" stroke=\"currentColor\" stroke-opacity=\"0.25\"/>\n"
    ));
    for (x, label) in &xticks {
        if *x >= l && *x <= l + pw {
            svg.push_str(&format!(
                "  <line class=\"mk-plot-grid\" x1=\"{x:.2}\" y1=\"{t}\" x2=\"{x:.2}\" y2=\"{}\" stroke=\"currentColor\" stroke-opacity=\"0.12\"/>\n",
                t + ph
            ));
            svg.push_str(&format!(
                "  <text class=\"mk-plot-tick\" x=\"{x:.2}\" y=\"{}\" text-anchor=\"middle\">{}</text>\n",
                t + ph + 16.0,
                xml_escape(label)
            ));
        }
    }
    for (y, label) in &yticks {
        if *y >= t && *y <= t + ph {
            svg.push_str(&format!(
                "  <line class=\"mk-plot-grid\" x1=\"{l}\" y1=\"{y:.2}\" x2=\"{}\" y2=\"{y:.2}\" stroke=\"currentColor\" stroke-opacity=\"0.12\"/>\n",
                l + pw
            ));
            svg.push_str(&format!(
                "  <text class=\"mk-plot-tick\" x=\"{}\" y=\"{:.2}\" text-anchor=\"end\">{}</text>\n",
                l - 6.0,
                y + 4.0,
                xml_escape(label)
            ));
        }
    }
    if s.mark == Mark::Line && !line_pts.is_empty() {
        svg.push_str(&format!(
            "  <polyline class=\"mk-plot-line\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.75\" points=\"{line_pts}\"/>\n"
        ));
    }
    for p in &prep.points {
        let (Some(xv), Some(yv)) = (tx(p.x), s.ys.transform(p.y)) else {
            continue;
        };
        let (px, py) = (map_x(xv), map_y(yv));
        if !px.is_finite() || !py.is_finite() {
            continue;
        }
        if s.mark == Mark::Bar || s.mark == Mark::Histogram {
            let bw = ((map_x(xv + prep.bar_width / 2.0) - map_x(xv - prep.bar_width / 2.0)).abs())
                .max(2.0);
            let top = py.min(zero_y);
            let bh = (py - zero_y).abs().max(1.5);
            let bx = px - bw / 2.0;
            svg.push_str(&format!(
                "  <rect class=\"mk-plot-bar-mark\" data-row=\"{}\" x=\"{bx:.2}\" y=\"{top:.2}\" width=\"{bw:.2}\" height=\"{bh:.2}\"/>\n",
                p.row
            ));
        } else {
            svg.push_str(&format!(
                "  <circle class=\"mk-plot-dot\" data-row=\"{}\" cx=\"{px:.2}\" cy=\"{py:.2}\" r=\"3.5\"/>\n",
                p.row
            ));
        }
    }
    svg.push_str(&format!(
        "  <text class=\"mk-plot-axis-label\" x=\"{:.2}\" y=\"{}\" text-anchor=\"middle\">{}</text>\n",
        l + pw / 2.0,
        h - 6.0,
        xml_escape(&xlabel)
    ));
    svg.push_str(&format!(
        "  <text class=\"mk-plot-axis-label\" x=\"14\" y=\"{:.2}\" text-anchor=\"middle\" transform=\"rotate(-90 14 {:.2})\">{}</text>\n",
        t + ph / 2.0,
        t + ph / 2.0,
        xml_escape(&ylabel)
    ));
    svg.push_str("</svg>\n");
    svg
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
    #[test]
    fn plot_spec_round_trip_and_refusals() {
        let spec = PlotFile {
            version: PLOT_SPEC_VERSION,
            source: PlotSource {
                path: "data.sqlite".into(),
                table: "points".into(),
                id: String::new(),
            },
            query: PlotQuery {
                dialect: "sql".into(),
                text: "SELECT * FROM \"points\" LIMIT 1000".into(),
                filters: PlotFilter::from_core(&[TableFilter {
                    column: "group".into(),
                    predicate: TablePredicate::Contains("alpha".into()),
                }]),
            },
            mark: Mark::Scatter,
            encodings: PlotEncodings {
                x: "x".into(),
                y: "y".into(),
            },
            scales: PlotScales {
                x: Scale::Linear,
                y: Scale::Log,
                equal: true,
            },
            labels: PlotLabels {
                title: "Points <1> & \"2\"".into(),
                x: "X axis".into(),
                y: "Y axis".into(),
            },
        };
        let json = spec.to_json().unwrap();
        let parsed = PlotFile::parse(&json).unwrap();
        assert_eq!(parsed, spec);
        assert_eq!(
            PlotFilter::to_core(&parsed.query.filters),
            vec![TableFilter {
                column: "group".into(),
                predicate: TablePredicate::Contains("alpha".into()),
            }]
        );

        let t = Table {
            columns: vec!["x".into(), "y".into(), "group".into()],
            rows: vec![
                vec![
                    Value::Float(1.0),
                    Value::Float(10.0),
                    Value::Text("alpha".into()),
                ],
                vec![
                    Value::Float(2.0),
                    Value::Float(100.0),
                    Value::Text("beta".into()),
                ],
            ],
            truncated: false,
        };
        let runtime = parsed.to_spec(&t);
        assert_eq!(runtime.mark, Mark::Scatter);
        assert_eq!(runtime.x, 0);
        assert_eq!(runtime.y, 1);
        assert_eq!(runtime.ys, Scale::Log);
        assert!(runtime.equal);
        let prep = prepare(&t, runtime);
        let svg = render_svg(&t, runtime, &prep, &parsed.labels, "count");
        assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg"));
        assert!(svg.contains("<title>Points &lt;1&gt; &amp; &quot;2&quot;</title>"));
        assert!(svg.contains("data-mark=\"scatter\""));
        assert!(svg.ends_with("</svg>\n"));

        // Refusals: unknown fields, wrong version, path traversal, missing encodings, oversized input, NUL bytes.
        for bad in [
            r#"{"version":1,"source":{"path":"data.sqlite"},"mark":"scatter","encodings":{"x":"x","y":"y"},"unknown":1}"#.to_string(),
            r#"{"version":1,"source":{"path":"data.sqlite","extra":true},"mark":"scatter","encodings":{"x":"x","y":"y"}}"#.to_string(),
            r#"{"version":1,"source":{"path":"data.sqlite"},"query":{"bad":1},"mark":"scatter","encodings":{"x":"x","y":"y"}}"#.to_string(),
            r#"{"version":1,"source":{"path":"data.sqlite"},"mark":"scatter","encodings":{"x":"x","y":"y","z":"z"}}"#.to_string(),
            r#"{"version":1,"source":{"path":"data.sqlite"},"mark":"scatter","encodings":{"x":"x","y":"y"},"scales":{"bad":true}}"#.to_string(),
            r#"{"version":1,"source":{"path":"data.sqlite"},"mark":"scatter","encodings":{"x":"x","y":"y"},"labels":{"bad":"x"}}"#.to_string(),
            r#"{"version":2,"source":{"path":"data.sqlite"},"mark":"scatter","encodings":{"x":"x","y":"y"}}"#.to_string(),
            r#"{"version":0,"source":{"path":"data.sqlite"},"mark":"scatter","encodings":{"x":"x","y":"y"}}"#.to_string(),
            r#"{"version":1,"source":{"path":"../secret.sqlite"},"mark":"scatter","encodings":{"x":"x","y":"y"}}"#.to_string(),
            r#"{"version":1,"source":{},"mark":"scatter","encodings":{"x":"x","y":"y"}}"#.to_string(),
            r#"{"version":1,"source":{"path":"data.sqlite"},"mark":"scatter","encodings":{"x":"","y":"y"}}"#.to_string(),
            r#"{"version":1,"source":{"path":"data.sqlite"},"mark":"scatter","encodings":{"x":"x"}}"#.to_string(),
            r#"{"version":1,"source":{"path":"data.sqlite"},"mark":"histogram","encodings":{"x":"x\u0000"}}"#.to_string(),
            " ".repeat(MAX_PLOT_SPEC_BYTES + 1),
        ] {
            assert!(PlotFile::parse(&bad).is_err(), "expected refusal for {bad}");
        }
    }
    #[test]
    fn prepare_aggregate_matches_in_memory_prepare() {
        use moonkale_core::TableAggregateBucket;
        let t = Table {
            columns: vec!["cat".into(), "val".into()],
            rows: vec![
                vec![Value::Text("a".into()), Value::Int(2)],
                vec![Value::Text("b".into()), Value::Int(5)],
                vec![Value::Text("a".into()), Value::Int(3)],
            ],
            truncated: false,
        };
        let bar_spec = Spec {
            x: 0,
            y: 1,
            mark: Mark::Bar,
            xs: Scale::Linear,
            ys: Scale::Linear,
            equal: false,
        };
        let agg = TableAggregateResult {
            buckets: vec![
                TableAggregateBucket {
                    key: Value::Text("a".into()),
                    x_mid: 0.0,
                    value: 5.0,
                    count: 2,
                },
                TableAggregateBucket {
                    key: Value::Text("b".into()),
                    x_mid: 0.0,
                    value: 5.0,
                    count: 1,
                },
            ],
            total_rows: 3,
            omitted: 0,
            lo: None,
            hi: None,
        };
        assert_eq!(prepare_aggregate(&t, bar_spec, &agg), prepare(&t, bar_spec));
    }
}
