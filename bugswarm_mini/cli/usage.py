from __future__ import annotations

from rich.console import Console
from rich.panel import Panel
from rich.table import Table
from rich import box

from ..gateway.cost_tracker import UsageDB

console = Console()


async def cmd_usage(args: list[str] | None = None) -> int:
    db = UsageDB()
    summary = db.get_summary()

    if summary["total_runs"] == 0:
        console.print("[yellow]No usage data yet. Run [bold]bugswarm run[/] to start hunting bugs.[/]")
        return 0

    console.print()

    info_lines = [
        f"  [bold]Total runs:[/]       {summary['total_runs']}",
        f"  [bold]Total tokens:[/]    {_fmt_tokens(summary['total_tokens'])}",
        f"  [bold]Est. cost:[/]       ${summary['total_cost']:.4f} (est.)",
        f"  [bold]Models used:[/]     {summary['models_used']}",
        f"  [bold]Bugs found:[/]      {summary['total_verified_findings']}",
    ]

    console.print(Panel.fit(
        "[bold cyan]Usage Summary[/]\n\n" + "\n".join(info_lines),
        border_style="cyan",
    ))
    console.print()

    runs = db.get_recent_runs(limit=15)
    if runs:
        run_table = Table(box=box.ROUNDED, header_style="bold cyan", title="Recent Runs")
        run_table.add_column("Date", style="dim", width=12)
        run_table.add_column("Repo")
        run_table.add_column("Model", no_wrap=True)
        run_table.add_column("Tokens", justify="right")
        run_table.add_column("Est. $", justify="right")
        run_table.add_column("Bugs")

        for r in runs:
            date_str = r["started_at"][:10] if r["started_at"] else "?"
            tokens_str = _fmt_tokens(r["tokens_consumed"])
            cost_str = f"${r['estimated_cost_dollars']:.4f}" if r["estimated_cost_dollars"] else "-"
            bugs_str = str(r["verified_findings"]) if r["verified_findings"] else "0"
            repo = r["repo"] or "-"
            model = r["model"]
            run_table.add_row(date_str, repo, model, tokens_str, cost_str, bugs_str)

        console.print(run_table)
        console.print()

    models = db.get_model_breakdown()
    if models:
        model_table = Table(box=box.ROUNDED, header_style="bold cyan", title="Models Used")
        model_table.add_column("Model", no_wrap=True)
        model_table.add_column("Runs", justify="right")
        model_table.add_column("Tokens", justify="right")
        model_table.add_column("Est. $", justify="right")
        model_table.add_column("Bugs")

        for m in models:
            model_table.add_row(
                m["model"],
                str(m["runs"]),
                _fmt_tokens(m["tokens"]),
                f"${m['cost']:.4f}",
                str(m["findings"]),
            )

        console.print(model_table)
        console.print()

    stale_runs = [r for r in runs if r.get("stale_warning")]
    if stale_runs:
        console.print(
            "[yellow]⚠  Some runs used price estimates that may be stale. "
            "Run [bold]bugswarm configure --refresh[/] to update.[/]"
        )

    return 0


def _fmt_tokens(tokens: int) -> str:
    if tokens >= 1_000_000:
        return f"{tokens / 1_000_000:.1f}M"
    if tokens >= 1_000:
        return f"{tokens / 1_000:.1f}K"
    return str(tokens)
