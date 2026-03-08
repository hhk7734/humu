import pytest
from unittest.mock import AsyncMock, patch, MagicMock

from humu.providers.base import LLMProvider, Message, LLMResponse, LLMStreamChunk
from humu.providers.registry import ProviderRegistry


def test_provider_registry_default():
    registry = ProviderRegistry()
    provider = registry.get("anthropic")
    assert provider is not None


def test_provider_registry_unknown():
    registry = ProviderRegistry()
    with pytest.raises(KeyError):
        registry.get("unknown-provider")


def test_message_model():
    msg = Message(role="user", content="hello")
    assert msg.role == "user"


def test_llm_response_model():
    resp = LLMResponse(text="hello", usage={"input_tokens": 10, "output_tokens": 5})
    assert resp.text == "hello"
