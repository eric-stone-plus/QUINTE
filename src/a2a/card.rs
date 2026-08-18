use serde_json::{Value, json};

pub fn agent_card(interface_url: &str, token_configured: bool) -> Value {
    let mut card = json!({
        "name": "quinte",
        "description": "Five-school multi-path review runtime: five first-pass lanes, anonymized recheck, two-arbiter verdicts, deterministic merge.",
        "version": env!("CARGO_PKG_VERSION"),
        "supportedInterfaces": [{
            "url": interface_url,
            "protocolBinding": "JSONRPC",
            "protocolVersion": "1.0"
        }],
        "capabilities": {
            "streaming": true,
            "pushNotifications": false,
            "extendedAgentCard": false
        },
        "defaultInputModes": ["application/json"],
        "defaultOutputModes": ["application/json"],
        "skills": [{
            "id": "five-school-review",
            "name": "five-school-review",
            "description": "One A2A task equals one QUINTE review: Brief in, one review.result artifact out.",
            "tags": ["review", "evidence", "verdict"]
        }]
    });
    if token_configured {
        card["securitySchemes"] = json!({
            "bearer": { "type": "http", "scheme": "bearer" }
        });
        card["security"] = json!([{ "bearer": [] }]);
    }
    card
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_identity_matches_host_md() {
        let card = agent_card("http://127.0.0.1:8801/", false);
        assert_eq!(card["name"], "quinte");
        assert_eq!(card["supportedInterfaces"][0]["protocolBinding"], "JSONRPC");
        assert_eq!(card["supportedInterfaces"][0]["protocolVersion"], "1.0");
        assert_eq!(card["skills"][0]["id"], "five-school-review");
        assert_eq!(card["version"], env!("CARGO_PKG_VERSION"));
        assert!(card.get("securitySchemes").is_none());
    }
}
