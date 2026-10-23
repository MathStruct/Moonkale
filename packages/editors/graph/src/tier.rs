//! Source size is measured at open/reload, never used to change tiers during
//! a live change-feed refresh. Explicit overrides reload the view.
use moonkale_core::{Query, QueryResult, Source, SourceError, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tier {
    Exact,
    Approximate,
    Coarse,
}
impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Approximate => "approximate",
            Self::Coarse => "coarse",
        }
    }
    fn measured(nodes: usize) -> Self {
        if nodes < 1500 {
            Self::Exact
        } else if nodes <= 100_000 {
            Self::Approximate
        } else {
            Self::Coarse
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Auto,
    Detailed,
    Coarse,
}
impl Choice {
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Detailed => "detailed",
            Self::Coarse => "coarse",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "detailed" => Self::Detailed,
            "coarse" => Self::Coarse,
            _ => Self::Auto,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Measurement {
    pub tier: Tier,
    pub nodes: usize,
    pub exact: bool,
}

pub(crate) async fn load(
    source: &dyn Source,
    query: Query,
    choice: Choice,
    cached: Option<Measurement>,
) -> Result<(QueryResult, Measurement), SourceError> {
    // Query/schema/local views retain their explicit meaning.
    if !matches!(query, Query::All { .. }) {
        let result = source.query(query).await?;
        let measurement = Measurement {
            tier: Tier::measured(result.nodes.len()),
            nodes: result.nodes.len(),
            exact: !result.truncated,
        };
        return Ok((result, measurement));
    }
    if let Some(measurement) = cached {
        let query = if measurement.tier == Tier::Coarse {
            Query::Coarse {
                level: 1,
                region: None,
            }
        } else {
            query
        };
        return Ok((source.query(query).await?, measurement));
    }
    // A coarse response includes the complete source count. This doubles as
    // the size query, so a million-node source is never sent to the panel.
    let coarse = source
        .query(Query::Coarse {
            level: 1,
            region: None,
        })
        .await;
    match coarse {
        Ok(result) => {
            let nodes = result
                .table
                .as_ref()
                .and_then(|t| t.rows.first())
                .and_then(|r| r.first())
                .and_then(|v| {
                    if let Value::Int(n) = v {
                        usize::try_from(*n).ok()
                    } else {
                        None
                    }
                })
                .ok_or_else(|| SourceError::Invalid("coarse source count missing".into()))?;
            let tier = match choice {
                Choice::Coarse => Tier::Coarse,
                Choice::Detailed => {
                    if nodes < 1500 {
                        Tier::Exact
                    } else {
                        Tier::Approximate
                    }
                }
                Choice::Auto => Tier::measured(nodes),
            };
            let measurement = Measurement {
                tier,
                nodes,
                exact: true,
            };
            if tier == Tier::Coarse {
                Ok((result, measurement))
            } else {
                Ok((source.query(query).await?, measurement))
            }
        }
        Err(SourceError::Unsupported(_)) if choice != Choice::Coarse => {
            let result = source.query(query).await?;
            let measurement = Measurement {
                tier: Tier::measured(result.nodes.len()),
                nodes: result.nodes.len(),
                exact: !result.truncated,
            };
            Ok((result, measurement))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_boundaries_match_renderer_and_query_cap() {
        for (n, expected) in [
            (1499, Tier::Exact),
            (1500, Tier::Approximate),
            (100000, Tier::Approximate),
            (100001, Tier::Coarse),
        ] {
            assert_eq!(Tier::measured(n), expected);
        }
    }
}
