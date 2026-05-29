from __future__ import annotations

import asyncio
import sys

import rich
from rich.console import Console
from rich.panel import Panel
from rich.prompt import Prompt, Confirm
from rich.table import Table
from rich.text import Text
from rich import box

from ..gateway.protocol import PROVIDER_PROTOCOLS, create_adapter, ProtocolAdapter
from ..gateway.registry import ModelRegistry
from ..gateway.config import (
    BugSwarmConfig,
    save_config,
    load_config,
    detect_api_key_provider,
)

console = Console()


def _format_context(ctx: int) -> str:
    if ctx >= 1_000_000:
        return f"{ctx // 1_000_000}M ctx"
    if ctx >= 1_000:
        return f"{ctx // 1_000}K ctx"
    return f"{ctx} ctx"


def _render_model_table(models: list, max_rows: int = 15) -> Table:
    table = Table(
        box=box.ROUNDED,
        show_header=True,
        header_style="bold cyan",
        title="Available Models",
    )
    table.add_column("#", style="dim", width=3)
    table.add_column("Model", style="bold", no_wrap=True)
    table.add_column("Provider", style="blue")
    table.add_column("Pricing (/1M tok)", style="green")
    table.add_column("Context", justify="right")
    table.add_column("Capabilities")
    table.add_column("")

    show_models = models[:max_rows]

    for i, m in enumerate(show_models, 1):
        caps_parts = []
        if m.capabilities.supports_tools:
            caps_parts.append("[bold green]tools[/]")
        if m.capabilities.supports_json_mode:
            caps_parts.append("json")
        if m.capabilities.supports_thinking:
            caps_parts.append("thinking")
        caps_str = ", ".join(caps_parts) if caps_parts else "[dim]none[/]"

        ctx_str = _format_context(m.context_window)

        pricing_str = m.pricing_label

        warning_str = ""
        if m.warning:
            warning_str = "[yellow]⚠[/]"
        if m.is_new:
            warning_str = "[bold yellow]NEW[/]"

        table.add_row(
            str(i),
            m.id,
            m.provider,
            pricing_str,
            ctx_str,
            caps_str,
            warning_str,
        )

    remaining = len(models) - len(show_models)
    if remaining > 0:
        table.add_row(
            "...",
            f"[dim]{remaining} more models[/]",
            "",
            "",
            "",
            "",
            "",
        )

    return table


def _print_header():
    console.print()
    console.print(Panel.fit(
        "[bold cyan]BugSwarm Configuration[/]\n\n"
        "[dim]Connect your model provider. Change later with [bold]bugswarm configure --edit[/][/]",
        border_style="cyan",
    ))
    console.print()


def _select_provider() -> str:
    providers = sorted(PROVIDER_PROTOCOLS.keys())
    providers.append("Other (OpenAI-compatible)")

    console.print("[bold]Select your provider:[/]\n")
    for i, p in enumerate(providers, 1):
        label = p
        if p == "Ollama":
            label += " [dim](local — no API key needed)[/]"
        elif p == "OpenRouter":
            label += " [dim](300+ models from every provider)[/]"
        console.print(f"  [cyan]{i:2d}[/]) {label}")

    console.print()
    choice = Prompt.ask("Select provider", default="2")

    try:
        idx = int(choice) - 1
        if 0 <= idx < len(providers):
            return providers[idx]
    except ValueError:
        pass

    return choice


def _select_model_interactive(models: list) -> str | None:
    if not models:
        console.print("[red]No models available from this provider.[/]")
        return None

    console.print("\n[dim]Fetching model list from provider...[/]\n")

    table = _render_model_table(models, max_rows=15)
    console.print(table)

    console.print()
    console.print("[dim]Enter model number, or 'a' to see all, or 'q' to quit[/]")
    choice = Prompt.ask("Select model", default="1")

    if choice.lower() == "q":
        return None
    if choice.lower() == "a":
        console.print()
        all_table = _render_model_table(models, max_rows=100)
        console.print(all_table)
        choice = Prompt.ask("Select model", default="1")
        if choice.lower() == "q":
            return None

    try:
        idx = int(choice) - 1
        if 0 <= idx < len(models):
            selected = models[idx]
            if selected.warning:
                console.print(f"\n[yellow]{selected.warning}[/]")
                if not Confirm.ask("Continue with this model?"):
                    return _select_model_interactive(models)
            return selected.id
    except (ValueError, IndexError):
        pass

    return choice


