"""CLI for the LLM Gateway."""

from __future__ import annotations

import asyncio
import json
import sys
from pathlib import Path

from .client import LLMClient
from .types import ChatMessage, ChatRequest, GatewayConfig, MessageRole, ProviderType


async def async_main() -> None:
    import argparse

    parser = argparse.ArgumentParser(description="Bug Swarm LLM Gateway")
    sub = parser.add_subparsers(dest="command")

    # chat
    chat_p = sub.add_parser("chat", help="Send a chat request")
    chat_p.add_argument("--message", "-m", required=True, help="User message")
    chat_p.add_argument("--system", "-s", help="System prompt")
    chat_p.add_argument("--provider", "-p", default="openai", choices=["openai", "anthropic", "google", "ollama", "deepseek"])
    chat_p.add_argument("--model", help="Model override")
    chat_p.add_argument("--temperature", type=float, default=0.7)
    chat_p.add_argument("--max-tokens", type=int, default=1024)
    chat_p.add_argument("--json", action="store_true", help="Output raw JSON")

    # stats
    sub.add_parser("stats", help="Show gateway statistics")

    # hotswap
    swap_p = sub.add_parser("hotswap", help="Hotswap active provider")
    swap_p.add_argument("provider", choices=["openai", "anthropic", "google", "ollama"])

    # budget
    sub.add_parser("budget", help="Show budget status")

    # count-tokens
    count_p = sub.add_parser("count-tokens", help="Count tokens for a message")
    count_p.add_argument("--message", "-m", required=True)
    count_p.add_argument("--provider", "-p", default="openai")

    # test (Phase 3 Gate)
    sub.add_parser("test", help="Run Phase 3 Gate tests (mock providers)")

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        return

    config = GatewayConfig.from_env()
    client = LLMClient(config)
    client.register_default_adapters()

    if args.command == "chat":
        messages = []
        if hasattr(args, 'system') and args.system:
            messages.append(ChatMessage(role=MessageRole.SYSTEM, content=args.system))
        messages.append(ChatMessage(role=MessageRole.USER, content=args.message))

        request = ChatRequest(
            messages=messages,
            model=args.model,
            temperature=args.temperature,
            max_tokens=args.max_tokens,
        )

        provider = ProviderType(args.provider)
        response = await client.chat(request, provider)

        if args.json:
            print(response.model_dump_json(indent=2))
        else:
            print(response.content)

    elif args.command == "stats":
        print(json.dumps(client.registry.stats, indent=2))

    elif args.command == "hotswap":
        client.hotswap(ProviderType(args.provider))
        print(f"Active provider: {client.registry.active_provider.value}")

    elif args.command == "budget":
        print(json.dumps(client.budget_remaining, indent=2))

    elif args.command == "count-tokens":
        msg = ChatMessage(role=MessageRole.USER, content=args.message)
        provider = ProviderType(args.provider)
        count = await client.count_tokens([msg], provider)
        print(f"Token count: {count}")

    elif args.command == "test":
        from .tests.phase3_gate import run_phase3_gate
        await run_phase3_gate(client)


def main() -> None:
    asyncio.run(async_main())


if __name__ == "__main__":
    main()
