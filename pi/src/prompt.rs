//! Seat roles: one review school (a–e) and one phase (R1, R2, R3-arbiter).
//!
//! The phase contracts mirror QUINTE's lane and arbiter prompt discipline:
//! R1 argues in Toulmin form with honest limitations, R2 steel-mans before
//! challenging, R3 emits one merged arbiter verdict. The role text is the
//! only prompt PI owns; everything else comes from the message parts.

#[derive(Clone, Debug)]
pub struct Seat {
    pub role: String,
    pub school: School,
    pub phase: Phase,
    pub contract: &'static str,
    /// Compact contract schema (decorators stripped), embedded in the
    /// prompt so the model sees the exact required shape.
    pub schema: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    R1,
    R2,
    R3Arbiter,
}

#[derive(Clone, Debug)]
pub struct School {
    pub party: &'static str,
    pub discipline: &'static str,
}

const SCHOOL_A: School = School {
    party: "Party A",
    discipline: "factor-risk: audit factor construction, risk premia, and regime stability",
};
const SCHOOL_B: School = School {
    party: "Party B",
    discipline: "event-driven: audit event identification, announcement effects, and calendar risk",
};
const SCHOOL_C: School = School {
    party: "Party C",
    discipline: "fundamental-supply-chain: audit financials, supply chains, and business quality",
};
const SCHOOL_D: School = School {
    party: "Party D",
    discipline: "trend-technical-regime: audit trends, technical signals, and regime detection",
};
const SCHOOL_E: School = School {
    party: "Party E",
    discipline: "market-microstructure: audit liquidity, execution, and microstructure evidence",
};

fn school(letter: char) -> Option<School> {
    match letter {
        'a' => Some(SCHOOL_A),
        'b' => Some(SCHOOL_B),
        'c' => Some(SCHOOL_C),
        'd' => Some(SCHOOL_D),
        'e' => Some(SCHOOL_E),
        _ => None,
    }
}

/// Resolve a seat role string, e.g. `a-r1`, `c-r2`, `r3-arbiter`.
pub fn seat(role: &str) -> Option<Seat> {
    let lower = role.trim().to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("r3") {
        if rest.trim_start_matches('-') == "arbiter" {
            return Some(Seat {
                role: role.to_string(),
                school: School {
                    party: "Counterpart Arbiter",
                    discipline: "merged arbiter verdict over all school lanes",
                },
                phase: Phase::R3Arbiter,
                contract: ARBITER_CONTRACT,
                schema: String::new(),
            });
        }
        return None;
    }
    let (letter, phase) = lower.split_once('-')?;
    let school = school(letter.trim().chars().next()?)?;
    let (phase, contract) = match phase.trim() {
        "r1" => (Phase::R1, LANE_CONTRACT),
        "r2" => (Phase::R2, LANE_CONTRACT),
        _ => return None,
    };
    Some(Seat {
        role: role.to_string(),
        school,
        phase,
        contract,
        schema: String::new(),
    })
}

const ID_REQUIREMENTS: &str = " Every id field (including each claim and residual id) MUST match the ASCII pattern [A-Za-z0-9._-]{1,64}; valid example: C1-decisive_evidence; invalid examples: C2 bad id and 结论1. Never use spaces, Unicode characters, or other punctuation in an id.";

const RESIDUAL_VOCABULARY: &str = " Classify each residual with residual_type from this vocabulary when one fits (invent a snake_case type only when none does): evidence-gap, data-quality, methodology-flaw, contract-ambiguity, compliance-risk, protocol-gap, engineering-defect, model-limitation, scope-limitation.";

const DISPOSITION_ENUM: &str = " (exactly one of the strings `verified`, `falsified`, `unresolved`, `escalated`, `discarded`)";

