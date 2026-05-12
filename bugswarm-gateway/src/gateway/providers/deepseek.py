"""DeepSeek provider adapter — OpenAI-compatible API.

DeepSeek models (deepseek-chat=V3, deepseek-reasoner=R1) use the OpenAI API format
with base_url=https://api.deepseek.com. The OpenAI adapter handles this natively
by setting base_url in ProviderConfig. This module exists for DeepSeek-specific
optimizations: prompt formatting, token counting, and model-specific behavior.
"""

from .openai import OpenAIAdapter


class DeepSeekAdapter(OpenAIAdapter):
    """Adapter for DeepSeek API — identical to OpenAI adapter with DeepSeek defaults.

    DeepSeek's API is fully OpenAI-compatible. The only difference is:
    - base_url = https://api.deepseek.com
    - Models: deepseek-chat (V3), deepseek-reasoner (R1)
    - Tokenizer: uses DeepSeek's tokenizer (approximated with cl100k_base)
    - Pricing: $0.27/1M input, $1.10/1M output (deepseek-chat)
    """

    pass
