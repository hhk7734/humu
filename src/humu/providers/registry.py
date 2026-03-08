from __future__ import annotations

from humu.providers.base import LLMProvider


class ProviderRegistry:
    def __init__(self) -> None:
        self._providers: dict[str, LLMProvider] = {}
        self._register_defaults()

    def _register_defaults(self) -> None:
        from humu.providers.anthropic import AnthropicProvider
        from humu.providers.openai import OpenAIProvider

        self._providers["anthropic"] = AnthropicProvider()
        self._providers["openai"] = OpenAIProvider()

    def get(self, name: str) -> LLMProvider:
        if name not in self._providers:
            raise KeyError(f"Unknown provider: {name}")
        return self._providers[name]

    def register(self, name: str, provider: LLMProvider) -> None:
        self._providers[name] = provider