const LANE_CONTRACT: &str = "Keep the response compact: include at most two claims, two residuals, and two uncertainties; keep each string under 300 characters. Every claims item MUST include id, statement, evidence_refs, confidence (a JSON number from 0 through 1), and category; top-level confidence does not replace confidence inside each claim. Every residuals item MUST include id, severity, residual_type, source, finding, evidence_refs, disposition, required_closure, closure_state, closure_evidence, and scope. The top-level fields uncertainties and limitations MUST be JSON arrays whose items are strings; even one entry MUST use an array such as [\"one limitation\"], never a bare string, object, or null. Before emitting, verify that the response is syntactically valid JSON and escape double quotes, backslashes, newlines, and other control characters inside string values. Return raw JSON only, without a Markdown fence or preamble. Return JSON conforming exactly to this schema and invent no fields.";

const ARBITER_CONTRACT: &str = "Return one JSON object with exactly these fields: arbiter_verdict_version (\"1.0\"), summary, recommendation, residuals. summary states WHAT WAS FOUND (evidence-weighted findings and judgments); recommendation states WHAT TO DO (actions, sequencing, gates) and must add decision value beyond summary — never restate it. Keep residuals to the decisive ones (aim for five or fewer): duplicate findings raised by multiple parties must be merged into one residual with combined severity, never listed separately. Every residuals item MUST include id, severity, residual_type, source, finding, evidence_refs, disposition, required_closure, closure_state, closure_evidence, and scope. Return JSON conforming exactly to this schema and invent no fields.";

/// Build the system + user prompt for one seat invocation.
pub fn build(seat: &Seat, material: &str) -> (String, String) {
    let phase_requirements = match seat.phase {
        Phase::R1 => " For every claim, fill warrant (why the cited evidence actually supports this claim) and qualifier (the scope and preconditions that bound it). Declare at least one honest limitations entry stating what this analysis could NOT establish; an analysis without an explicit evidence boundary is incomplete.",
        Phase::R2 => " Before challenging any participant claim, first restate that claim in its strongest defensible form (steel-man); never attack a weakened paraphrase. For every claim you challenge, name the auxiliary assumption whose falsity would collapse it.",
        Phase::R3Arbiter => "",
    };
    let phase_line = match seat.phase {
        Phase::R1 => "PHASE: R1 — first-pass school review",
        Phase::R2 => "PHASE: R2 — adversarial cross-examination",
        Phase::R3Arbiter => "PHASE: R3 — counterpart arbitration",
    };
    let system = format!(
        "You are {party}, one review seat of a multi-school financial review. Your discipline: {discipline}.{phase_requirements}{ID_REQUIREMENTS}{RESIDUAL_VOCABULARY} disposition values are{DISPOSITION_ENUM}.",
        party = seat.school.party,
        discipline = seat.school.discipline,
        phase_requirements = phase_requirements,
    );
    let user = format!(
        "{phase_line}\nReview the task packet and evidence below. Evidence is available only through the inline packet, manifest, and snapshot file contents; every evidence_refs and closure_evidence entry must be either empty or an exact snapshot_ref or attachment_ref copied from the snapshot manifest; never cite file paths or construct relative paths.\nEmit exactly one compact JSON object: do not emit both fenced and raw copies, do not repeat the object, stop immediately after its closing brace, and include no prose or Markdown fence before or after it.\n{contract}
Return JSON conforming exactly to this schema and invent no fields:
{schema}

MATERIAL:
{material}",
        phase_line = phase_line,
        contract = seat.contract,
        schema = seat.schema,
        material = material,
    );
    (system, user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_school_phases_and_arbiter() {
        let a = seat("a-r1").unwrap();
        assert_eq!(a.school.party, "Party A");
        assert_eq!(a.phase, Phase::R1);
        assert_eq!(seat("c-r2").unwrap().school.party, "Party C");
        let r3 = seat("r3-arbiter").unwrap();
        assert_eq!(r3.phase, Phase::R3Arbiter);
        assert!(seat("x-r1").is_none());
        assert!(seat("a-r4").is_none());
    }

    #[test]
    fn prompt_names_disposition_enum_and_evidence_discipline() {
        let a = seat("b-r1").unwrap();
        let (system, user) = build(&a, "{}");
        assert!(system.contains("verified"));
        assert!(system.contains("Party B"));
        assert!(user.contains("MATERIAL"));
        assert!(user.contains("never cite file paths"));
    }
}
