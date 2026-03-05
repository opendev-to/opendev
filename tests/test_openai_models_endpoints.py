"""Empirical test: probe every OpenAI model against both API endpoints.

Usage:
    OPENAI_API_KEY=sk-... python -m pytest tests/test_openai_models_endpoints.py -s

The test loads all OpenAI model IDs from the model registry
(~/.opendev/cache/providers/openai.json), skips models with specialized
capabilities (audio, realtime, image-generation, search, computer-use,
research), and tries a minimal request against both:
  - POST https://api.openai.com/v1/chat/completions
  - POST https://api.openai.com/v1/responses

It prints a summary table showing which endpoint(s) each model supports.
"""

import os

import pytest
import requests

from swecli.config.models import get_model_registry

# Capabilities that indicate the model needs a specialized API
SKIP_CAPABILITIES = {"audio", "realtime", "image-generation", "search", "computer-use", "research"}

CHAT_URL = "https://api.openai.com/v1/chat/completions"
RESPONSES_URL = "https://api.openai.com/v1/responses"


def _load_openai_models() -> list[dict]:
    """Load model entries from the OpenAI provider in the model registry."""
    registry = get_model_registry()
    provider = registry.get_provider("openai")
    if provider is None:
        pytest.skip("OpenAI provider not found in model registry")

    models = []
    for model in provider.models.values():
        caps = set(model.capabilities)
        if caps & SKIP_CAPABILITIES:
            continue
        models.append({"id": model.id, "name": model.name, "capabilities": model.capabilities})
    return models


def _try_chat_completions(api_key: str, model_id: str) -> tuple[int, str]:
    """Try a minimal chat completions request. Returns (status_code, snippet)."""
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}",
    }
    payload = {
        "model": model_id,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 5,
    }
    try:
        resp = requests.post(CHAT_URL, headers=headers, json=payload, timeout=(10, 30))
        return resp.status_code, resp.text[:200]
    except Exception as exc:
        return -1, str(exc)[:200]


def _try_responses(api_key: str, model_id: str) -> tuple[int, str]:
    """Try a minimal responses API request. Returns (status_code, snippet)."""
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}",
    }
    payload = {
        "model": model_id,
        "input": "hi",
        "max_output_tokens": 5,
    }
    try:
        resp = requests.post(RESPONSES_URL, headers=headers, json=payload, timeout=(10, 30))
        return resp.status_code, resp.text[:200]
    except Exception as exc:
        return -1, str(exc)[:200]


@pytest.mark.skipif(
    not os.getenv("OPENAI_API_KEY"),
    reason="OPENAI_API_KEY not set — skipping live endpoint probe",
)
def test_probe_openai_models_endpoints():
    """Probe every OpenAI model against both endpoints and print summary."""
    api_key = os.environ["OPENAI_API_KEY"]
    models = _load_openai_models()

    results = []
    for model_data in models:
        model_id = model_data["id"]
        chat_status, chat_snippet = _try_chat_completions(api_key, model_id)
        resp_status, resp_snippet = _try_responses(api_key, model_id)
        results.append({
            "model": model_id,
            "chat_status": chat_status,
            "chat_snippet": chat_snippet,
            "responses_status": resp_status,
            "responses_snippet": resp_snippet,
        })

    # Print summary table
    print("\n" + "=" * 100)
    print(f"{'Model':<30} {'Chat Completions':>18} {'Responses API':>18}  Recommendation")
    print("-" * 100)
    for r in results:
        chat_ok = "OK" if r["chat_status"] == 200 else str(r["chat_status"])
        resp_ok = "OK" if r["responses_status"] == 200 else str(r["responses_status"])

        if r["chat_status"] == 200 and r["responses_status"] != 200:
            rec = "chat"
        elif r["responses_status"] == 200 and r["chat_status"] != 200:
            rec = "responses"
        elif r["chat_status"] == 200 and r["responses_status"] == 200:
            rec = "both (prefer chat)"
        else:
            rec = "NEITHER"

        print(f"  {r['model']:<28} {chat_ok:>18} {resp_ok:>18}  {rec}")

    print("=" * 100)

    # Print detailed errors for failing models
    print("\nDetailed errors:")
    for r in results:
        if r["chat_status"] != 200:
            print(f"\n  {r['model']} chat/{r['chat_status']}: {r['chat_snippet'][:120]}")
        if r["responses_status"] != 200:
            print(f"\n  {r['model']} responses/{r['responses_status']}: {r['responses_snippet'][:120]}")
