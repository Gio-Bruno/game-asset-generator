#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

struct Api {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    _tmp: tempfile::TempDir,
    next: u64,
    notifications: Vec<Value>,
}
impl Api {
    fn new(mode: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let fake = tmp.path().join("codex");
        std::fs::write(&fake, include_str!("fixtures/fake_codex.py")).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_asset-forge"))
            .args([
                "--data-dir",
                tmp.path().join("data").to_str().unwrap(),
                "serve",
            ])
            .env("ASSET_FORGE_CODEX", fake)
            .env("FORGE_TEST_MODE", mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            _tmp: tmp,
            next: 0,
            notifications: vec![],
        }
    }
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        writeln!(
            self.input,
            "{}",
            json!({"id":self.next,"method":method,"params":params})
        )
        .unwrap();
        self.input.flush().unwrap();
        loop {
            let mut line = String::new();
            assert!(self.output.read_line(&mut line).unwrap() > 0, "API closed");
            let value: Value = serde_json::from_str(&line).unwrap();
            if value["method"] == "events/notification" {
                self.notifications.push(value["params"].clone());
            }
            if value["id"] == self.next {
                return value;
            }
        }
    }
    fn project(&mut self) -> String {
        self.call("projects/create",json!({"name":"Fixture Game","style":{"name":"Ink","description":"Pixel art with forest greens"}}))["result"]["id"].as_str().unwrap().into()
    }
    fn create(&mut self, p: &str) -> Value {
        self.call("jobs/create",json!({"projectId":p,"idempotencyKey":"intent-1","prompt":"A forest scout","width":64,"height":96,"transparentBackground":true}))["result"].clone()
    }
    fn wait(&mut self, id: &str) -> Value {
        for _ in 0..200 {
            let job = self.call("jobs/get", json!({"id":id}))["result"].clone();
            if !["QUEUED", "RUNNING"].contains(&job["status"].as_str().unwrap()) {
                return job;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("Fixture job never completed");
    }
}
impl Drop for Api {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn guide_batch_creates_independent_named_assets_and_replays_once() {
    let mut api = Api::new("guide-batch");
    let project = api.project();
    let mut subjects = vec![];
    for (name, kind) in [
        ("Scout", "CHARACTER"),
        ("Watchtower", "STRUCTURE"),
        ("Forest", "SCENE"),
    ] {
        subjects.push(
            api.call(
                "characters/create",
                json!({"projectId":project,"name":name,"description":name,"kind":kind}),
            )["result"]
                .clone(),
        );
    }
    let request = json!({"requestId":"batch-guide","projectId":project,"message":"Render the scout, tower and forest as three separate assets","allowGeneration":true});
    let started = api.call("assistant/message", request.clone())["result"].clone();
    let done = api.wait_guide(started["id"].as_str().unwrap());
    assert_eq!(done["status"], "READY");
    assert_eq!(done["turnJobCount"], 3);
    for id in done["generatedJobIds"].as_array().unwrap() {
        let job = api.wait(id.as_str().unwrap());
        assert_eq!(job["status"], "SUCCEEDED");
        assert!(
            job["request"]["prompt"]
                .as_str()
                .unwrap()
                .starts_with("Render only")
        );
    }
    let assets = api.call("assets/list", json!({"projectId":project}))["result"]["data"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(assets.len(), 3);
    let mut paths = std::collections::HashSet::new();
    for subject in subjects {
        let asset = assets
            .iter()
            .find(|a| a["characterId"] == subject["id"])
            .unwrap();
        assert_eq!(asset["name"], subject["name"]);
        assert!(paths.insert(asset["path"].as_str().unwrap()));
        assert!(std::path::Path::new(asset["path"].as_str().unwrap()).exists());
        let saved = api.call("characters/get", json!({"id":subject["id"]}))["result"].clone();
        assert_eq!(
            saved["referenceAssetIds"],
            json!([asset["id"]]),
            "Each identity uses its own image for later variations"
        );
        let out = api
            ._tmp
            .path()
            .join(format!("{}.png", subject["name"].as_str().unwrap()));
        assert!(
            api.call("assets/export", json!({"id":asset["id"],"path":out}))
                .get("result")
                .is_some()
        );
    }
    assert_eq!(
        api.call("assistant/message", request)["result"]["id"],
        started["id"]
    );
    assert_eq!(
        api.call("jobs/list", json!({"projectId":project}))["result"]["pagination"]["totalItems"],
        3
    );
}

fn batch_request(api: &mut Api, project: &str) -> Value {
    let mut items = vec![];
    for name in ["Arrow tower", "Cannon tower", "Magic tower"] {
        let subject = api.call(
            "characters/create",
            json!({"projectId":project,"name":name,"description":name,"kind":"STRUCTURE"}),
        )["result"]["id"]
            .clone();
        items.push(json!({"characterId":subject,"prompt":"Render this tower alone","width":64,"height":64}));
    }
    json!({"projectId":project,"idempotencyKey":"three-towers","items":items})
}

#[test]
fn cancelling_one_batch_item_keeps_other_jobs_and_exports() {
    let mut api = Api::new("success");
    let project = api.project();
    let request = batch_request(&mut api, &project);
    let batch = api.call("jobs/batch/create", request.clone())["result"].clone();
    let ids = batch["jobIds"].as_array().unwrap();
    api.call("jobs/cancel", json!({"id":ids[1]}));
    assert_eq!(api.wait(ids[1].as_str().unwrap())["status"], "CANCELLED");
    for i in [0, 2] {
        let job = api.wait(ids[i].as_str().unwrap());
        assert_eq!(job["status"], "SUCCEEDED");
        assert_eq!(job["assetIds"].as_array().unwrap().len(), 1);
    }
    assert_eq!(api.call("jobs/batch/create", request)["result"], batch);
    assert_eq!(
        api.call("assets/list", json!({"projectId":project}))["result"]["pagination"]["totalItems"],
        2
    );
}

#[test]
fn one_shot_batch_waits_for_remaining_images_after_one_failure() {
    let mut api = Api::new("batch-partial");
    let project = api.project();
    let request = batch_request(&mut api, &project);
    api.child.kill().unwrap();
    api.child.wait().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_asset-forge"))
        .arg("--data-dir")
        .arg(api._tmp.path().join("data"))
        .args(["call", "jobs/batch/create"])
        .arg(request.to_string())
        .env("ASSET_FORGE_CODEX", api._tmp.path().join("codex"))
        .env("FORGE_TEST_MODE", "batch-partial")
        .output()
        .unwrap();
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let jobs = result["result"]["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs.iter().filter(|j| j["status"] == "FAILED").count(), 1);
    assert_eq!(
        jobs.iter().filter(|j| j["status"] == "SUCCEEDED").count(),
        2
    );
    for job in jobs.iter().filter(|j| j["status"] == "SUCCEEDED") {
        assert_eq!(job["assetIds"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn full_stdio_generation_replays_without_duplicate_assets() {
    let mut api = Api::new("success");
    let project = api.project();
    let created = api.create(&project);
    let id = created["id"].as_str().unwrap();
    let job = api.wait(id);
    assert_eq!(job["status"], "SUCCEEDED");
    let duplicate = api.create(&project);
    assert_eq!(duplicate["id"], id);
    assert_eq!(duplicate["status"], "SUCCEEDED");
    let assets = api.call("assets/list", json!({"projectId":project}));
    let data = assets["result"]["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["width"], 64);
    assert_eq!(data[0]["height"], 96);
    assert_eq!(data[0]["hasAlpha"], true);
    assert_eq!(api.call("jobs/create",json!({"projectId":project,"idempotencyKey":"intent-1","prompt":"Changed","width":64,"height":96,"transparentBackground":true}))["error"]["code"],"IDEMPOTENCY_CONFLICT");
}

#[test]
fn generated_animation_extracts_six_frames_and_replays_one_job() {
    let mut api = Api::new("animation");
    let project = api.project();
    let character = api.call(
        "characters/create",
        json!({"projectId":project,"name":"Mira","description":"Forest scout"}),
    )["result"]["id"]
        .clone();
    let request = json!({"projectId":project,"characterId":character,"idempotencyKey":"walk-1","config":{"name":"Walk","motion":"WALK","frameWidth":16,"frameHeight":16}});
    let created = api.call("animations/create", request.clone())["result"].clone();
    let id = created["id"].as_str().unwrap();
    let job = api.wait(id);
    assert_eq!(job["status"], "SUCCEEDED");
    assert_eq!(job["request"]["animation"]["motion"], "WALK");
    let clip = api.call("animations/get", json!({"id":id}))["result"].clone();
    assert_eq!(clip["frames"].as_array().unwrap().len(), 6);
    assert!(std::path::Path::new(clip["previewPath"].as_str().unwrap()).exists());
    assert_eq!(api.call("animations/create", request)["result"]["id"], id);
    assert_eq!(
        api.call("assets/list", json!({"projectId":project}))["result"]["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    api.call(
        "animations/timing/update",
        json!({"id":id,"fps":12,"isLooping":false}),
    );
    assert_eq!(
        api.call("jobs/get", json!({"id":id}))["result"]["request"]["animation"]["fps"],
        8
    );
    let export = api._tmp.path().join("walk.zip");
    assert!(
        api.call("animations/export", json!({"id":id,"path":export}))
            .get("result")
            .is_some()
    );
}

#[test]
fn guide_animation_uses_the_shared_generation_budget() {
    let mut api = Api::new("guide-animation");
    let started=api.call("assistant/message",json!({"requestId":"animate-guide","message":"Make a forest scout and a walk cycle","allowGeneration":true}));
    let id = started["result"]["id"].as_str().unwrap();
    let question = api.wait_guide(id);
    api.answer_setup(&question, false);
    let mut done = Value::Null;
    for _ in 0..200 {
        done = api.call("assistant/get", json!({"id":id}))["result"].clone();
        if done["status"] != "THINKING" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(done["status"], "READY");
    assert_eq!(done["turnJobCount"], 1);
    let jobs = done["generatedJobIds"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    let job = api.wait(jobs[0].as_str().unwrap());
    assert_eq!(job["status"], "SUCCEEDED");
    assert_eq!(
        api.call("animations/get", json!({"id":jobs[0]}))["result"]["frames"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}
#[test]
fn provider_failure_and_invalid_pixels_are_not_success() {
    for (mode, code) in [
        ("failed", "GENERATION_FAILED"),
        ("invalid-image", "INVALID_IMAGE"),
        ("no-image", "NO_IMAGE_GENERATED"),
        ("unavailable", "IMAGE_GENERATION_UNAVAILABLE"),
    ] {
        let mut api = Api::new(mode);
        let project = api.project();
        let created = api.create(&project);
        let job = api.wait(created["id"].as_str().unwrap());
        assert_eq!(job["status"], "FAILED");
        assert_eq!(job["error"]["code"], code);
    }
}
#[test]
fn disconnect_is_unknown_and_cancellation_interrupts() {
    let mut api = Api::new("disconnect");
    let p = api.project();
    let job = api.create(&p);
    assert_eq!(api.wait(job["id"].as_str().unwrap())["status"], "UNKNOWN");
    let mut api = Api::new("slow");
    let p = api.project();
    let job = api.create(&p);
    std::thread::sleep(std::time::Duration::from_millis(200));
    api.call("jobs/cancel", json!({"id":job["id"]}));
    assert_eq!(api.wait(job["id"].as_str().unwrap())["status"], "CANCELLED");
}

impl Api {
    fn answer_setup(&mut self, session: &Value, skipped: bool) {
        assert_eq!(session["status"], "READY", "{session}");
        assert!(session["pendingQuestion"].is_object(), "{session}");
        let input = json!({"requestId":format!("answer-{}",session["id"].as_str().unwrap()),"sessionId":session["id"],"projectId":session["projectId"],"message":"My choice","questionAnswer":{"questionId":session["pendingQuestion"]["id"],"optionId":if skipped {Value::Null} else {session["pendingQuestion"]["options"][0]["id"].clone()},"skipped":skipped}});
        let accepted = self.call("assistant/message", input.clone());
        assert_eq!(accepted["result"]["id"], session["id"], "{accepted}");
        assert_eq!(
            self.call("assistant/message", input)["result"]["id"],
            session["id"]
        );
    }
    fn wait_guide(&mut self, id: &str) -> Value {
        for _ in 0..200 {
            let session = self.call("assistant/get", json!({"id":id}))["result"].clone();
            if session["status"] != "THINKING" {
                return session;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("Fixture guide never completed");
    }
}
#[test]
fn guide_actions_are_scoped_and_replayed_without_duplicate_characters_or_images() {
    for permitted in [false, true] {
        let mut api = Api::new("guide-generate");
        let input = json!({"requestId":"setup-1","message":"Set up a woodland game and make Mira.","allowGeneration":permitted});
        let accepted = api.call("assistant/message", input.clone())["result"].clone();
        let id = accepted["id"].as_str().unwrap();
        let question = api.wait_guide(id);
        assert_eq!(
            api.call("projects/list", json!({}))["result"]["data"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        api.answer_setup(&question, !permitted);
        let session = api.wait_guide(id);
        assert_eq!(session["status"], "READY", "{session}");
        let project = session["projectId"].as_str().unwrap();
        let characters =
            api.call("characters/list", json!({"projectId":project}))["result"]["data"]
                .as_array()
                .unwrap()
                .clone();
        assert_eq!(characters.len(), 1);
        assert_eq!(
            api.call("projects/get", json!({"id":project}))["result"]["style"]["name"],
            "Woodland ink"
        );
        assert_eq!(
            api.call("assistant/message", input.clone())["result"]["id"],
            id
        );
        assert_eq!(
            api.call("projects/list", json!({}))["result"]["data"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            api.call("characters/list", json!({"projectId":project}))["result"]["data"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let jobs = api.call("jobs/list", json!({"projectId":project}))["result"]["data"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(jobs.len(), usize::from(permitted));
        if permitted {
            let job = api.wait(jobs[0]["id"].as_str().unwrap());
            assert_eq!(job["status"], "SUCCEEDED");
            assert_eq!(session["generatedJobIds"].as_array().unwrap().len(), 1);
        }
        let mut changed = input;
        changed["message"] = json!("A different request");
        assert_eq!(
            api.call("assistant/message", changed)["error"]["code"],
            "IDEMPOTENCY_CONFLICT"
        );
        let foreign = api.project();
        assert_eq!(api.call("assistant/message",json!({"requestId":"scope-2","sessionId":id,"projectId":foreign,"message":"Change another game"}))["error"]["code"],"VALIDATION_ERROR");
    }
}

#[test]
fn chat_attachments_supply_pixels_and_revision_references_for_one_message() {
    let mut api = Api::new("guide-reference");
    let project = api.project();
    let original = api.create(&project);
    let original = api.wait(original["id"].as_str().unwrap());
    assert_eq!(original["status"], "SUCCEEDED");
    let reference = original["assetIds"][0].clone();
    let input = json!({"requestId":"revision-1","projectId":project,"message":"Give this structure a red roof","allowGeneration":true,"referenceAssetIds":[reference]});
    let accepted = api.call("assistant/message", input.clone())["result"].clone();
    let id = accepted["id"].as_str().unwrap();
    let done = api.wait_guide(id);
    assert_eq!(done["status"], "READY", "{done}");
    assert_eq!(done["referenceAssetIds"], json!([reference]));
    assert_eq!(done["messages"][0]["referenceAssetIds"], json!([reference]));
    assert_eq!(done["turnJobCount"], 1);
    let job = api.wait(done["generatedJobIds"][0].as_str().unwrap());
    assert_eq!(job["status"], "SUCCEEDED", "{job}");
    assert_eq!(job["request"]["referenceAssetIds"], json!([reference]));
    assert_eq!(job["referenceAssetIds"], json!([reference]));
    assert_eq!(
        api.call("assistant/message", input.clone())["result"]["id"],
        id
    );
    let mut changed = input;
    changed["referenceAssetIds"] = json!([]);
    assert_eq!(
        api.call("assistant/message", changed)["error"]["code"],
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(
        api.call("projects/get", json!({"id":project}))["result"]["style"]["referenceAssetIds"],
        json!([])
    );

    let next = api.call(
        "assistant/message",
        json!({"requestId":"revision-2","sessionId":id,"message":"What's next?"}),
    )["result"]
        .clone();
    let next = api.wait_guide(next["id"].as_str().unwrap());
    assert_eq!(next["status"], "READY", "{next}");
    assert_eq!(next["referenceAssetIds"], json!([]));
    assert_eq!(next["turnJobCount"], 0);
    assert_eq!(
        api.call("jobs/list", json!({"projectId":project}))["result"]["data"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn chat_new_game_maps_explicit_revision_ids_without_repeating_creation() {
    let mut api = Api::new("guide-new-game-reference");
    let original_project = api.project();
    let original = api.create(&original_project);
    let original = api.wait(original["id"].as_str().unwrap());
    assert_eq!(original["status"], "SUCCEEDED");
    let reference = original["assetIds"][0].clone();
    let input = json!({"requestId":"new-game-revision-1","projectId":original_project,"message":"Create a new game using this tower, and give it a red roof","allowGeneration":true,"referenceAssetIds":[reference]});
    let accepted = api.call("assistant/message", input.clone())["result"].clone();
    let id = accepted["id"].as_str().unwrap();
    let question = api.wait_guide(id);
    assert_eq!(
        api.call("projects/list", json!({}))["result"]["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    api.answer_setup(&question, false);
    let done = api.wait_guide(id);
    assert_eq!(done["status"], "READY", "{done}");
    let new_project = done["projectId"].clone();
    let copied = done["referenceAssetIds"][0].clone();
    assert_ne!(new_project, original_project);
    assert_ne!(copied, reference);
    assert_eq!(done["messages"][0]["referenceAssetIds"], json!([reference]));
    assert_eq!(
        done["messages"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|m| m["role"] == "USER")
            .unwrap()["referenceAssetIds"],
        json!([copied])
    );
    assert_eq!(done["turnJobCount"], 1);
    let job = api.wait(done["generatedJobIds"][0].as_str().unwrap());
    assert_eq!(job["status"], "SUCCEEDED", "{job}");
    assert_eq!(job["projectId"], new_project);
    assert_eq!(job["request"]["referenceAssetIds"], json!([copied]));
    assert_eq!(job["referenceAssetIds"], json!([copied]));
    assert_eq!(api.call("assistant/message", input)["result"]["id"], id);
    assert_eq!(
        api.call("projects/list", json!({}))["result"]["data"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        api.call("assets/list", json!({"projectId":original_project}))["result"]["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        api.call("jobs/list", json!({"projectId":new_project}))["result"]["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn guide_disconnect_is_unknown_and_duplicate_message_does_not_restart_it() {
    let mut api = Api::new("guide-disconnect");
    let input = json!({"requestId":"lost-1","message":"Set up a game"});
    let accepted = api.call("assistant/message", input.clone())["result"].clone();
    let id = accepted["id"].as_str().unwrap();
    let session = api.wait_guide(id);
    assert_eq!(session["status"], "UNKNOWN");
    assert_eq!(session["error"]["code"], "CODEX_DISCONNECTED");
    assert_eq!(api.call("assistant/message", input)["result"]["id"], id);
    assert_eq!(
        api.call("assistant/get", json!({"id":id}))["result"]["status"],
        "UNKNOWN"
    );
}

#[test]
fn guide_busy_and_cancellation_preserve_the_accepted_intent() {
    let mut api = Api::new("slow");
    let input = json!({"requestId":"slow-guide-1","message":"Develop a game"});
    let session = api.call("assistant/message", input.clone())["result"].clone();
    let id = session["id"].as_str().unwrap();
    assert_eq!(
        api.call(
            "assistant/message",
            json!({"requestId":"next-guide-2","sessionId":id,"message":"Another request"})
        )["error"]["code"],
        "ASSISTANT_BUSY"
    );
    std::thread::sleep(std::time::Duration::from_millis(150));
    api.call("assistant/cancel", json!({"id":id}));
    let final_session = api.wait_guide(id);
    assert_eq!(final_session["status"], "FAILED");
    assert_eq!(final_session["error"]["code"], "CANCELLED");
    assert_eq!(api.call("assistant/message", input)["result"]["id"], id);
    assert_eq!(
        api.call("projects/list", json!({}))["result"]["data"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn guided_style_customization_keeps_references_and_preset_dimensions() {
    let mut api = Api::new("guide-custom");
    let catalog = api.call("styles/presets/list", json!({}))["result"]
        .as_array()
        .unwrap()
        .clone();
    let mut style = catalog.iter().find(|p| p["id"] == "paint").unwrap()["style"].clone();
    let project = api.call(
        "projects/create",
        json!({"name":"Custom Game","style":style}),
    )["result"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/generated/mira-idle.png");
    let asset = api.call(
        "assets/import",
        json!({"projectId":project,"path":path,"name":"Pinned reference"}),
    )["result"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    style["referenceAssetIds"] = json!([asset]);
    api.call(
        "projects/style/update",
        json!({"projectId":project,"style":style}),
    );
    let session=api.call("assistant/message",json!({"requestId":"customize-1","projectId":project,"message":"Give my game a moonlit palette and create a character.","allowGeneration":true}))["result"].clone();
    let session = api.wait_guide(session["id"].as_str().unwrap());
    assert_eq!(session["status"], "READY", "{session}");
    let project_state = api.call("projects/get", json!({"id":project}))["result"].clone();
    assert_eq!(project_state["name"], "Custom Game");
    assert_eq!(project_state["style"]["name"], "Moonlit story");
    assert_eq!(
        project_state["style"]["palette"],
        json!(["#7386A4", "#C8D0E2"])
    );
    assert_eq!(project_state["style"]["presetId"], "paint");
    assert_eq!(project_state["style"]["referenceAssetIds"], json!([asset]));
    let job = api.wait(session["generatedJobIds"][0].as_str().unwrap());
    assert_eq!(job["status"], "SUCCEEDED");
    assert_eq!(job["request"]["width"], 1024);
    assert_eq!(job["request"]["height"], 1024);
    assert_eq!(job["styleSnapshot"], project_state["style"]);
    assert_eq!(job["referenceAssetIds"], json!([asset]));
}

#[test]
fn login_cancellation_and_completion_refresh_account_state() {
    let mut api = Api::new("login");
    assert_eq!(
        api.call("account/read", json!({}))["result"]["isLoggedIn"],
        false
    );
    let first = api.call("account/login/start", json!({}))["result"].clone();
    assert_eq!(
        first["authUrl"],
        "https://auth.openai.com/authorize?fixture=true"
    );
    assert_eq!(
        api.call("account/login/cancel", json!({"id":first["loginId"]}))["result"]["isCancelled"],
        true
    );
    std::thread::sleep(std::time::Duration::from_millis(180));
    assert_eq!(
        api.call("account/read", json!({}))["result"]["isLoggedIn"],
        false
    );
    api.call("account/login/start", json!({}));
    for _ in 0..40 {
        let status = api.call("account/read", json!({}))["result"].clone();
        if status["isLoggedIn"] == true {
            assert_eq!(status["canGenerateImages"], true);
            api.call("account/read", json!({}));
            assert!(
                api.notifications
                    .iter()
                    .any(|n| n["kind"] == "ACCOUNT_CONNECTED")
            );
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("Login completion did not refresh account state");
}
