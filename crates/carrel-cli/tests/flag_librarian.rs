//! Flags as requests to the librarian (#454) at the terminal, with a scripted text model
//! (`carrel-provider-echo-script`): `carrel flag --ask` leads to a question, answering it in
//! `carrel questions answer` leads to a fix, and `carrel flagged --accept` makes it and clears
//! the flag. The core's own tests are in `crates/carrel-core/tests/flag_requests.rs`.

use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::{Value, json};

const ORIGIN: &str = "01a18701-0500-776a-9867-08063e3d5d47";
const ORIGIN_CARD: &str = "Reading/On the Origin of Species.md";

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The scripted model, built beside this test in the target folder it runs from.
fn echo_provider() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let exe = std::env::current_exe().unwrap();
    let profile = exe.parent().unwrap().parent().unwrap();
    let status = std::process::Command::new(env!("CARGO"))
        .args([
            "build",
            "--quiet",
            "-p",
            "carrel-core",
            "--example",
            "echo_provider",
        ])
        .arg("--target-dir")
        .arg(profile.parent().unwrap())
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    profile
        .join("examples")
        .join(format!("echo_provider{}", std::env::consts::EXE_SUFFIX))
}

/// A copy of the sample archive with history, its own configuration folder, and the scripted
/// model on its `PATH`, so tests never touch the real configuration.
struct Sandbox {
    dir: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let sandbox = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-archive"),
            &sandbox.root(),
        );
        let bin = sandbox.dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::copy(
            echo_provider(),
            bin.join(format!(
                "carrel-provider-echo-script{}",
                std::env::consts::EXE_SUFFIX
            )),
        )
        .unwrap();
        let out = sandbox.run(&[], &["init", "."]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        sandbox
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("archive")
    }

    /// Runs `carrel args` with the model answering with `script`, one line per request.
    fn run(&self, script: &[Value], args: &[&str]) -> Output {
        let lines: Vec<String> = script.iter().map(Value::to_string).collect();
        let file = self.dir.path().join("script.jsonl");
        std::fs::write(&file, lines.join("\n")).unwrap();
        std::process::Command::new(env!("CARGO_BIN_EXE_carrel"))
            .args(args)
            .current_dir(self.root())
            .env("PATH", self.dir.path().join("bin"))
            .env("ECHO_SCRIPT", &file)
            .env("CARREL_CONFIG_DIR", self.dir.path().join("config"))
            .env("CARREL_TRASH_DIR", self.dir.path().join("Trash"))
            .env("CARREL_WORKER", "off")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }

    fn ok(&self, script: &[Value], args: &[&str]) -> String {
        let out = self.run(script, args);
        assert!(
            out.status.success(),
            "carrel {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn json(&self, script: &[Value], args: &[&str]) -> Value {
        let mut args = args.to_vec();
        args.push("--json");
        serde_json::from_str(&self.ok(script, &args)).unwrap()
    }
}

#[allow(clippy::needless_pass_by_value)] // written in place, as `json!` gives it
fn answer(text: &str, suggestions: Value) -> Value {
    json!({"action": "answer", "input": "", "answer": text, "items": [], "passages": [], "suggestions": suggestions})
}

#[test]
fn a_flag_handed_to_the_librarian_asks_then_fixes_and_accepting_clears_it() {
    let sandbox = Sandbox::new();

    // Flagged and handed over at once: the librarian asks.
    let asks = [answer(
        "Question: is it the year, or the kind of copy?",
        json!([]),
    )];
    let text = sandbox.ok(
        &asks,
        &["flag", ORIGIN_CARD, "--note", "summary is off", "--ask"],
    );
    assert!(text.contains("Waiting for your answer."), "{text}");
    assert!(
        text.contains("The librarian asks: is it the year, or the kind of copy?"),
        "{text}"
    );

    // Status and `carrel flagged` list it by state.
    let status = sandbox.ok(&[], &["status"]);
    assert!(status.contains("Waiting for your answer (1):"), "{status}");
    let listed = sandbox.json(&[], &["flagged"]);
    assert_eq!(listed["flagged"][0]["state"], "asked", "{listed}");

    // The question is in the queue; answering it there has the librarian try again.
    let questions = sandbox.json(&[], &["questions"]);
    let question = questions["questions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["kind"] == "flag")
        .unwrap_or_else(|| panic!("a flag question: {questions}"));
    let id = question["id"].as_str().unwrap();
    let fixes = [answer(
        "It's a reprint; the summary should say so.",
        json!([{"change": "correct", "item": ORIGIN, "field": "summary",
                "value": "Charles Darwin's 1859 book, a reprint of the first edition.",
                "why": "The owner says it's a reprint.", "confidence": 0.9}]),
    )];
    let text = sandbox.ok(
        &fixes,
        &[
            "questions",
            "answer",
            id,
            "The kind of copy: it's a reprint",
        ],
    );
    assert!(text.starts_with("Answered. "), "{text}");
    assert!(text.contains("A fix waiting for you."), "{text}");
    assert!(text.contains("1.  Set the summary of"), "{text}");

    // Accepting it makes the change and clears the flag.
    let text = sandbox.ok(&[], &["flagged", ORIGIN_CARD, "--accept", "1"]);
    assert!(text.contains("The flag is cleared."), "{text}");
    let card = std::fs::read_to_string(sandbox.root().join(ORIGIN_CARD)).unwrap();
    assert!(card.contains("a reprint of the first edition"), "{card}");
    assert!(!card.contains("needs_work"), "{card}");
    let listed = sandbox.json(&[], &["flagged"]);
    assert_eq!(listed["flagged"], json!([]));
}

#[test]
fn when_it_cant_help_flagged_says_why_and_how_to_report_it() {
    let sandbox = Sandbox::new();
    sandbox.ok(&[], &["flag", "Projects", "--note", "needs a 3D preview"]);
    let cant = [answer(
        "Can't: see — I can't see what the models look like.",
        json!([]),
    )];
    let text = sandbox.ok(&cant, &["flagged", "Projects", "--ask"]);
    assert!(text.contains("The librarian can't help."), "{text}");
    assert!(
        text.contains("It can't help: I can't see what the models look like."),
        "{text}"
    );
    assert!(
        text.contains("Carrel would need: seeing what's in pictures."),
        "{text}"
    );
    assert!(
        text.contains("Report it to development with `carrel report gaps --report"),
        "{text}"
    );
    let gaps = sandbox.json(&[], &["report", "gaps"]);
    assert!(
        gaps.to_string()
            .contains("librarian: seeing what's in pictures"),
        "{gaps}"
    );
}
