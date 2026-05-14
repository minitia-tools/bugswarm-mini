"""Subcommand dispatch — run, status, report, learn, dry-run."""

from __future__ import annotations

import asyncio
import json
import sys
import time

import structlog

from .config import CLIConfig
from .wiring import wire_everything
from .progress import emit_progress, emit_complete, emit_error, emit_finding
from .validation import PreFlight

logger = structlog.get_logger(__name__)


async def cmd_run(config: CLIConfig) -> int:
    """Full IEP loop. Returns exit code: 0=clean, 1=bugs found, 2=error."""
    # Pre-flight
    if not config.dry_run:
        warnings = await PreFlight.check_all(config)
        PreFlight.print_warnings(warnings)

    if config.dry_run:
        print("✓ Dry run successful — all checks passed" if not warnings else "✗ Issues found")
        return 0 if not warnings else 2

    # Wire
    engine, persistence, gateway, evidence = wire_everything(config)
    logger.info("bugswarm_run_start", repo=str(config.repo), persona=config.persona.value,
                model=config.model, provider=config.provider)

    # Run
    try:
        report = await engine.run()
    except Exception as e:
        logger.error("run_failed", error=str(e)[:500])
        if config.json_mode:
            emit_error("Run failed", str(e)[:500])
        return 2
    finally:
        persistence.close()

    # Output
    findings = report.findings
    verified = report.verified_count

    if config.json_mode:
        for f in findings:
            emit_finding(f.to_dict())
        emit_complete(report.to_dict())

    if config.output:
        with open(config.output, 'w') as f:
            if config.format == "sarif":
                try:
                    from swarm.observability import export_sarif
                    finding_dicts = [fd.to_dict() for fd in findings]
                    json.dump(export_sarif(finding_dicts), f, indent=2)
                except ImportError:
                    json.dump(report.to_dict(), f, indent=2)
            else:
                json.dump(report.to_dict(), f, indent=2)
        logger.info("report_saved", path=str(config.output), format=config.format)

    # Summary
    print(f"\n{'='*60}")
    print(f"  Bug Swarm Run Complete")
    print(f"{'='*60}")
    print(f"  Duration:  {report.duration_secs:.0f}s")
    print(f"  Findings:  {len(findings)} ({verified} verified)")
    print(f"  Tokens:    {report.tokens_consumed}")
    print(f"  Cost:      ${report.cost_usd:.4f}")
    print(f"  Persona:   {report.persona}")
    print(f"{'='*60}")

    return 1 if verified > 0 else 0


async def cmd_status(config: CLIConfig) -> int:
    """Print live state from persistence."""
    from agent.persistence import PersistenceManager
    pm = PersistenceManager(config.db_path)
    state = pm.get_run(config.run_id) if hasattr(config, 'run_id') else None
    if state:
        print(json.dumps({"running": state.status == "active",
                          "round": state.round_number, "turn": state.turn_number,
                          "findings": state.findings_count,
                          "verified": state.verified_findings,
                          "tokens": state.tokens_consumed}, indent=2))
    else:
        print(json.dumps({"running": False, "message": "No active run found"}, indent=2))
    pm.close()
    return 0


async def cmd_report(config: CLIConfig) -> int:
    """Export findings from a previous run."""
    from agent.persistence import PersistenceManager
    pm = PersistenceManager(config.db_path)
    findings = pm.get_findings("latest", verified_only=False)

    if config.format == "sarif":
        finding_dicts = [{"claim": f.claim, "location": f.location,
                          "mechanism": f.mechanism, "severity_estimate": f.severity,
                          "verified": f.verified} for f in findings]
        output = json.dumps(export_sarif(finding_dicts), indent=2)
    else:
        output = json.dumps([{"claim": f.claim, "location": f.location,
                              "severity": f.severity, "verified": f.verified}
                             for f in findings], indent=2)

    if config.output:
        config.output.write_text(output)
    else:
        print(output)
    pm.close()
    return 0


async def cmd_learn(config: CLIConfig) -> int:
    """Ingest a production incident for feedback learning."""
    try:
        from swarm.hardening import FeedbackEngine, ProductionIncident
    except ImportError:
        print(json.dumps({"status": "error", "message": "Hardening module not available"}))
        return 2
    fe = FeedbackEngine()
    incident_id = getattr(config, 'incident_id', 'manual')
    location = getattr(config, 'incident_location', 'unknown')
    bug_type = getattr(config, 'incident_type', 'unknown')
    severity = getattr(config, 'incident_severity', 5)

    incident = ProductionIncident(
        incident_id=incident_id, bug_location=location,
        bug_type=bug_type, severity=severity,
    )
    fe.ingest_incident(incident)
    adjustments = fe.auto_tune()
    print(json.dumps({"status": "learned", "adjustments": adjustments}, indent=2))
    return 0
