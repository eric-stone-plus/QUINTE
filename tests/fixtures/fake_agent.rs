use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const VALID_OUTPUT: &str = r#"{
  "lane_output_version": "1.0",
  "task_restatement": "Review the supplied evidence packet.",
  "verdict": "The bounded review completed.",
  "confidence": 0.75,
  "claims": [],
  "residuals": [],
  "uncertainties": []
}"#;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(child) = std::env::var_os("FAKE_AGENT_RUNTIME_CHILD") {
        let status = Command::new(child)
            .args(args.iter().skip(1))
            .env_remove("FAKE_AGENT_RUNTIME_CHILD")
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .unwrap();
        std::process::exit(status.code().unwrap_or(1));
    }

    let args_probe = std::env::var_os("FAKE_AGENT_ARGS_PROBE");
    if args.len() != 3 && args_probe.is_none() {
        eprintln!("expected PHASE PARTY_ID PACKET_PATH");
        std::process::exit(64);
    }

    if let Some(path) = args_probe {
        fs::write(path, args.join("\0")).unwrap();
    }

    #[cfg(windows)]
    if let Ok(path) = std::env::var("FAKE_AGENT_CONSOLE_PROBE") {
        use std::ptr::null_mut;

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetConsoleWindow() -> *mut core::ffi::c_void;
        }

        let has_console_window = unsafe { GetConsoleWindow() } != null_mut();
        fs::write(path, if has_console_window { "window" } else { "hidden" }).unwrap();
    }

    if let Ok(sentinel) = std::env::var("FAKE_AGENT_STDERR_SENTINEL") {
        eprintln!("{sentinel}");
    }

    let fixture_dir = std::env::current_exe().unwrap().parent().unwrap().to_owned();
    if args[0] == "R1" && args[1] == "Party A" {
        if fixture_dir.join("fake-agent-controlled").exists() {
            fs::write(fixture_dir.join("fake-agent-started"), b"started\n").unwrap();
            while !fixture_dir.join("fake-agent-release").exists() {
                thread::sleep(Duration::from_millis(10));
            }
        }

        let delay_path = fixture_dir.join("fake-agent-delay-ms");
        if let Ok(delay) = fs::read_to_string(delay_path)
            && let Ok(delay) = delay.trim().parse::<u64>()
        {
            thread::sleep(Duration::from_millis(delay));
        }
    }

    if args[0] == "R1" && args[1] != "Party A" {
        let delay_path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("fake-agent-delay-other-ms");
        if let Ok(delay) = fs::read_to_string(delay_path)
            && let Ok(delay) = delay.trim().parse::<u64>()
        {
            thread::sleep(Duration::from_millis(delay));
        }
    }

    let repetition_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-repetition-party");
    if fs::read_to_string(repetition_party).is_ok_and(|party| party.trim() == args[1]) {
        let counter = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("fake-agent-repetition-count");
        let attempts = fs::read_to_string(&counter)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        fs::write(counter, (attempts + 1).to_string()).unwrap();
        let always = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("fake-agent-repetition-always")
            .exists();
        if attempts == 0 || always {
            print!(
                r#"{{"type":"error","error":{{"name":"UnknownError","data":{{"message":"Text repetition detected: repeated n-grams after 2 recovery attempts. Session terminated."}}}}}}"#
            );
            return;
        }
        print!("{}", serde_json_string(VALID_OUTPUT));
        return;
    }

    let timeout_output_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-timeout-output-party");
    if args[0] == "R1"
        && fs::read_to_string(timeout_output_party).is_ok_and(|party| party.trim() == args[1])
    {
        print!("{VALID_OUTPUT}");
        io::stdout().flush().unwrap();
        loop {
            thread::sleep(Duration::from_secs(60));
        }
    }

    let timeout_once_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-timeout-once-party");
    if fs::read_to_string(timeout_once_party).is_ok_and(|party| party.trim() == args[1]) {
        let counter = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("fake-agent-timeout-once-count");
        let attempts = fs::read_to_string(&counter)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        fs::write(counter, (attempts + 1).to_string()).unwrap();
        if attempts == 0 {
            loop {
                thread::sleep(Duration::from_secs(60));
            }
        }
    }

    let codewhale_invalid_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-codewhale-invalid-party");
    if args[0] == "R1"
        && fs::read_to_string(codewhale_invalid_party)
            .is_ok_and(|party| party.trim() == args[1])
    {
        let counter = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("fake-agent-codewhale-invalid-count");
        let attempts = fs::read_to_string(&counter)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        fs::write(counter, (attempts + 1).to_string()).unwrap();
        let content = if attempts == 0 {
            "analysis completed before a truncated final candidate\n```json\n{\"task_restatement\":\"cut off\"".to_string()
        } else {
            VALID_OUTPUT.to_string()
        };
        println!(r#"{{"type":"content","content":{}}}"#, json_string(&content));
        println!(r#"{{"type":"metadata","meta":{{"status":"completed"}}}}"#);
        println!(r#"{{"type":"done"}}"#);
        return;
    }

    let codewhale_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-codewhale-party");
    if fs::read_to_string(codewhale_party).is_ok_and(|party| party.trim() == args[1]) {
        println!(
            r#"{{"type":"content","content":{}}}"#,
            json_string(VALID_OUTPUT)
        );
        println!(r#"{{"type":"metadata","meta":{{"status":"completed"}}}}"#);
        println!(r#"{{"type":"done"}}"#);
        return;
    }

    let invalid_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-invalid-party");
    if fs::read_to_string(invalid_party)
        .is_ok_and(|party| party.trim() == args[1])
    {
        let output = VALID_OUTPUT.replace("\n}", ",\n  \"next_phase\": \"R3\"\n}");
        print!("{output}");
        return;
    }

    let flood_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-flood-party");
    if fs::read_to_string(flood_party).is_ok_and(|party| party.trim() == args[1]) {
        let block = vec![b'x'; 64 * 1024];
        for _ in 0..256 {
            if io::stdout().write_all(&block).is_err() {
                break;
            }
        }
        return;
    }

    let invalid_evidence_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-invalid-evidence-party");
    if fs::read_to_string(invalid_evidence_party).is_ok_and(|party| party.trim() == args[1]) {
        print!(
            "{}",
            VALID_OUTPUT.replace(
                "\"claims\": []",
                r#""claims": [{"id":"claim-1","statement":"invalid evidence","evidence_refs":["snapshot://missing.txt"],"confidence":0.5,"category":"test"}]"#,
            )
        );
        return;
    }

    let rate_limit_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-rate-limit-party");
    if args[0] == "R2"
        && fs::read_to_string(rate_limit_party).is_ok_and(|party| party.trim() == args[1])
    {
        let counter = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("fake-agent-rate-limit-count");
        let attempts = fs::read_to_string(&counter)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        fs::write(counter, (attempts + 1).to_string()).unwrap();
        if attempts == 0 {
            print!(r#"{{"error":{{"type":"rate_limit_error","retry_after":0}}}}"#);
            eprintln!("HTTP 429 Too Many Requests");
            std::process::exit(75);
        }
    }

    let prose_429_party = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("fake-agent-prose-429-party");
    if fs::read_to_string(prose_429_party).is_ok_and(|party| party.trim() == args[1]) {
        print!("{}", VALID_OUTPUT.replace("The bounded review completed.", "A cited claim contains 429 as ordinary prose."));
        return;
    }

    match std::env::var("FAKE_AGENT_MODE").as_deref() {
        Ok("invalid_utf8") => io::stdout().write_all(&[0xff, 0xfe]).unwrap(),
        Ok("unknown_field") => {
            let output = VALID_OUTPUT.replace("\n}", ",\n  \"next_phase\": \"R3\"\n}");
            print!("{output}");
        }
        _ => print!("{VALID_OUTPUT}"),
    }
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

fn serde_json_string(value: &str) -> String {
    format!(
        r#"{{"type":"content","part":{{"text":{}}}}}"#,
        json_string(value)
    )
}
