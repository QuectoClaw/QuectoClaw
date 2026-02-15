// QuectoClaw — Provider factory

use super::{http::HTTPProvider, LLMProvider};
use crate::config::Config;

/// Create an LLM provider from the loaded config.
///
/// Auto-detects the provider from the model name and resolves API credentials.
pub fn create_provider(cfg: &Config) -> anyhow::Result<Box<dyn LLMProvider>> {
    let (api_key, api_base, provider_name) = cfg
        .resolve_provider()
        .ok_or_else(|| anyhow::anyhow!(
            "No API key configured. Set a provider key in ~/.quectoclaw/config.json or via environment variables.\n\
             Example: QUECTOCLAW_PROVIDERS_OPENAI_API_KEY=sk-..."
        ))?;

    tracing::info!(
        provider = %provider_name,
        model = %cfg.agents.defaults.model,
        api_base = %if api_base.is_empty() { "(default)" } else { &api_base },
        "Creating LLM provider"
    );

    // Get proxy if available (only nvidia has one in the default config)
    let proxy = if provider_name == "nvidia" && !cfg.providers.nvidia.proxy.is_empty() {
        Some(cfg.providers.nvidia.proxy.as_str())
    } else {
        None
    };

    let provider = HTTPProvider::new(api_key, api_base, proxy, cfg.agents.defaults.model.clone())?;

    Ok(Box::new(provider))
}
