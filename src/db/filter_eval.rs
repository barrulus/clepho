//! Filter JSON evaluator (spec §3.5).
//! Used by smart-album membership recompute (index stage) and by the BrowseView
//! filter bar (Plan 4). Single source of truth for filter semantics.

use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub combinator: Combinator,
    pub clauses: Vec<Clause>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Combinator {
    And,
    Or,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "facet")]
#[serde(rename_all = "snake_case")]
pub enum Clause {
    People { op: SetOp, values: Vec<String> },
    Objects { op: SetOp, values: Vec<String> },
    Tags { op: SetOp, values: Vec<String> },
    Cameras { op: SetOp, values: Vec<String> },
    Events { op: SetOp, values: Vec<String> },
    Places { op: PlaceOp, value: PlaceValue },
    Dates { op: DateOp, value: DateValue },
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetOp {
    AnyOf,
    AllOf,
    NoneOf,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceOp {
    WithinKm,
    BoundingBox,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlaceValue {
    WithinKm {
        lat: f64,
        lon: f64,
        km: f64,
    },
    Bbox {
        min_lat: f64,
        min_lon: f64,
        max_lat: f64,
        max_lon: f64,
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOp {
    Between,
    Year,
    Month,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DateValue {
    Between { from: String, to: String },
    Year(i32),
    YearMonth { year: i32, month: u32 },
}

/// Evaluate a filter against one photo. Returns true if the photo matches.
#[allow(dead_code)]
pub fn matches_photo(conn: &Connection, photo_id: i64, filter: &Filter) -> Result<bool> {
    let results: Vec<bool> = filter
        .clauses
        .iter()
        .map(|c| matches_clause(conn, photo_id, c))
        .collect::<Result<Vec<_>>>()?;

    Ok(match filter.combinator {
        Combinator::And => results.iter().all(|b| *b),
        Combinator::Or => results.iter().any(|b| *b),
    })
}

fn matches_clause(conn: &Connection, photo_id: i64, clause: &Clause) -> Result<bool> {
    match clause {
        Clause::Objects { op, values } => match_set(
            conn,
            photo_id,
            *op,
            values,
            "SELECT o.name FROM photo_objects po JOIN objects o ON o.id=po.object_id
             WHERE po.photo_id=?1",
        ),
        Clause::Tags { op, values } => match_set(
            conn,
            photo_id,
            *op,
            values,
            "SELECT t.name FROM photo_user_tags pt JOIN user_tags t ON t.id=pt.tag_id
             WHERE pt.photo_id=?1",
        ),
        Clause::People { op, values } => match_set(
            conn,
            photo_id,
            *op,
            values,
            "SELECT p.name FROM faces f JOIN people p ON p.id=f.person_id
             WHERE f.photo_id=?1 AND p.name IS NOT NULL",
        ),
        Clause::Cameras { op, values } => {
            let cams: Vec<String> = conn
                .query_row(
                    "SELECT TRIM(COALESCE(camera_make,'') || ' ' || COALESCE(camera_model,''))
                     FROM photos WHERE id=?1",
                    rusqlite::params![photo_id],
                    |r| r.get::<_, String>(0),
                )
                .map(|s| if s.is_empty() { vec![] } else { vec![s] })
                .unwrap_or_default();
            Ok(eval_set(*op, values, &cams))
        }
        Clause::Events { op, values } => match_set(
            conn,
            photo_id,
            *op,
            values,
            "SELECT e.name FROM photo_events pe JOIN events e ON e.id=pe.event_id
             WHERE pe.photo_id=?1",
        ),
        Clause::Places { op, value } => match_place(conn, photo_id, *op, value),
        Clause::Dates { op, value } => match_date(conn, photo_id, *op, value),
    }
}

fn match_set(
    conn: &Connection,
    photo_id: i64,
    op: SetOp,
    values: &[String],
    sql: &str,
) -> Result<bool> {
    let mut stmt = conn.prepare(sql)?;
    let actual: Vec<String> = stmt
        .query_map(rusqlite::params![photo_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(eval_set(op, values, &actual))
}

fn eval_set(op: SetOp, values: &[String], actual: &[String]) -> bool {
    let actual_set: std::collections::HashSet<&str> = actual.iter().map(|s| s.as_str()).collect();
    match op {
        SetOp::AnyOf => values.iter().any(|v| actual_set.contains(v.as_str())),
        SetOp::AllOf => values.iter().all(|v| actual_set.contains(v.as_str())),
        SetOp::NoneOf => values.iter().all(|v| !actual_set.contains(v.as_str())),
    }
}

fn match_place(conn: &Connection, photo_id: i64, op: PlaceOp, value: &PlaceValue) -> Result<bool> {
    let coords: Option<(f64, f64)> = conn
        .query_row(
            "SELECT gps_lat, gps_lon FROM photos
             WHERE id=?1 AND gps_lat IS NOT NULL AND gps_lon IS NOT NULL",
            rusqlite::params![photo_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((lat, lon)) = coords else {
        return Ok(false);
    };

    match (op, value) {
        (
            PlaceOp::WithinKm,
            PlaceValue::WithinKm {
                lat: clat,
                lon: clon,
                km,
            },
        ) => Ok(haversine_km(lat, lon, *clat, *clon) <= *km),
        (
            PlaceOp::BoundingBox,
            PlaceValue::Bbox {
                min_lat,
                min_lon,
                max_lat,
                max_lon,
            },
        ) => Ok(lat >= *min_lat && lat <= *max_lat && lon >= *min_lon && lon <= *max_lon),
        _ => Err(anyhow!("place op/value mismatch")),
    }
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn match_date(conn: &Connection, photo_id: i64, op: DateOp, value: &DateValue) -> Result<bool> {
    let taken: Option<String> = conn
        .query_row(
            "SELECT taken_at FROM photos WHERE id=?1",
            rusqlite::params![photo_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let Some(t) = taken else {
        return Ok(false);
    };

    let dt: chrono::DateTime<chrono::Utc> = if let Ok(d) = chrono::DateTime::parse_from_rfc3339(&t)
    {
        d.with_timezone(&chrono::Utc)
    } else if let Ok(d) = chrono::NaiveDate::parse_from_str(&t, "%Y-%m-%d") {
        d.and_hms_opt(0, 0, 0).unwrap().and_utc()
    } else {
        return Err(anyhow!("bad taken_at {}", t));
    };

    Ok(match (op, value) {
        (DateOp::Between, DateValue::Between { from, to }) => {
            let f = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")?;
            let to = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")?;
            let d = dt.date_naive();
            d >= f && d <= to
        }
        (DateOp::Year, DateValue::Year(y)) => {
            dt.format("%Y").to_string().parse::<i32>().ok() == Some(*y)
        }
        (DateOp::Month, DateValue::YearMonth { year, month }) => {
            let yy = dt.format("%Y").to_string().parse::<i32>().ok();
            let mm = dt.format("%m").to_string().parse::<u32>().ok();
            yy == Some(*year) && mm == Some(*month)
        }
        _ => return Err(anyhow!("date op/value mismatch")),
    })
}
