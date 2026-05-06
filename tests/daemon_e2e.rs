//! End-to-end: photo tree -> scheduler -> all stages done.
//! Uses MockLlmDescribeClient so the test runs offline.

use clepho::config::PipelineConfig;
use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::circuit_breaker::CircuitBreaker;
use clepho::pipeline::log::JsonlAppender;
use clepho::pipeline::scheduler::Scheduler;
use clepho::pipeline::stages::{
    exif::ExifStage,
    index::IndexStage,
    llm::{LlmDescribeClient, LlmStage},
    scan::ScanStage,
    thumb::ThumbStage,
    Stage,
};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

struct MockLlm;
impl LlmDescribeClient for MockLlm {
    fn describe_and_tag_image(
        &self,
        _: &Path,
        _: Option<&str>,
    ) -> anyhow::Result<(String, Vec<String>)> {
        Ok(("a sunset".into(), vec!["sunset".into()]))
    }
}

#[test]
fn daemon_pipeline_walks_photo_to_index_done() {
    let src = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();

    // Synthesise one fake jpeg
    let img: image::ImageBuffer<image::Rgb<u8>, _> =
        image::ImageBuffer::from_fn(32, 32, |_, _| image::Rgb([200u8, 100, 50]));
    let path = src.path().join("a.jpg");
    img.save_with_format(&path, image::ImageFormat::Jpeg)
        .unwrap();

    let conn = Connection::open_in_memory().unwrap();
    apply_v2_schema(&conn).unwrap();

    let scan = Arc::new(ScanStage::new(
        vec![src.path().to_path_buf()],
        vec!["jpg".into()],
    ));
    scan.discover(&conn, src.path()).unwrap();

    let stages: Vec<Arc<dyn Stage>> = vec![
        scan.clone(),
        Arc::new(ExifStage),
        Arc::new(ThumbStage::new(cache.path().to_path_buf(), 64)),
        Arc::new(LlmStage {
            client: Arc::new(MockLlm),
            global_prompt_override: None,
        }),
        Arc::new(IndexStage),
    ];

    let scheduler = Scheduler {
        stages,
        breaker: Arc::new(CircuitBreaker::new(3)),
        log: Arc::new(JsonlAppender::new(logs.path()).unwrap()),
        config: PipelineConfig::default(),
        clock: Arc::new(SystemClock),
        cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    // Each pass advances exactly the stages whose prereqs are met. Five
    // stages, fresh row, so 5 passes is the upper bound.
    for _ in 0..6 {
        scheduler.run_pass(&conn, None).unwrap();
    }

    let (s, e, t, l, i): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT scan_done_at, exif_done_at, thumb_done_at, llm_done_at, index_done_at
             FROM photos WHERE path LIKE '%a.jpg'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert!(
        s.is_some() && e.is_some() && t.is_some() && l.is_some() && i.is_some(),
        "all stages should be done; got scan={:?} exif={:?} thumb={:?} llm={:?} index={:?}",
        s,
        e,
        t,
        l,
        i
    );

    // Side-effects: photo got an AI description and the 'sunset' object linked.
    let (desc, src_kind): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT description, description_source FROM photos WHERE path LIKE '%a.jpg'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(desc.as_deref(), Some("a sunset"));
    assert_eq!(src_kind.as_deref(), Some("ai"));

    let object_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM photo_objects po JOIN objects o ON o.id=po.object_id
             WHERE o.name='sunset'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(object_count, 1);
}