async def cmd_configure(args: list[str] | None = None) -> int:
    config = load_config()

    if "--edit" in (args or []):
        console.print("[yellow]Editing existing configuration...[/]")
    elif config.is_configured:
        console.print(f"[dim]Current config: {config.provider} / {config.model} ({config.api_key_display})[/]")
        if not Confirm.ask("Reconfigure?"):
            return 0

    _print_header()

    provider = _select_provider()
    if provider.lower() == "q":
        return 0

    if provider == "Other (OpenAI-compatible)":
        provider = Prompt.astr("Enter provider name (e.g., 'MyProvider')")
        base_url = Prompt.ask("Enter base URL (e.g., 'https://api.myprovider.com/v1')")
    else:
        base_url = None

    api_key = ""
    if provider != "Ollama":
        detected = None
        api_key = Prompt.ask("Paste your API key", password=True)
        detected = detect_api_key_provider(api_key)
        if detected and detected != provider:
            console.print(
                f"[yellow]Note: Your API key looks like it belongs to {detected}, "
                f"not {provider}. Continue anyway?[/]"
            )
            if not Confirm.ask("Continue?"):
                api_key = Prompt.ask("Paste the correct API key", password=True)

    console.print(f"\n[dim]Contacting {provider} API...[/]")

    try:
        adapter = create_adapter(provider, api_key, base_url)
    except ValueError as e:
        console.print(f"[red]{e}[/]")
        return 1

    registry = ModelRegistry()
    models = await registry.load(adapter=adapter, probe_new=True)

    if not models:
        console.print("[red]No models discovered from this provider. Check your API key.[/]")
        return 1

    known_count = sum(1 for m in models if m.is_pricing_known)
    new_count = len(models) - known_count

    console.print(
        f"[dim]Discovered {len(models)} models "
        f"({known_count} with pricing, {new_count} new/probed)[/]"
    )

    ranked = registry.rank_for_display(models)
    model_id = _select_model_interactive(ranked)
    if model_id is None:
        return 0

    bugswarm_config = BugSwarmConfig(
        provider=provider,
        api_key=api_key,
        base_url=base_url,
        model=model_id,
        budget_tokens=10_000_000,
    )

    save_config(bugswarm_config)

    console.print()
    console.print(Panel.fit(
        "[bold green]Configuration saved![/]\n\n"
        f"  [bold]Provider:[/]  {provider}\n"
        f"  [bold]Model:[/]     {model_id}\n"
        f"  [bold]Budget:[/]    {bugswarm_config.budget_tokens:,} tokens (default)\n\n"
        "[dim]To start hunting bugs:[/]  [bold]bugswarm run /path/to/repo[/]\n"
        "[dim]To view usage stats:[/]    [bold]bugswarm usage[/]\n"
        "[dim]To change later:[/]        [bold]bugswarm configure --edit[/]",
        border_style="green",
    ))

    return 0


async def cmd_show_config(args: list[str] | None = None) -> int:
    config = load_config()
    if not config.is_configured:
        console.print("[yellow]Not configured. Run [bold]bugswarm configure[/] first.[/]")
        return 1

    console.print(Panel.fit(
        "[bold]Current Configuration[/]\n\n"
        f"  [bold]Provider:[/]  {config.provider}\n"
        f"  [bold]Model:[/]     {config.model}\n"
        f"  [bold]API Key:[/]   {config.api_key_display}\n"
        f"  [bold]Budget:[/]    {config.budget_tokens:,} tokens\n"
        f"  [bold]Base URL:[/]  {config.base_url or '(default)'}",
        border_style="green",
    ))
    return 0
