use clepho::db::{apply_v2_schema, filter_eval::*};
use rusqlite::Connection;

fn db_with_photo() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, gps_lat, gps_lon, taken_at, camera_make, camera_model)
         VALUES ('p.jpg', 41.89, 12.49, '2024-06-12T19:23:45+02:00', 'Sony', 'A7iv')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();
    c.execute(
        "INSERT INTO photo_objects(photo_id, object_id, source) VALUES (1,1,'ai')",
        [],
    )
    .unwrap();
    c
}

#[test]
fn objects_anyof_matches() {
    let c = db_with_photo();
    let f = Filter {
        combinator: Combinator::And,
        clauses: vec![Clause::Objects {
            op: SetOp::AnyOf,
            values: vec!["sunset".into()],
        }],
    };
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn objects_anyof_misses() {
    let c = db_with_photo();
    let f = Filter {
        combinator: Combinator::And,
        clauses: vec![Clause::Objects {
            op: SetOp::AnyOf,
            values: vec!["snow".into()],
        }],
    };
    assert!(!matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn objects_noneof_matches_when_absent() {
    let c = db_with_photo();
    let f = Filter {
        combinator: Combinator::And,
        clauses: vec![Clause::Objects {
            op: SetOp::NoneOf,
            values: vec!["snow".into()],
        }],
    };
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn places_within_km_matches() {
    let c = db_with_photo();
    let f = Filter {
        combinator: Combinator::And,
        clauses: vec![Clause::Places {
            op: PlaceOp::WithinKm,
            value: PlaceValue::WithinKm {
                lat: 41.9,
                lon: 12.5,
                km: 50.0,
            },
        }],
    };
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn places_within_km_misses() {
    let c = db_with_photo();
    let f = Filter {
        combinator: Combinator::And,
        clauses: vec![Clause::Places {
            op: PlaceOp::WithinKm,
            value: PlaceValue::WithinKm {
                lat: 0.0,
                lon: 0.0,
                km: 50.0,
            },
        }],
    };
    assert!(!matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn date_between_matches() {
    let c = db_with_photo();
    let f = Filter {
        combinator: Combinator::And,
        clauses: vec![Clause::Dates {
            op: DateOp::Between,
            value: DateValue::Between {
                from: "2024-01-01".into(),
                to: "2024-12-31".into(),
            },
        }],
    };
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn and_combinator() {
    let c = db_with_photo();
    let f = Filter {
        combinator: Combinator::And,
        clauses: vec![
            Clause::Objects {
                op: SetOp::AnyOf,
                values: vec!["sunset".into()],
            },
            Clause::Dates {
                op: DateOp::Year,
                value: DateValue::Year(2024),
            },
        ],
    };
    assert!(matches_photo(&c, 1, &f).unwrap());

    let f2 = Filter {
        combinator: Combinator::And,
        clauses: vec![
            Clause::Objects {
                op: SetOp::AnyOf,
                values: vec!["sunset".into()],
            },
            Clause::Dates {
                op: DateOp::Year,
                value: DateValue::Year(2023),
            },
        ],
    };
    assert!(!matches_photo(&c, 1, &f2).unwrap());
}

#[test]
fn or_combinator() {
    let c = db_with_photo();
    let f = Filter {
        combinator: Combinator::Or,
        clauses: vec![
            Clause::Objects {
                op: SetOp::AnyOf,
                values: vec!["snow".into()],
            },
            Clause::Dates {
                op: DateOp::Year,
                value: DateValue::Year(2024),
            },
        ],
    };
    assert!(matches_photo(&c, 1, &f).unwrap());
}

#[test]
fn property_filter_roundtrip_serde() {
    use proptest::prelude::*;
    // Constrain km to integer-valued f64 — arbitrary doubles can drift by one
    // ulp through JSON because the textual repr isn't always the canonical
    // shortest. Integer-valued doubles round-trip exactly.
    proptest!(|(years in 2000i32..2030, km_int in 1u32..1000)| {
        let km = km_int as f64;
        let f = Filter {
            combinator: Combinator::And,
            clauses: vec![
                Clause::Dates { op: DateOp::Year, value: DateValue::Year(years) },
                Clause::Places {
                    op: PlaceOp::WithinKm,
                    value: PlaceValue::WithinKm { lat: 0.0, lon: 0.0, km }
                },
            ],
        };
        let s = serde_json::to_string(&f).unwrap();
        let f2: Filter = serde_json::from_str(&s).unwrap();
        prop_assert_eq!(format!("{:?}", f), format!("{:?}", f2));
    });
}
