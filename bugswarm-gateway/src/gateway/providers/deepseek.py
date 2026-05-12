"""DeepSeek provider adapter — OpenAI-compatible with DeepSeek-specific optimizations.

DeepSeek V4 Flash and V4 Pro use the OpenAI API format. The OpenAI adapter handles
this natively via base_url=https://api.deepseek.com.
"""

from .openai import OpenAIAdapter


class DeepSeekAdapter(OpenAIAdapter):
    """Adapter for DeepSeek API — fully OpenAI-compatible.

    Models: deepseek-v4-flash (fast), deepseek-v4-pro (powerful)
    Pricing: $0.14/1M input, $0.28/1M output (v4-flash, cache miss)
    Context: 1M tokens, max output 384K tokens
    Features: JSON output, tool calls, FIM completion, chat prefix completion
    """
    pass
