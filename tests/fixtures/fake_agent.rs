use std::fs;
use std::io::{self, Write};
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
    if args.len() != 3 {
        eprintln!("expected PHASE PARTY_ID PACKET_PATH");
        std::process::exit(64);
    }

    if args[0] == "R1" && args[1] == "Party A" {
        let delay_path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("fake-agent-delay-ms");
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
