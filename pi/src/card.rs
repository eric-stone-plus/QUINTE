//! A2A v1.0 agent card. Static capabilities: this seat receives one task
//! and returns one contract artifact; it accepts JSON parts only.

use serde_json::json;

pub fn agent_card(seat: &crate::prompt::Seat) -> serde_json::Value {
    json!({
        "name": format!("pi-seat-{}", seat.role.replace('-', "_")),
        "description": format!(
            "Minimal A2A v1.0 review seat: {} ({})",
            seat.school.party, seat.school.discipline
        ),
        "url": "http://127.0.0.1:8900/",
        "version": "1.0.0",
        "protocolVersion": "1.0",
        "capabilities": {
            "streaming": false,
            "pushNotifications": false,
            "stateTransitionHistory": false
        },
        "defaultInputModes": ["application/json"],
        "defaultOutputModes": ["application/json"],
        "skills": [{
            "id": "review-seat",
            "name": "contract review seat",
            "description": "Reviews the supplied packet and evidence, returns one contract artifact.",
            "inputModes": ["application/json"],
            "outputModes": ["application/json"]
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_carries_protocol_version() {
        let seat = crate::prompt::seat("a-r1").unwrap();
        let card = agent_card(&seat);
        assert_eq!(card["protocolVersion"], "1.0");
        assert!(card["name"].as_str().unwrap().contains("a_r1"));
    }
}
