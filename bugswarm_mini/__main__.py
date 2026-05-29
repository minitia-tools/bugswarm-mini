from __future__ import annotations

import asyncio
import sys

import click

from .cli.configure import cmd_configure, cmd_show_config
from .cli.usage import cmd_usage
from .gateway.config import load_config


@click.group()
@click.version_option(version="0.1.0", prog_name="bugswarm")
def cli():
    """BugSwarm — autonomous bug hunting with AI agents."""


@cli.command()
@click.option("--edit", is_flag=True, help="Edit existing configuration")
def configure(edit: bool):
    """Configure model provider and API key."""
    args = ["--edit"] if edit else []
    asyncio.run(cmd_configure(args))


@cli.command(name="config")
def show_config():
    """Show current configuration."""
    asyncio.run(cmd_show_config())


@cli.command()
@click.argument("repo", default=".")
@click.option("--budget", default=None, help="Token budget (e.g., '5M', '$2')")
@click.option("--unlimited", is_flag=True, help="No budget ceiling")
@click.option("--json", "json_output", is_flag=True, help="Output JSON")
def run(repo: str, budget: str | None, unlimited: bool, json_output: bool):
    """Hunt bugs in REPO."""
    config = load_config()
    if not config.is_configured:
        click.echo("Not configured. Run 'bugswarm configure' first.", err=True)
        sys.exit(1)

    click.echo(f"[dim]BugSwarm v0.1.0 — hunting bugs in {repo}[/]")
    click.echo(f"[dim]Model: {config.model} via {config.provider}[/]")
    click.echo("[dim]Run engine coming soon...[/]")


@cli.command()
@click.option("--json", "json_output", is_flag=True, help="Output JSON")
def usage(json_output: bool):
    """View usage statistics and billing."""
    asyncio.run(cmd_usage())


@cli.command()
def version():
    """Show version information."""
    from . import __version__

    click.echo(f"BugSwarm v{__version__}")


def main():
    cli()


if __name__ == "__main__":
    main()
