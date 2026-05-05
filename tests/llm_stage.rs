use clepho::db::{apply_v2_schema, FixedClock};
use clepho::pipeline::stages::{
    llm::{LlmDescribeClient, LlmStage},
    Stage, StageOutcome,
};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

struct MockLlm {
    desc: String,
    tags: Vec<String>,
    fail: Option<String>,
    captured_prompt: std::sync::Mutex<Option<String>>,
}

impl MockLlm {
    fn new(desc: &str, tags: Vec<&str>) -> Self {
        Self {
            desc: desc.into(),
            tags: tags.into_iter().map(String::from).collect(),
            fail: None,
            captured_prompt: std::sync::Mutex::new(None),
        }
    }
    fn failing(msg: &str) -> Self {
        Self {
            desc: String::new(),
            tags: vec![],
            fail: Some(msg.into()),
            captured_prompt: std::sync::Mutex::new(None),
        }
    }
}

impl LlmDescribeClient for MockLlm {
    fn describe_and_tag_image(
        &self,
        _p: &Path,
        custom_prompt: Option<&str>,
    ) -> anyhow::Result<(String, Vec<String>)> {
        *self.captured_prompt.lock().unwrap() = custom_prompt.map(String::from);
        if let Some(e) = &self.fail {
            anyhow::bail!("{}", e);
        }
        Ok((self.desc.clone(), self.tags.clone()))
    }
    fn embedding_model_name(&self) -> &'static str {
        "mock-embed"
    }
    fn text_embedding(&self, _t: &str) -> anyhow::Result<Option<Vec<f32>>> {
        Ok(Some(vec![0.1, 0.2, 0.3]))
    }
}

fn setup_db_with_pending() -> (Connection, i64) {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at)
         VALUES ('/tmp/p.jpg', '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z')",
        [],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    (c, id)
}

#[test]
fn llm_stage_writes_description_and_objects_with_ai_provenance() {
    let (c, id) = setup_db_with_pending();
    let mock = Arc::new(MockLlm::new(
        "A sunset over Rome",
        vec!["sunset", "rome"],
    ));
    let stage = LlmStage {
        client: mock.clone(),
        global_prompt_override: None,
    };

    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    let out = stage.process_one(&c, &pending[0], &clk).unwrap();
    assert!(matches!(out, StageOutcome::Ok));

    let (desc, src): (Option<String>, Option<String>) = c
        .query_row(
            "SELECT description, description_source FROM photos WHERE id=?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(desc.as_deref(), Some("A sunset over Rome"));
    assert_eq!(src.as_deref(), Some("ai"));

    let objects: Vec<(String, String)> = c
        .prepare(
            "SELECT o.name, po.source FROM photo_objects po
             JOIN objects o ON o.id=po.object_id WHERE po.photo_id=?1",
        )
        .unwrap()
        .query_map(rusqlite::params![id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(objects.len(), 2);
    assert!(objects.iter().all(|(_, s)| s == "ai"));

    // Embedding row written
    let (kind, dims): (String, i64) = c
        .query_row(
            "SELECT kind, dims FROM embeddings WHERE photo_id=?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind, "text");
    assert_eq!(dims, 3);
}

#[test]
fn llm_stage_does_not_overwrite_user_description() {
    let (c, id) = setup_db_with_pending();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    c.execute(
        "UPDATE photos SET description='User wrote this',
           description_source='user', description_confirmed_at='2026-05-04T11:00:00Z' WHERE id=?1",
        rusqlite::params![id],
    )
    .unwrap();

    let mock = Arc::new(MockLlm::new("AI version", vec![]));
    let stage = LlmStage {
        client: mock,
        global_prompt_override: None,
    };
    let pending = stage.pending(&c, None, 10).unwrap();
    let _ = stage.process_one(&c, &pending[0], &clk).unwrap();

    let desc: String = c
        .query_row(
            "SELECT description FROM photos WHERE id=?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(desc, "User wrote this");
}

#[test]
fn llm_stage_classifies_unreachable_error() {
    let (c, _id) = setup_db_with_pending();
    let mock = Arc::new(MockLlm::failing("connection refused"));
    let stage = LlmStage {
        client: mock,
        global_prompt_override: None,
    };
    let pending = stage.pending(&c, None, 10).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    let out = stage.process_one(&c, &pending[0], &clk).unwrap();
    match out {
        StageOutcome::Err { error_class, .. } => assert_eq!(error_class, "llm_unreachable"),
        _ => panic!("expected error"),
    }
}

#[test]
fn llm_stage_classifies_timeout_and_bad_json() {
    let (c, _id) = setup_db_with_pending();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");

    let stage = LlmStage {
        client: Arc::new(MockLlm::failing("read timeout after 30s")),
        global_prompt_override: None,
    };
    let p = stage.pending(&c, None, 10).unwrap();
    match stage.process_one(&c, &p[0], &clk).unwrap() {
        StageOutcome::Err { error_class, .. } => assert_eq!(error_class, "llm_timeout"),
        _ => panic!(),
    }

    // Reset llm_error so pending is non-empty again
    c.execute(
        "UPDATE photos SET llm_error=NULL, llm_done_at=NULL WHERE id=1",
        [],
    )
    .unwrap();

    let stage = LlmStage {
        client: Arc::new(MockLlm::failing("invalid json from provider")),
        global_prompt_override: None,
    };
    let p = stage.pending(&c, None, 10).unwrap();
    match stage.process_one(&c, &p[0], &clk).unwrap() {
        StageOutcome::Err { error_class, .. } => assert_eq!(error_class, "llm_bad_json"),
        _ => panic!(),
    }
}

#[test]
fn llm_stage_uses_folder_prompt_when_set() {
    let (c, _id) = setup_db_with_pending();
    c.execute(
        "INSERT INTO folder_prompts(path, custom_prompt) VALUES ('/tmp', 'pet photos')",
        [],
    )
    .unwrap();

    let mock = Arc::new(MockLlm::new("a dog", vec![]));
    let stage = LlmStage {
        client: mock.clone(),
        global_prompt_override: Some("global default".into()),
    };
    let pending = stage.pending(&c, None, 10).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    stage.process_one(&c, &pending[0], &clk).unwrap();

    assert_eq!(
        mock.captured_prompt.lock().unwrap().as_deref(),
        Some("pet photos"),
        "folder_prompts row should win over global override"
    );
}

#[test]
fn llm_stage_falls_back_to_global_when_no_folder_prompt() {
    let (c, _id) = setup_db_with_pending();

    let mock = Arc::new(MockLlm::new("x", vec![]));
    let stage = LlmStage {
        client: mock.clone(),
        global_prompt_override: Some("global default".into()),
    };
    let pending = stage.pending(&c, None, 10).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    stage.process_one(&c, &pending[0], &clk).unwrap();

    assert_eq!(
        mock.captured_prompt.lock().unwrap().as_deref(),
        Some("global default")
    );
}
