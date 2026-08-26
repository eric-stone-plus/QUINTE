//! One OpenAI-compatible completion call, blocking. Only two failure
//! classes retry in-seat, both provably work-free: transport errors
//! (the request may never have left the machine) and HTTP 429 (the
//! provider refused the request, so it never landed and retrying the
//! idempotent call duplicates no billed work). Everything else fails
//! closed — business-level retry policy belongs to the orchestrator;
//! a seat that retried real work would duplicate it and inflate cost
//! without any epistemic gain.

use anyhow::{Context, Result, bail};
use std::io::Read;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Provider {
    key_env: String,
    base_url: String,
    model: String,
    anthropic: bool,
}

/// Anthropic Messages gateways expose themselves by path convention
/// (``.../anthropic``), same rule as the STAMMTISCH AI driver.
fn is_anthropic_endpoint(base_url: &str) -> bool {
    let path = base_url.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    path.eq_ignore_ascii_case("anthropic")
}

/// One provider request, transport-shaped. The Anthropic branch maps the
/// (system, user) pair onto Messages format and unwraps the content
/// blocks (thinking blocks are skipped — reasoning is not the product).
fn build_request(
    provider: &Provider,
    api_key: &str,
    system: &str,
    user: &str,
) -> (String, Vec<(&'static str, String)>, Value) {
    if provider.anthropic {
        let url = format!("{}/v1/messages", provider.base_url);
        let headers = vec![
            ("x-api-key", api_key.to_string()),
            ("anthropic-version", "2023-06-01".to_string()),
        ];
        let body = json!({
            "model": provider.model,
            "max_tokens": 8192,
            "temperature": 0.2,
            "system": system,
            "messages": [{"role": "user", "content": user}],
        });
        (url, headers, body)
    } else {
        let url = format!("{}/chat/completions", provider.base_url);
        let headers = vec![("Authorization", format!("Bearer {api_key}"))];
        let body = json!({
            "model": provider.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0.2,
        });
        (url, headers, body)
    }
}

fn extract_text(provider: &Provider, body: &Value) -> Result<String> {
    if provider.anthropic {
        let mut out = String::new();
        let blocks = body
            .get("content")
            .and_then(Value::as_array)
            .context("anthropic response has no content blocks")?;
        for block in blocks {
            if block.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    out.push_str(text);
                }
            }
        }
        if out.trim().is_empty() {
            bail!("anthropic response carried no text block");
        }
        Ok(out)
    } else {
        let content = body
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .context("provider response has no choices[0].message.content")?;
        Ok(content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_endpoints_are_detected_by_path() {
        assert!(is_anthropic_endpoint(
            "https://token-plan.cn-beijing.maas.aliyuncs.com/apps/anthropic"));
        assert!(is_anthropic_endpoint("http://127.0.0.1:8082/anthropic/"));
        assert!(!is_anthropic_endpoint("https://api.deepseek.com/v1"));
    }

    #[test]
    fn anthropic_request_shape_and_text_extraction() {
        let provider = Provider {
            key_env: "K".into(),
            base_url: "https://gw.example/apps/anthropic".into(),
            model: "qwen3.8-max".into(),
            anthropic: true,
        };
        let (url, headers, body) =
            build_request(&provider, "sk-test", "SYS", "USER");
        assert_eq!(url, "https://gw.example/apps/anthropic/v1/messages");
        assert!(headers.iter().any(|(k, v)| *k == "x-api-key" && v == "sk-test"));
        assert_eq!(body["system"], "SYS");
        assert_eq!(body["messages"][0]["role"], "user");
        let parsed = json!({"content": [
            {"type": "thinking", "thinking": "..."},
            {"type": "text", "text": "answer"},
        ]});
        assert_eq!(extract_text(&provider, &parsed).unwrap(), "answer");
    }

    #[test]
    fn openai_shape_unchanged() {
        let provider = Provider {
            key_env: "K".into(),
            base_url: "https://api.example/v1".into(),
            model: "m".into(),
            anthropic: false,
        };
        let (url, headers, _body) = build_request(&provider, "sk", "S", "U");
        assert_eq!(url, "https://api.example/v1/chat/completions");
        assert!(headers.iter().any(|(k, _)| *k == "Authorization"));
    }
}

impl Provider {
    pub fn new(key_env: &str, base_url: &str, model: &str) -> Result<Self> {
        if base_url.trim().is_empty() || model.trim().is_empty() {
            bail!("provider base_url and model must be non-empty");
        }
        Ok(Self {
            key_env: key_env.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            anthropic: is_anthropic_endpoint(base_url),
        })
    }

    /// One chat completion. Fails closed: transport errors, HTTP errors,
    /// and provider error envelopes all become `Err`.
    pub fn complete(&self, system: &str, user: &str) -> Result<String> {
        // reqwest's no-provider rustls build requires a process crypto
        // provider; installing it at call time (idempotent) keeps the
        // invariant local to the code that needs it.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let api_key = std::env::var(&self.key_env).with_context(|| {
            format!(
                "environment variable {} is not set",
                self.key_env
            )
        })?;
        // Follow the ambient proxy (e.g. a local egress gateway): reqwest
        // does not read HTTP(S)_PROXY by default. PI has exactly one
        // provider host, so NO_PROXY is deliberately NOT honored — a
        // machine-level NO_PROXY entry for the provider host would
        // silently move traffic to a flaky direct path.
        let mut builder = reqwest::blocking::Client::builder()
            .user_agent("pi")
            // reqwest's 30s default total timeout is far too small for a
            // thinking model's first-token latency on a full review
            // packet; 600s matches QUINTE's lane budget order.
            .timeout(std::time::Duration::from_secs(600));
        let direct = std::env::var("PI_DIRECT").ok().as_deref() == Some("1");
        if let Some(proxy_url) = if direct {
            None
        } else {
            std::env::var("HTTPS_PROXY")
                .ok()
                .or_else(|| std::env::var("https_proxy").ok())
        } {
            let proxy = reqwest::Proxy::https(&proxy_url)
                .map_err(|e| anyhow::anyhow!("HTTPS_PROXY is not a usable proxy URL: {e:#}"))?;
            builder = builder.proxy(proxy);
        }
        let client = builder.build()?;
        let (url, headers, body) = build_request(self, &api_key, system, user);
        let mut attempt = 0;
        let mut rate_limited_attempts = 0u32;
        let body: Value = loop {
            let mut response = loop {
                let mut request = client.post(&url);
                for (name, value) in &headers {
                    request = request.header(*name, value);
                }
                match request.json(&body).send()
                {
                    Ok(response) => break response,
                    Err(e) => {
                        attempt += 1;
                        if attempt >= 3 {
                            return Err(anyhow::anyhow!(
                                "provider request failed after {attempt} attempts: {e:?}"
                            ));
                        }
                        std::thread::sleep(std::time::Duration::from_secs(attempt));
                    }
                }
            };
            let status = response.status();
            // Honor Retry-After when the provider sends one; otherwise
            // back off 5s/15s/45s/90s. The schedule must outlive a
            // per-minute rate window, not just a concurrency blip: a
            // four-attempt/35s ceiling was observed exhausting three of
            // five R1 lanes under a rate-limited shared key, while a
            // ~155s tolerance converts the same collisions into delays.
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let mut bytes = Vec::new();
            response
                .read_to_end(&mut bytes)
                .map_err(|e| anyhow::anyhow!("provider body read failed: {e:?}"))?;
            let parsed: Value = serde_json::from_slice(&bytes)
                .map_err(|e| anyhow::anyhow!("provider response is not JSON: {e:#}"))?;
            if status.as_u16() == 429 {
                rate_limited_attempts += 1;
                let detail = parsed
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("rate limited");
                if rate_limited_attempts >= 5 {
                    bail!(
                        "provider returned 429 after {rate_limited_attempts} attempts: {detail}"
                    );
                }
                let wait = retry_after
                    .unwrap_or_else(|| match rate_limited_attempts {
                        1 => 5,
                        2 => 15,
                        3 => 45,
                        _ => 90,
                    })
                    .min(120);
                std::thread::sleep(std::time::Duration::from_secs(wait));
                continue;
            }
            if !status.is_success() {
                let detail = parsed
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("unspecified provider error");
                bail!("provider returned {}: {detail}", status.as_u16());
            }
            break parsed;
        };
        extract_text(self, &body)
    }
}
