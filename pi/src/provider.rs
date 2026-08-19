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
        if let Some(proxy_url) = std::env::var("HTTPS_PROXY")
            .ok()
            .or_else(|| std::env::var("https_proxy").ok())
        {
            let proxy = reqwest::Proxy::https(&proxy_url)
                .map_err(|e| anyhow::anyhow!("HTTPS_PROXY is not a usable proxy URL: {e:#}"))?;
            builder = builder.proxy(proxy);
        }
        let client = builder.build()?;
        // Transport-level retry: the request is fully idempotent (same
        // prompt, same contract), and a local egress gateway can drop one
        // of several concurrent tunneled requests. Business-level retry
        // policy stays with the orchestrator.
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0.2,
        });
        let mut attempt = 0;
        let mut rate_limited_attempts = 0u32;
        let body: Value = loop {
            let mut response = loop {
                match client
                    .post(format!("{}/chat/completions", self.base_url))
                    .header("Authorization", format!("Bearer {api_key}"))
                    .json(&body)
                    .send()
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
            // back off 5s/10s/20s so a five-seat R1 fan-out staggers
            // itself under the account's concurrency ceiling.
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
                if rate_limited_attempts >= 4 {
                    bail!(
                        "provider returned 429 after {rate_limited_attempts} attempts: {detail}"
                    );
                }
                let wait = retry_after
                    .unwrap_or_else(|| 5u64 * (1 << (rate_limited_attempts - 1)))
                    .min(60);
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
