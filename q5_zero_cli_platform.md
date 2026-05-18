# BugSwarm Zero-CLI Platform: Architecture & Implementation Plan

## Executive Summary

BugSwarm Zero-CLI transforms BugSwarm from a command-line tool into a fully automated SaaS platform where users paste a GitHub URL and receive comprehensive security analysis results in a web dashboard — no CLI knowledge required. The platform comprises a FastAPI backend (`bugswarm-api`), a React frontend (`bugswarm-web`), a GitHub App for PR integration, a VS Code extension, chat integrations, and a Kubernetes-based deployment model. This plan covers 12 weeks with 5 engineers: 3 backend, 2 frontend.

---

## 1. Complete Architecture Diagram

```
+---------------------------------------------------------------------+
|                          USER ENTRY POINTS                           |
|                                                                     |
|  +------------+  +------------+  +------------+  +------------+    |
|  | Web Browser|  | GitHub PR  |  | VS Code    |  |Slack/Dscord|    |
|  |(React SPA) |  |(GitHub App)|  |(Extension) |  |(Webhooks)  |    |
|  +-----+------+  +-----+------+  +-----+------+  +-----+------+    |
|        |               |               |               |            |
|        v               v               v               v            |
|  +----------------------------------------------------------------+ |
|  |              API GATEWAY (Nginx / Traefik)                      | |
|  |  Rate Limiting | TLS | WAF | CORS | Request Routing             | |
|  +-----------------------------+-----------------------------------+ |
|                                |                                     |
|                                v                                     |
|  +----------------------------------------------------------------+ |
|  |                 BUGSWARM-API (FastAPI + Uvicorn)                 | |
|  |                                                                  | |
|  |  +-----------------+ +--------------+ +--------------------+    | |
|  |  | Auth Middleware  | | Rate Limiter | | Request Validator  |    | |
|  |  | (JWT+API Key+    | | (per-user,   | | (Pydantic v2       |    | |
|  |  |  GitHub OAuth)   | |  per-IP,     | |  models+OpenAPI)   |    | |
|  |  |                  | |  per-repo)   | |                    |    | |
|  |  +-----------------+ +--------------+ +--------------------+    | |
|  |                                                                  | |
|  |  REST Endpoints:                                                 | |
|  |  POST   /api/v1/scan              Submit scan job                | |
|  |  GET    /api/v1/scan/{id}          Scan status + metadata         | |
|  |  GET    /api/v1/scan/{id}/findings Paginated findings            | |
|  |  WS     /api/v1/scan/{id}/stream   Real-time analysis stream     | |
|  |  GET    /api/v1/scan/{id}/report   Download SARIF/JSON/PDF       | |
|  |  DELETE /api/v1/scan/{id}          Cancel/delete scan            | |
|  |  POST   /api/v1/auth/github        GitHub OAuth login            | |
|  |  POST   /api/v1/auth/refresh       JWT refresh                   | |
|  |  GET    /api/v1/user/me            User profile + quota          | |
|  |  GET    /api/v1/user/settings      Preferences                   | |
|  |  PUT    /api/v1/user/settings      Update preferences            | |
|  |  POST   /api/v1/webhook/github     GitHub App webhook receiver   | |
|  |  POST   /api/v1/webhook/slack      Slack command receiver        | |
|  |  GET    /api/v1/org/metrics        Org-level analytics           | |
|  |  POST   /api/v1/org/members        SSO team management           | |
|  +------------------------------+-----------------------------------+ |
|                                 |                                     |
|       +----------+----------+---+---+----------+                      |
|       v          v          v       v          v                      |
|  +--------+ +--------+ +--------+ +--------+ +--------------+        |
|  | Redis  | | Celery | |Postgres| |  S3    | |  Redis Pub/  |        |
|  | Cache  | |Worker  | |  DB    | |(MinIO) | |  Sub (WS)   |        |
|  | +Queue | |Cluster | |        | |        | |              |        |
|  +--------+ +----+---+ +--------+ +--------+ +--------------+        |
|                  |                                                     |
|                  v                                                     |
|  +------------------------------------------------------------------+ |
|  |             CELERY WORKER CLUSTER (Auto-scale)                    | |
|  |                                                                    | |
|  |  +------------+ +------------+ +------------+                     | |
|  |  |Repo Cloner | |CPG Builder | |Bug Detector|                     | |
|  |  |(git clone, | |(bugswarm-  | |(analyzer   |                     | |
|  |  | sparse c/o,| | cpg multi- | | engine, ML)|                     | |
|  |  | caching)   | | lang)      | |            |                     | |
|  |  +------------+ +------------+ +------------+                     | |
|  |                                                                    | |
|  |  +------------+ +------------+ +------------+                     | |
|  |  |Exploit Gen | |Report Bldr | |Notification|                     | |
|  |  |(PoC gen,   | |(SARIF,JSON,| |(email,Slack|                     | |
|  |  | mutation,  | | PDF,heatmap| | Discord,GH)|                     | |
|  |  | fuzzing)   | | )          | |            |                     | |
|  |  +------------+ +------------+ +------------+                     | |
|  +------------------------------------------------------------------+ |
|                                                                       |
|  +------------------------------------------------------------------+ |
|  |                EXTERNAL INTEGRATIONS                              | |
|  |                                                                    | |
|  |  GitHub App         VS Code Extension      Slack/Discord           | |
|  |  - PR annotations   - Inline findings      - /bugswarm scan        | |
|  |  - Status checks    - Status bar status    - Channel alerts        | |
|  |  - Auto-scan PRs    - Quick fix actions    - Scheduled scans       | |
|  +------------------------------------------------------------------+ |
|                                                                       |
|  +------------------------------------------------------------------+ |
|  |              DEPLOYMENT & INFRASTRUCTURE                           | |
|  |                                                                    | |
|  |  Terraform     Kubernetes    Helm       Prometheus  Grafana        | |
|  |  (AWS/GCP)     (EKS/GKE)    (Charts)   (Metrics)   (Dash)         | |
|  +------------------------------------------------------------------+ |
+---------------------------------------------------------------------+
```

### 1.2 Data Flow: End-to-End Scan Lifecycle

```
1. USER: Pastes GitHub URL "https://github.com/org/repo" on landing page
       |
2. FRONTEND: POST /api/v1/scan { url: "https://github.com/org/repo", branch: "main" }
       |
3. API: Validates JWT/auth, checks rate limit, creates scan record in Postgres,
        enqueues Celery task, returns { scan_id: "uuid", status: "queued" }
       |
4. FRONTEND: Opens WebSocket to WS /api/v1/scan/{scan_id}/stream
       |
5. CELERY WORKER (Repo Cloner):
       |-- Shallow clone repo to ephemeral volume
       |-- Detect language breakdown (bugswarm-cpg index)
       |-- Publish progress: "repo cloned, 154 files, 12 languages"
       |
6. CELERY WORKER (CPG Builder):
       |-- Run bugswarm-cpg on all source files in parallel
       |-- Build language-specific CPGs (AST, CallGraph, CFG, SSA, Taint)
       |-- Publish progress: "CPG built: 23,401 nodes, 6,502 edges"
       |
7. CELERY WORKER (Bug Detector):
       |-- Run taint analysis on all Tier 1/2 languages
       |-- Run pattern matching on Tier 3 languages
       |-- Cross-reference with CWE database
       |-- Publish real-time findings via Redis Pub/Sub
       |
8. FRONTEND: WebSocket receives each finding, updates heatmap in real-time
       |
9. CELERY WORKER (Exploit Generator):
       |-- For each HIGH/CRITICAL finding, attempt mutation-based PoC generation
       |-- Run fuzzer on suspicious inputs
       |-- Store reproduction case in S3
       |
10. CELERY WORKER (Report Builder):
       |-- Compile findings into SARIF v2.1.0
       |-- Generate JSON summary with remediation steps
       |-- Render PDF report with executive summary + detailed findings
       |-- Store all reports in S3
       |
11. CELERY WORKER (Notification):
       |-- If scan was triggered by GitHub PR: post findings as PR comments
       |-- If Slack integration is configured: post summary to channel
       |-- If email notifications are on: send digest email
       |
12. FRONTEND: Dashboard updates to "complete", shows final severity breakdown
```

---

## 2. Backend: `bugswarm-api/` — FastAPI Server

### 2.1 Project Structure

```
bugswarm-api/
|-- pyproject.toml                     # Poetry project config
|-- alembic.ini                        # Database migrations
|-- alembic/
|   |-- versions/                      # Migration scripts
|   |-- env.py
|-- app/
|   |-- __init__.py
|   |-- main.py                        # FastAPI app factory
|   |-- config.py                      # Settings (pydantic-settings)
|   |-- dependencies.py                # FastAPI dependency injection
|   |-- middleware/
|   |   |-- __init__.py
|   |   |-- auth.py                    # JWT + API Key + OAuth middleware
|   |   |-- rate_limit.py             # Tiered rate limiting
|   |   |-- cors.py                    # CORS config
|   |   |-- logging.py                 # Request/response logging
|   |   |-- request_id.py             # X-Request-ID injection
|   |-- models/
|   |   |-- __init__.py
|   |   |-- user.py                    # SQLAlchemy User model
|   |   |-- org.py                     # Organization / team model
|   |   |-- scan.py                    # Scan job model
|   |   |-- finding.py                # Bug finding model
|   |   |-- api_key.py                # API key model
|   |   |-- notification.py           # Notification preferences
|   |   |-- audit.py                   # Audit log model
|   |-- schemas/
|   |   |-- __init__.py
|   |   |-- scan.py                    # Pydantic v2 request/response schemas
|   |   |-- user.py                    # User schemas
|   |   |-- finding.py                # Finding schemas
|   |   |-- auth.py                    # Auth schemas
|   |   |-- webhook.py                # Webhook payload schemas
|   |   |-- report.py                 # Report export schemas
|   |-- routers/
|   |   |-- __init__.py
|   |   |-- scan.py                    # /api/v1/scan endpoints
|   |   |-- auth.py                    # /api/v1/auth endpoints
|   |   |-- user.py                    # /api/v1/user endpoints
|   |   |-- webhook.py                # /api/v1/webhook endpoints
|   |   |-- org.py                     # /api/v1/org endpoints
|   |-- services/
|   |   |-- __init__.py
|   |   |-- scan_service.py           # Orchestration: submit, cancel, status
|   |   |-- auth_service.py           # JWT issuance, OAuth flows
|   |   |-- github_service.py         # GitHub API client (octokit.py)
|   |   |-- notification_service.py   # Email, Slack, Discord dispatch
|   |   |-- report_service.py         # SARIF/JSON/PDF generation
|   |   |-- quota_service.py          # Scan limits, billing tiers
|   |   |-- websocket_service.py      # WebSocket manager
|   |-- tasks/
|   |   |-- __init__.py
|   |   |-- celery_app.py             # Celery configuration
|   |   |-- clone_task.py             # Repo cloning task
|   |   |-- cpg_task.py               # CPG construction task
|   |   |-- analyze_task.py           # Bug detection task
|   |   |-- exploit_task.py           # PoC generation task
|   |   |-- report_task.py            # Report building task
|   |   |-- notify_task.py            # Notification dispatch task
|   |   |-- cleanup_task.py           # Ephemeral volume cleanup
|   |-- integrations/
|   |   |-- __init__.py
|   |   |-- github_app.py             # GitHub App handler
|   |   |-- slack.py                  # Slack Bolt app
|   |   |-- discord.py                # Discord bot
|   |   |-- email.py                  # SendGrid / SES
|   |-- db/
|   |   |-- __init__.py
|   |   |-- session.py                # Async SQLAlchemy session
|   |   |-- redis.py                  # Redis connection pool
|   |   |-- s3.py                     # S3/MinIO client
|   |-- utils/
|   |   |-- __init__.py
|   |   |-- security.py               # Password hashing, token gen
|   |   |-- pagination.py             # Cursor-based pagination
|   |   |-- sanitize.py               # Input sanitization
|   |   |-- metrics.py                # Prometheus metrics
|-- tests/
|   |-- conftest.py                    # Fixtures (DB, Redis, test client)
|   |-- test_scan.py
|   |-- test_auth.py
|   |-- test_webhook.py
|   |-- test_websocket.py
|   |-- test_celery_tasks.py
|-- Dockerfile
|-- docker-compose.yml
|-- Makefile
```

### 2.2 Core API Endpoints (Exact Specifications)

#### POST /api/v1/scan — Submit Scan Job

```python
# app/schemas/scan.py
from pydantic import BaseModel, Field, field_validator
from typing import Optional, List
from enum import Enum
import re

class ScanScope(str, Enum):
    FULL = "full"
    CHANGED_FILES = "changed_files"
    DIFF_ONLY = "diff_only"

class ScanTrigger(str, Enum):
    MANUAL = "manual"
    PUSH = "push"
    PULL_REQUEST = "pull_request"
    SCHEDULED = "scheduled"

class ScanRequest(BaseModel):
    repo_url: str = Field(
        ...,
        description="GitHub repository URL",
        examples=["https://github.com/org/repo"],
        pattern=r"^https://github\.com/[\w.-]+/[\w.-]+$"
    )
    branch: str = Field(default="main", max_length=255)
    commit_sha: Optional[str] = Field(default=None, max_length=40)
    scope: ScanScope = Field(default=ScanScope.FULL)
    trigger: ScanTrigger = Field(default=ScanTrigger.MANUAL)
    pr_number: Optional[int] = Field(default=None, ge=1)
    languages: Optional[List[str]] = Field(
        default=None,
        description="Filter languages to scan. None = auto-detect all."
    )
    priority: int = Field(default=0, ge=0, le=10, description="Job priority")
    callback_url: Optional[str] = Field(default=None, description="Webhook URL for completion notification")

    @field_validator("branch")
    @classmethod
    def validate_branch(cls, v: str) -> str:
        if re.search(r"[^a-zA-Z0-9._/-]", v):
            raise ValueError("Branch name contains invalid characters")
        return v

class ScanResponse(BaseModel):
    scan_id: str = Field(..., description="UUID v7 scan identifier")
    status: str = Field(default="queued")
    repo_url: str
    branch: str
    created_at: str
    estimated_duration_seconds: int = Field(default=300)
    position_in_queue: int = Field(default=0)

# app/routers/scan.py
from fastapi import APIRouter, Depends, HTTPException, BackgroundTasks
from app.dependencies import get_current_user, get_rate_limiter
from app.services.scan_service import ScanService
from app.schemas.scan import ScanRequest, ScanResponse

router = APIRouter(prefix="/api/v1/scan", tags=["scan"])

@router.post("", response_model=ScanResponse, status_code=201)
async def submit_scan(
    request: ScanRequest,
    current_user = Depends(get_current_user),
    rate_limiter = Depends(get_rate_limiter),
    scan_service: ScanService = Depends(),
):
    await rate_limiter.check(
        user_id=current_user.id,
        repo=request.repo_url,
        limit=current_user.tier.scans_per_hour
    )

    scan = await scan_service.create_scan(
        user=current_user,
        request=request,
    )

    await scan_service.enqueue_scan(scan)

    return ScanResponse(
        scan_id=scan.id,
        status="queued",
        repo_url=request.repo_url,
        branch=request.branch,
        created_at=scan.created_at.isoformat(),
        estimated_duration_seconds=scan.estimated_duration,
        position_in_queue=await scan_service.get_queue_position(scan.id),
    )
```

#### GET /api/v1/scan/{id} — Scan Status

```python
@router.get("/{scan_id}", response_model=ScanStatusResponse)
async def get_scan_status(
    scan_id: str,
    current_user = Depends(get_current_user),
    scan_service: ScanService = Depends(),
):
    scan = await scan_service.get_scan(scan_id, user_id=current_user.id)
    if not scan:
        raise HTTPException(status_code=404, detail="Scan not found")

    return ScanStatusResponse(
        scan_id=scan.id,
        status=scan.status,
        progress_percent=scan.progress_percent,
        current_stage=scan.current_stage,
        findings_count=await scan_service.count_findings(scan.id),
        severity_breakdown=await scan_service.severity_breakdown(scan.id),
        started_at=scan.started_at.isoformat() if scan.started_at else None,
        completed_at=scan.completed_at.isoformat() if scan.completed_at else None,
    )
```

#### GET /api/v1/scan/{id}/findings — Paginated Findings

```python
@router.get("/{scan_id}/findings", response_model=FindingsPageResponse)
async def get_findings(
    scan_id: str,
    severity: Optional[str] = None,
    language: Optional[str] = None,
    cwe_id: Optional[str] = None,
    file_path: Optional[str] = None,
    status: Optional[str] = None,
    cursor: Optional[str] = None,
    limit: int = Query(default=50, ge=1, le=200),
    current_user = Depends(get_current_user),
    finding_service: FindingService = Depends(),
):
    findings, next_cursor, total = await finding_service.list_findings(
        scan_id=scan_id,
        user_id=current_user.id,
        severity=severity,
        language=language,
        cwe_id=cwe_id,
        file_path=file_path,
        status=status,
        cursor=cursor,
        limit=limit,
    )
    return FindingsPageResponse(
        findings=findings,
        next_cursor=next_cursor,
        total_count=total,
    )
```

#### WS /api/v1/scan/{id}/stream — Real-Time Stream

```python
from fastapi import WebSocket, WebSocketDisconnect
from app.services.websocket_service import WebSocketManager

ws_manager = WebSocketManager()

@router.websocket("/{scan_id}/stream")
async def scan_stream(
    websocket: WebSocket,
    scan_id: str,
    token: str = Query(...),
):
    user = await ws_manager.authenticate_websocket(token)
    if not user:
        await websocket.close(code=4001, reason="Authentication failed")
        return

    scan = await scan_service.verify_scan_access(scan_id, user.id)
    if not scan:
        await websocket.close(code=4004, reason="Scan not found")
        return

    await ws_manager.connect(scan_id, user.id, websocket)

    try:
        # Send current state immediately
        current_state = await scan_service.get_current_state(scan_id)
        await websocket.send_json({
            "type": "state_sync",
            "data": current_state.model_dump()
        })

        # Subscribe to Redis channel for real-time updates
        async with redis.pubsub() as pubsub:
            await pubsub.subscribe(f"scan:{scan_id}:events")

            async for message in pubsub.listen():
                if message["type"] == "message":
                    data = json.loads(message["data"])
                    await ws_manager.send_to_scan(scan_id, data)

                # Stay alive: client can send pings
                try:
                    client_msg = await asyncio.wait_for(
                        websocket.receive_text(), timeout=30
                    )
                    if client_msg == "ping":
                        await websocket.send_text("pong")
                except asyncio.TimeoutError:
                    continue

    except WebSocketDisconnect:
        pass
    finally:
        await ws_manager.disconnect(scan_id, user.id)
```

### 2.3 Authentication System

```python
# app/middleware/auth.py
from fastapi import Request, HTTPException, Depends
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
from jose import JWTError, jwt
from datetime import datetime, timedelta, timezone
from typing import Optional
import hashlib
import hmac

# JWT Configuration
JWT_ALGORITHM = "RS256"  # Asymmetric for distributed verification
ACCESS_TOKEN_EXPIRE_MINUTES = 30
REFRESH_TOKEN_EXPIRE_DAYS = 30

# API Key format: bswarm_<base64_32_bytes>
API_KEY_PREFIX = "bswarm_"

class AuthMiddleware:
    """Three authentication methods, checked in priority order:
    1. Bearer JWT (web UI sessions)
    2. X-API-Key header (CI/CD integrations)
    3. X-GitHub-Token (GitHub App JWT)
    """

    async def authenticate(self, request: Request) -> User:
        credentials = await self.extract_credentials(request)

        # Priority 1: Bearer JWT
        if credentials and credentials.scheme == "Bearer":
            return await self.verify_jwt(credentials.credentials)

        # Priority 2: X-API-Key header
        api_key = request.headers.get("X-API-Key")
        if api_key:
            return await self.verify_api_key(api_key)

        # Priority 3: GitHub App JWT
        gh_jwt = request.headers.get("X-GitHub-Token")
        if gh_jwt:
            return await self.verify_github_jwt(gh_jwt)

        raise HTTPException(status_code=401, detail="Authentication required")

    async def verify_jwt(self, token: str) -> User:
        try:
            payload = jwt.decode(
                token,
                settings.JWT_PUBLIC_KEY,
                algorithms=[JWT_ALGORITHM],
                options={"require": ["sub", "exp", "iat"]}
            )
            user_id = payload["sub"]
            session_id = payload.get("sid")

            # Check if session is still valid (not revoked)
            session_valid = await redis.get(f"session:{user_id}:{session_id}")
            if not session_valid:
                raise HTTPException(status_code=401, detail="Session expired")

            return await user_service.get_by_id(user_id)

        except JWTError:
            raise HTTPException(status_code=401, detail="Invalid token")

    async def verify_api_key(self, api_key: str) -> User:
        if not api_key.startswith(API_KEY_PREFIX):
            raise HTTPException(status_code=401, detail="Invalid API key format")

        # Constant-time comparison
        key_hash = hashlib.sha256(api_key.encode()).hexdigest()
        user_id = await redis.get(f"apikey:{key_hash}")

        if not user_id:
            # Fallback to DB lookup
            key_record = await db.query(ApiKey).filter(
                ApiKey.key_hash == key_hash,
                ApiKey.is_active == True,
            ).first()

            if not key_record:
                raise HTTPException(status_code=401, detail="Invalid API key")

            # Cache for 5 minutes
            await redis.setex(f"apikey:{key_hash}", 300, key_record.user_id)
            user_id = key_record.user_id

            # Update last_used
            await db.execute(
                update(ApiKey).where(ApiKey.id == key_record.id)
                .values(last_used_at=datetime.now(timezone.utc))
            )

        return await user_service.get_by_id(user_id)

    async def verify_github_jwt(self, gh_jwt: str) -> User:
        """Verify GitHub App installation token and resolve to user."""
        github_user = await github_service.get_user_from_installation_token(gh_jwt)
        user = await user_service.find_or_create_github_user(github_user)
        return user
```

### 2.4 Rate Limiting

```python
# app/middleware/rate_limit.py
from fastapi import Request, HTTPException
from typing import Dict, Tuple
import time

class TieredRateLimiter:
    """Rate limits with three dimensions: user, IP, repo."""

    def __init__(self, redis_client):
        self.redis = redis_client
        self.tiers = {
            "free": {
                "scans_per_hour": 5,
                "scans_per_day": 10,
                "concurrent_scans": 1,
                "max_repo_size_mb": 100,
                "max_files": 500,
            },
            "pro": {
                "scans_per_hour": 30,
                "scans_per_day": 100,
                "concurrent_scans": 5,
                "max_repo_size_mb": 500,
                "max_files": 5000,
            },
            "team": {
                "scans_per_hour": 100,
                "scans_per_day": 500,
                "concurrent_scans": 20,
                "max_repo_size_mb": 2000,
                "max_files": 50000,
            },
            "enterprise": {
                "scans_per_hour": 1000,
                "scans_per_day": 10000,
                "concurrent_scans": 100,
                "max_repo_size_mb": 10000,
                "max_files": 500000,
            },
        }

    async def check(
        self,
        user_id: str,
        repo: str,
        ip: Optional[str] = None,
    ):
        tier = await self.get_user_tier(user_id)
        limits = self.tiers[tier]

        # Check user hourly limit
        user_hour_key = f"ratelimit:user:{user_id}:hour"
        count = await self.redis.incr(user_hour_key)
        if count == 1:
            await self.redis.expire(user_hour_key, 3600)
        if count > limits["scans_per_hour"]:
            raise HTTPException(
                status_code=429,
                detail=f"User hourly limit exceeded ({limits['scans_per_hour']}/hour). Upgrade your plan.",
                headers={"Retry-After": str(await self.redis.ttl(user_hour_key))}
            )

        # Check concurrent scans
        concurrency_key = f"ratelimit:user:{user_id}:concurrent"
        active = await self.redis.scard(concurrency_key)
        if active >= limits["concurrent_scans"]:
            raise HTTPException(
                status_code=429,
                detail=f"Too many concurrent scans ({limits['concurrent_scans']} max)"
            )

        # Check per-repo rate limit (prevent spam on same repo)
        repo_hour_key = f"ratelimit:repo:{hash(repo)}:{user_id}:hour"
        repo_count = await self.redis.incr(repo_hour_key)
        if repo_count == 1:
            await self.redis.expire(repo_hour_key, 3600)
        if repo_count > max(1, limits["scans_per_hour"] // 3):
            raise HTTPException(
                status_code=429,
                detail="Too many scans on this repository. Please wait."
            )

        # Track concurrent scan
        await self.redis.sadd(concurrency_key, repo)
        # Will be cleaned up on scan completion
```

### 2.5 Celery Task Pipeline

```python
# app/tasks/celery_app.py
from celery import Celery
from app.config import settings

celery_app = Celery(
    "bugswarm",
    broker=settings.REDIS_URL,
    backend=settings.REDIS_URL,
    include=[
        "app.tasks.clone_task",
        "app.tasks.cpg_task",
        "app.tasks.analyze_task",
        "app.tasks.exploit_task",
        "app.tasks.report_task",
        "app.tasks.notify_task",
        "app.tasks.cleanup_task",
    ],
)

celery_app.conf.update(
    task_serializer="json",
    accept_content=["json"],
    result_serializer="json",
    timezone="UTC",
    enable_utc=True,
    task_track_started=True,
    task_time_limit=3600,       # 1 hour hard limit per task
    task_soft_time_limit=3300,  # 55 min soft limit
    worker_max_tasks_per_child=50,
    worker_prefetch_multiplier=1,
    task_acks_late=True,
    task_reject_on_worker_lost=True,
    result_expires=86400 * 7,   # Keep results 7 days
    # Task routing
    task_routes={
        "app.tasks.clone_task.*": {"queue": "clone"},
        "app.tasks.cpg_task.*": {"queue": "cpg"},
        "app.tasks.analyze_task.*": {"queue": "analyze"},
        "app.tasks.exploit_task.*": {"queue": "exploit"},
        "app.tasks.report_task.*": {"queue": "report"},
        "app.tasks.notify_task.*": {"queue": "notify"},
    },
)


# app/tasks/clone_task.py
from celery import chain, group, chord
from app.tasks.celery_app import celery_app
from app.services.redis_pubsub import publish_event

@celery_app.task(bind=True, name="clone_repo")
def clone_repo(self, scan_id: str, repo_url: str, branch: str):
    """Shallow clone repository to ephemeral volume."""
    publish_event(scan_id, {
        "type": "stage_change",
        "stage": "cloning",
        "message": f"Cloning {repo_url} ({branch})...",
        "progress": 0,
    })

    work_dir = Path(f"/tmp/bugswarm-scans/{scan_id}")
    work_dir.mkdir(parents=True, exist_ok=True)

    try:
        repo = git.Repo.clone_from(
            repo_url,
            work_dir,
            branch=branch,
            depth=1,       # Shallow clone
            single_branch=True,
            no_checkout=False,
        )

        # Run bugswarm-cpg index to detect languages
        result = subprocess.run(
            ["bugswarm-cpg", "index", str(work_dir), "--json"],
            capture_output=True, text=True, timeout=120
        )
        lang_breakdown = json.loads(result.stdout)

        publish_event(scan_id, {
            "type": "stage_change",
            "stage": "cloned",
            "message": f"Repository cloned: {lang_breakdown['total_files']} files, "
                       f"{len(lang_breakdown['languages'])} languages",
            "progress": 10,
            "data": lang_breakdown,
        })

        return {
            "scan_id": scan_id,
            "work_dir": str(work_dir),
            "languages": lang_breakdown["languages"],
            "total_files": lang_breakdown["total_files"],
            "commit": repo.head.commit.hexsha,
        }

    except Exception as e:
        publish_event(scan_id, {
            "type": "error",
            "stage": "cloning",
            "message": str(e),
        })
        raise


@celery_app.task(bind=True, name="build_cpg")
def build_cpg(self, scan_id: str, work_dir: str, languages: list):
    """Construct Code Property Graphs for all files."""
    publish_event(scan_id, {
        "type": "stage_change",
        "stage": "cpg_building",
        "message": f"Building CPG for {len(languages)} languages...",
        "progress": 15,
    })

    result = subprocess.run(
        ["bugswarm-cpg", "analyze", work_dir, "--format", "json", "--parallel"],
        capture_output=True, text=True, timeout=600
    )

    cpg_output = json.loads(result.stdout)

    publish_event(scan_id, {
        "type": "stage_change",
        "stage": "cpg_built",
        "message": f"CPG built: {cpg_output['node_count']} nodes, {cpg_output['edge_count']} edges",
        "progress": 40,
        "data": {
            "node_count": cpg_output["node_count"],
            "edge_count": cpg_output["edge_count"],
            "function_count": cpg_output["function_count"],
        },
    })

    return {
        "scan_id": scan_id,
        "cpg_path": cpg_output.get("output_path"),
        "summary": cpg_output,
    }


@celery_app.task(bind=True, name="detect_bugs")
def detect_bugs(self, scan_id: str, cpg_path: str, languages: list):
    """Run bug detection on the CPG."""
    publish_event(scan_id, {
        "type": "stage_change",
        "stage": "analyzing",
        "message": "Running taint analysis and bug detection...",
        "progress": 50,
    })

    findings = []
    analyzer = BugAnalyzer(cpg_path)

    total_funcs = analyzer.function_count()
    for i, finding in enumerate(analyzer.analyze_all()):
        findings.append(finding)

        # Publish each finding in real-time
        publish_event(scan_id, {
            "type": "finding",
            "finding": finding.model_dump(),
        })

        progress = 50 + int(40 * (i + 1) / total_funcs)
        if i % 100 == 0:
            publish_event(scan_id, {
                "type": "progress",
                "progress": progress,
                "findings_count": len(findings),
            })

    publish_event(scan_id, {
        "type": "stage_change",
        "stage": "analysis_complete",
        "message": f"Analysis complete: {len(findings)} findings",
        "progress": 90,
    })

    return {
        "scan_id": scan_id,
        "findings": findings,
        "count": len(findings),
    }


def run_scan_pipeline(scan_id: str, repo_url: str, branch: str):
    """Celery workflow: Chain clone -> cpg -> detect -> exploit -> report -> notify"""
    workflow = chain(
        clone_repo.s(scan_id, repo_url, branch),
        build_cpg.s(),
        detect_bugs.s(),
        generate_exploits.s(),
        build_reports.s(),
        send_notifications.s(),
    )
    workflow.apply_async()
```

### 2.6 Storage: S3-Compatible for Scan Artifacts

```python
# app/db/s3.py
import boto3
from botocore.config import Config
from app.config import settings

s3_client = boto3.client(
    "s3",
    endpoint_url=settings.S3_ENDPOINT,   # e.g., http://minio:9000
    aws_access_key_id=settings.S3_ACCESS_KEY,
    aws_secret_access_key=settings.S3_SECRET_KEY,
    config=Config(signature_version="s3v4"),
    region_name=settings.S3_REGION,
)

ARTIFACT_BUCKET = "bugswarm-scan-artifacts"

# Bucket layout:
# s3://bugswarm-scan-artifacts/
#   {user_id}/
#     {scan_id}/
#       receipt.json           # Scan metadata receipt
#       findings.sarif         # SARIF v2.1.0 report
#       report.pdf             # Human-readable PDF
#       report.json            # Machine-readable JSON
#       cpg/                   # Serialized CPG data
#         ast.bin
#         call_graph.bin
#         cfg.bin
#       poc/                   # Proof-of-concept exploit files
#         finding_{id}/
#           exploit.py
#           input.bin
#           crash.log
#       logs/                  # Execution logs
#         clone.log
#         cpg.log
#         analyze.log
#       repo.tar.gz            # Archived repo snapshot (optional)

def store_artifact(scan_id: str, user_id: str, key: str, data: bytes, content_type: str):
    """Store artifact in S3 with server-side encryption."""
    s3_client.put_object(
        Bucket=ARTIFACT_BUCKET,
        Key=f"{user_id}/{scan_id}/{key}",
        Body=data,
        ContentType=content_type,
        ServerSideEncryption="AES256",
        Metadata={
            "scan_id": scan_id,
            "user_id": user_id,
            "created_at": datetime.now(timezone.utc).isoformat(),
        },
    )

def get_artifact_url(scan_id: str, user_id: str, key: str, expires_in=3600):
    """Generate presigned URL for artifact download."""
    return s3_client.generate_presigned_url(
        "get_object",
        Params={
            "Bucket": ARTIFACT_BUCKET,
            "Key": f"{user_id}/{scan_id}/{key}",
        },
        ExpiresIn=expires_in,
    )
```

---

## 3. Frontend: `bugswarm-web/` — React + TypeScript + Tailwind

### 3.1 Project Structure

```
bugswarm-web/
|-- package.json
|-- tsconfig.json
|-- tailwind.config.ts
|-- vite.config.ts
|-- index.html
|-- public/
|   |-- favicon.svg
|   |-- og-image.png
|-- src/
|   |-- main.tsx                       # React 18 createRoot
|   |-- App.tsx                        # Root component with routing
|   |-- vite-env.d.ts
|   |-- api/
|   |   |-- client.ts                  # Axios instance with interceptors
|   |   |-- auth.ts                    # Auth API calls
|   |   |-- scan.ts                    # Scan API calls
|   |   |-- findings.ts               # Findings API calls
|   |   |-- websocket.ts              # WebSocket hook
|   |   |-- types.ts                   # Generated types from OpenAPI
|   |-- components/
|   |   |-- layout/
|   |   |   |-- AppShell.tsx           # Main layout with sidebar
|   |   |   |-- Sidebar.tsx            # Navigation sidebar
|   |   |   |-- TopBar.tsx             # Top bar with search + user menu
|   |   |   |-- Footer.tsx
|   |   |-- landing/
|   |   |   |-- Hero.tsx               # Main hero section
|   |   |   |-- RepoInput.tsx          # GitHub URL input + paste zone
|   |   |   |-- QuickStart.tsx         # Quick start guide cards
|   |   |   |-- Features.tsx           # Feature grid
|   |   |   |-- Pricing.tsx            # Pricing tiers
|   |   |-- dashboard/
|   |   |   |-- Dashboard.tsx          # Main dashboard container
|   |   |   |-- SeverityDonut.tsx      # Severity breakdown chart
|   |   |   |-- FindingTimeline.tsx    # Findings over time
|   |   |   |-- LanguageBreakdown.tsx  # Bug distribution by language
|   |   |   |-- TopCWEs.tsx            # Most common CWEs bar chart
|   |   |   |-- ScanHistory.tsx        # Recent scans list
|   |   |   |-- HeatmapWidget.tsx      # Mini heatmap preview
|   |   |   |-- OrgMetrics.tsx         # Team/organization metrics
|   |   |-- scan/
|   |   |   |-- ScanProgress.tsx       # Real-time scan progress
|   |   |   |-- ScanResults.tsx        # Results overview
|   |   |   |-- FindingsTable.tsx      # Sortable/filterable findings table
|   |   |   |-- FindingCard.tsx        # Individual finding detail card
|   |   |   |-- ExploitChain.tsx       # Exploit chain visualization
|   |   |-- codeviewer/
|   |   |   |-- CodeViewer.tsx         # Syntax-highlighted code viewer
|   |   |   |-- AnnotationLayer.tsx    # Bug annotations overlay
|   |   |   |-- FileTree.tsx           # Repository file tree
|   |   |   |-- DirtyDiff.tsx          # Changed lines indicator
|   |   |   |-- TaintPath.tsx          # Taint flow visualization overlay
|   |   |-- reports/
|   |   |   |-- ReportBuilder.tsx      # Custom report configuration
|   |   |   |-- SarifExport.tsx        # SARIF export options
|   |   |   |-- PdfExport.tsx          # PDF customization
|   |   |-- settings/
|   |   |   |-- ProfileSettings.tsx    # Profile editing
|   |   |   |-- ApiKeyManager.tsx      # API key generation and management
|   |   |   |-- Notifications.tsx      # Notification preferences
|   |   |   |-- ScanSchedule.tsx       # Scheduled scan configuration
|   |   |   |-- Integrations.tsx       # GitHub, Slack, Discord settings
|   |   |   |-- TeamManagement.tsx     # Organization team management
|   |   |-- common/
|   |   |   |-- Button.tsx
|   |   |   |-- Input.tsx
|   |   |   |-- Badge.tsx              # Severity badge
|   |   |   |-- Modal.tsx
|   |   |   |-- Toast.tsx
|   |   |   |-- Spinner.tsx
|   |   |   |-- EmptyState.tsx
|   |   |   |-- ErrorBoundary.tsx
|   |   |   |-- CopyButton.tsx
|   |-- hooks/
|   |   |-- useScan.ts                 # Scan lifecycle hook
|   |   |-- useWebSocket.ts            # WebSocket connection hook
|   |   |-- useFindings.ts             # Findings query hook
|   |   |-- useAuth.ts                 # Authentication hook
|   |   |-- useDebounce.ts
|   |   |-- useInfiniteScroll.ts
|   |-- stores/
|   |   |-- authStore.ts               # Zustand auth store
|   |   |-- scanStore.ts               # Zustand scan store
|   |   |-- uiStore.ts                 # UI state store
|   |-- utils/
|   |   |-- severity.ts                # Severity color mapping
|   |   |-- formatDate.ts
|   |   |-- download.ts                # File download helper
|   |   |-- repoParser.ts              # GitHub URL parsing
|   |-- pages/
|   |   |-- LandingPage.tsx
|   |   |-- DashboardPage.tsx
|   |   |-- ScanDetailPage.tsx
|   |   |-- CodeViewerPage.tsx
|   |   |-- ReportsPage.tsx
|   |   |-- SettingsPage.tsx
|   |   |-- OrgSettingsPage.tsx
|   |   |-- LoginPage.tsx
|   |   |-- NotFoundPage.tsx
|   |-- router.tsx                      # React Router v6 config
```

### 3.2 Landing Page

```typescript
// src/pages/LandingPage.tsx
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { RepoInput } from "@/components/landing/RepoInput";
import { Features } from "@/components/landing/Features";
import { Pricing } from "@/components/landing/Pricing";
import { useAuth } from "@/hooks/useAuth";
import { api } from "@/api/client";

export function LandingPage() {
  const { user, loginWithGitHub } = useAuth();
  const navigate = useNavigate();
  const [isScanning, setIsScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (repoUrl: string) => {
    if (!user) {
      // Redirect to GitHub OAuth, then come back
      await loginWithGitHub({ redirectAfter: `/?repo=${encodeURIComponent(repoUrl)}` });
      return;
    }

    setIsScanning(true);
    setError(null);

    try {
      const { scan_id } = await api.scans.submit({ repo_url: repoUrl });
      navigate(`/scan/${scan_id}`);
    } catch (err: any) {
      if (err.status === 429) {
        setError("You've reached your scan limit. Upgrade your plan or wait a moment.");
      } else {
        setError(err.message || "Failed to start scan. Please try again.");
      }
    } finally {
      setIsScanning(false);
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-gray-950 via-slate-950 to-indigo-950">
      <Hero onScan={handleSubmit} isScanning={isScanning} error={error} />
      <Features />
      <Pricing />
    </div>
  );
}


// src/components/landing/RepoInput.tsx
import { useState, useCallback } from "react";
import { motion } from "framer-motion";
import { FiGithub, FiUpload, FiZap } from "react-icons/fi";

interface RepoInputProps {
  onSubmit: (url: string) => void;
  isScanning: boolean;
  error: string | null;
}

export function RepoInput({ onSubmit, isScanning, error }: RepoInputProps) {
  const [url, setUrl] = useState("");
  const [dragOver, setDragOver] = useState(false);

  const isValid = /^https:\/\/github\.com\/[\w.-]+\/[\w.-]+/.test(url);

  const handlePaste = useCallback((e: React.ClipboardEvent) => {
    const pasted = e.clipboardData.getData("text").trim();
    if (pasted.startsWith("https://github.com/")) {
      setUrl(pasted);
    }
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    // Handle zip file drops
    const file = e.dataTransfer.files[0];
    if (file && file.name.endsWith(".zip")) {
      // Upload zip flow
    }
  }, []);

  return (
    <div className="max-w-2xl mx-auto">
      <div
        className={`
          relative group rounded-2xl border-2 transition-all duration-300
          ${dragOver ? "border-indigo-400 bg-indigo-950/30" : "border-gray-700 bg-gray-900/50"}
          ${isValid ? "ring-2 ring-indigo-500/50" : ""}
        `}
        onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
      >
        <div className="flex items-center gap-3 p-4">
          <FiGithub className="w-6 h-6 text-gray-400 flex-shrink-0" />
          <input
            type="url"
            placeholder="Paste your GitHub repo URL... (e.g., https://github.com/org/repo)"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onPaste={handlePaste}
            className="flex-1 bg-transparent text-lg text-white placeholder-gray-500 
                       outline-none border-none"
            autoFocus
          />
          <button
            onClick={() => onSubmit(url)}
            disabled={!isValid || isScanning}
            className={`
              flex items-center gap-2 px-6 py-3 rounded-xl font-semibold
              transition-all duration-200
              ${isValid && !isScanning
                ? "bg-indigo-600 hover:bg-indigo-500 text-white cursor-pointer shadow-lg shadow-indigo-500/25"
                : "bg-gray-800 text-gray-500 cursor-not-allowed"}
            `}
          >
            {isScanning ? (
              <div className="w-5 h-5 border-2 border-white/30 border-t-white rounded-full animate-spin" />
            ) : (
              <FiZap className="w-5 h-5" />
            )}
            {isScanning ? "Scanning..." : "Scan Now"}
          </button>
        </div>

        <div className="flex items-center gap-4 px-4 pb-4 text-sm text-gray-500">
          <span className="flex items-center gap-1">
            <FiUpload className="w-4 h-4" /> or upload a ZIP
          </span>
          <span>|</span>
          <button
            onClick={() => {/* Connect GitHub account flow */}}
            className="hover:text-indigo-400 transition-colors"
          >
            Connect GitHub to scan private repos
          </button>
        </div>
      </div>

      {error && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          className="mt-3 p-4 bg-red-950/50 border border-red-800 rounded-xl text-red-300 text-sm"
        >
          {error}
        </motion.div>
      )}
    </div>
  );
}
```

### 3.3 Real-Time Bug Heatmap

```typescript
// src/components/dashboard/HeatmapWidget.tsx
import { useEffect, useRef } from "react";
import * as d3 from "d3";

interface HeatmapDatum {
  filePath: string;
  lineStart: number;
  lineEnd: number;
  severity: "critical" | "high" | "medium" | "low";
  findingCount: number;
}

export function BugHeatmap({ findings, repoTree }: { findings: HeatmapDatum[]; repoTree: FileNode }) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current || findings.length === 0) return;

    const svg = d3.select(svgRef.current);
    const width = svgRef.current.clientWidth;
    const height = svgRef.current.clientHeight;

    svg.selectAll("*").remove();

    // Group findings by file
    const files = d3.group(findings, d => d.filePath);
    const fileEntries = Array.from(files.entries()).sort((a, b) => {
      const sevOrder = { critical: 4, high: 3, medium: 2, low: 1 };
      const maxSevA = d3.max(a[1], d => sevOrder[d.severity]) || 0;
      const maxSevB = d3.max(b[1], d => sevOrder[d.severity]) || 0;
      return maxSevB - maxSevA;
    });

    const colorScale = d3.scaleOrdinal<string>()
      .domain(["critical", "high", "medium", "low"])
      .range(["#ef4444", "#f97316", "#eab308", "#3b82f6"]);

    const rectHeight = Math.min(20, height / fileEntries.length - 2);
    const maxFileWidth = 200;
    const heatmapWidth = width - maxFileWidth - 20;

    // Draw file labels
    const labelGroup = svg.append("g");
    fileEntries.forEach(([path, fileFindings], i) => {
      const y = i * (rectHeight + 2);
      labelGroup.append("text")
        .attr("x", 0)
        .attr("y", y + rectHeight / 2)
        .attr("dy", "0.35em")
        .attr("fill", "#9ca3af")
        .attr("font-size", "11px")
        .attr("font-family", "monospace")
        .text(path.split("/").pop() || path)
        .append("title").text(path);
    });

    // Draw heatmap cells (each cell = a line range)
    fileEntries.forEach(([path, fileFindings], i) => {
      const y = i * (rectHeight + 2);
      const maxLine = d3.max(fileFindings, d => d.lineEnd) || 1;

      fileFindings.forEach(f => {
        const xStart = maxFileWidth + (f.lineStart / maxLine) * heatmapWidth;
        const cellWidth = Math.max(3, ((f.lineEnd - f.lineStart) / maxLine) * heatmapWidth);

        svg.append("rect")
          .attr("x", xStart)
          .attr("y", y)
          .attr("width", cellWidth)
          .attr("height", rectHeight)
          .attr("fill", colorScale(f.severity))
          .attr("rx", 2)
          .attr("opacity", 0.8)
          .attr("cursor", "pointer")
          .append("title")
          .text(`${path}:${f.lineStart}-${f.lineEnd}\n${f.severity.toUpperCase()}: ${f.findingCount} bug(s)`);
      });
    });

  }, [findings]);

  return <svg ref={svgRef} className="w-full h-full" />;
}
```

### 3.4 Code Viewer with Bug Annotations

```typescript
// src/components/codeviewer/CodeViewer.tsx
import { useMemo, useCallback } from "react";
import { codeToHtml } from "shiki";
import { AnnotationLayer } from "./AnnotationLayer";
import { TaintPath } from "./TaintPath";

interface CodeViewerProps {
  sourceCode: string;
  language: string;
  filePath: string;
  findings: Finding[];
  selectedFinding?: Finding;
  onLineClick?: (line: number) => void;
}

export function CodeViewer({
  sourceCode,
  language,
  filePath,
  findings,
  selectedFinding,
  onLineClick,
}: CodeViewerProps) {
  const lines = useMemo(() => sourceCode.split("\n"), [sourceCode]);

  const lineFindings = useMemo(() => {
    const map = new Map<number, Finding[]>();
    for (const f of findings) {
      for (let line = f.lineStart; line <= f.lineEnd; line++) {
        if (!map.has(line)) map.set(line, []);
        map.get(line)!.push(f);
      }
    }
    return map;
  }, [findings]);

  const getLineClass = useCallback((lineNum: number): string => {
    const lineFindings = findings.filter(
      f => lineNum >= f.lineStart && lineNum <= f.lineEnd
    );
    if (lineFindings.length === 0) return "";

    const severities = lineFindings.map(f => f.severity);
    if (severities.includes("critical")) return "bg-red-950/40 border-l-4 border-red-500";
    if (severities.includes("high")) return "bg-orange-950/30 border-l-4 border-orange-500";
    if (severities.includes("medium")) return "bg-yellow-950/20 border-l-4 border-yellow-500";
    return "bg-blue-950/15 border-l-4 border-blue-500";
  }, [findings]);

  return (
    <div className="relative font-mono text-sm bg-gray-950 rounded-xl border border-gray-800 overflow-hidden">
      {/* Toolbar */}
      <div className="flex items-center gap-4 px-4 py-2 bg-gray-900 border-b border-gray-800">
        <span className="text-gray-400 text-xs">{filePath}</span>
        <span className="text-gray-600 text-xs">{language}</span>
        <div className="flex-1" />
        <button className="text-xs text-gray-400 hover:text-white">Copy</button>
        <button className="text-xs text-gray-400 hover:text-white">Raw</button>
        <span className="text-xs text-gray-500">
          {findings.length} finding{findings.length !== 1 ? "s" : ""}
        </span>
      </div>

      {/* Code lines */}
      <div className="overflow-auto max-h-[70vh]">
        <table className="w-full border-collapse">
          <tbody>
            {lines.map((line, i) => {
              const lineNum = i + 1;
              const findingsOnLine = lineFindings.get(lineNum) || [];
              const isSelected = findingsOnLine.some(
                f => f.id === selectedFinding?.id
              );

              return (
                <tr
                  key={lineNum}
                  className={`
                    ${getLineClass(lineNum)}
                    ${isSelected ? "bg-indigo-950/40" : ""}
                    hover:bg-gray-900/50 cursor-pointer
                  `}
                  onClick={() => onLineClick?.(lineNum)}
                >
                  <td className="w-14 pr-4 text-right text-gray-600 select-none border-r border-gray-800 px-4">
                    {lineNum}
                  </td>
                  <td className="pl-4 relative">
                    <code className="whitespace-pre">{line}</code>
                    {/* Inline annotations */}
                    {findingsOnLine.map(f => (
                      <AnnotationLayer
                        key={f.id}
                        finding={f}
                        isStart={lineNum === f.lineStart}
                        isEnd={lineNum === f.lineEnd}
                      />
                    ))}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {/* Taint flow visualization overlay */}
      {selectedFinding?.taintPath && (
        <TaintPath
          taintPath={selectedFinding.taintPath}
          sourceCode={sourceCode}
        />
      )}
    </div>
  );
}


// src/components/codeviewer/AnnotationLayer.tsx
export function AnnotationLayer({ finding, isStart, isEnd }: AnnotationLayerProps) {
  const severityColors = {
    critical: { bg: "bg-red-500", text: "text-red-100", border: "border-red-400" },
    high: { bg: "bg-orange-500", text: "text-orange-100", border: "border-orange-400" },
    medium: { bg: "bg-yellow-500", text: "text-yellow-100", border: "border-yellow-400" },
    low: { bg: "bg-blue-500", text: "text-blue-100", border: "border-blue-400" },
  };
  const colors = severityColors[finding.severity];

  return (
    <div
      className={`
        absolute right-2 top-0 -translate-y-1/2
        flex items-center gap-1 px-2 py-0.5 rounded-full
        text-[10px] font-medium ${colors.text} ${colors.bg}
        cursor-pointer z-10 shadow-lg
      `}
      onClick={(e) => {
        e.stopPropagation();
        // Show finding detail tooltip
      }}
    >
      {finding.cwe_id && <span>{finding.cwe_id}</span>}
      <span>{finding.short_title}</span>
    </div>
  );
}
```

### 3.5 Settings: API Key Management

```typescript
// src/components/settings/ApiKeyManager.tsx
import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/api/client";
import { FiKey, FiCopy, FiTrash2, FiPlus, FiEye, FiEyeOff } from "react-icons/fi";
import { toast } from "react-hot-toast";

export function ApiKeyManager() {
  const queryClient = useQueryClient();
  const [showNewKey, setShowNewKey] = useState<string | null>(null);
  const [keyName, setKeyName] = useState("");
  const [revealedKeys, setRevealedKeys] = useState<Set<string>>(new Set());

  const { data: apiKeys, isLoading } = useQuery({
    queryKey: ["api-keys"],
    queryFn: () => api.user.listApiKeys(),
  });

  const createKey = useMutation({
    mutationFn: (name: string) => api.user.createApiKey(name),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ["api-keys"] });
      setShowNewKey(data.key); // Show key once — only time it's visible
      setKeyName("");
    },
  });

  const revokeKey = useMutation({
    mutationFn: (keyId: string) => api.user.revokeApiKey(keyId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["api-keys"] });
      toast.success("API key revoked");
    },
  });

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-white">API Keys</h2>
        <p className="text-gray-400 text-sm mt-1">
          Use API keys to authenticate with the BugSwarm API from CI/CD pipelines or scripts.
        </p>
      </div>

      {/* New key creation */}
      <div className="flex gap-3">
        <input
          type="text"
          placeholder="Key name (e.g., GitHub Actions CI)"
          value={keyName}
          onChange={(e) => setKeyName(e.target.value)}
          className="flex-1 bg-gray-900 border border-gray-700 rounded-lg px-4 py-2.5 
                     text-white text-sm focus:ring-2 focus:ring-indigo-500 outline-none"
        />
        <button
          onClick={() => createKey.mutate(keyName)}
          disabled={!keyName.trim() || createKey.isPending}
          className="flex items-center gap-2 px-4 py-2.5 bg-indigo-600 hover:bg-indigo-500 
                     disabled:bg-gray-800 disabled:text-gray-500 text-white rounded-lg 
                     text-sm font-medium transition-colors"
        >
          <FiPlus className="w-4 h-4" />
          Create Key
        </button>
      </div>

      {/* Show newly created key (one-time) */}
      {showNewKey && (
        <div className="p-4 bg-yellow-950/50 border border-yellow-800 rounded-xl">
          <p className="text-yellow-300 text-sm font-medium mb-2">
            Copy your API key now. You won't be able to see it again.
          </p>
          <div className="flex items-center gap-3">
            <code className="flex-1 bg-gray-950 px-4 py-2.5 rounded-lg text-indigo-400 
                           text-sm font-mono select-all break-all">
              {showNewKey}
            </code>
            <button
              onClick={() => {
                navigator.clipboard.writeText(showNewKey);
                toast.success("API key copied");
              }}
              className="p-2.5 bg-gray-800 hover:bg-gray-700 rounded-lg text-gray-400"
            >
              <FiCopy className="w-4 h-4" />
            </button>
          </div>
        </div>
      )}

      {/* Existing keys */}
      <div className="space-y-3">
        {apiKeys?.map((key) => (
          <div
            key={key.id}
            className="flex items-center gap-4 p-4 bg-gray-900/50 border border-gray-800 
                       rounded-xl hover:border-gray-700 transition-colors"
          >
            <FiKey className="w-5 h-5 text-gray-500" />
            <div className="flex-1 min-w-0">
              <p className="text-white font-medium">{key.name}</p>
              <p className="text-gray-500 text-xs mt-0.5">
                Created {new Date(key.created_at).toLocaleDateString()} ·
                Last used {key.last_used_at ? new Date(key.last_used_at).toLocaleDateString() : "never"}
              </p>
              <div className="flex items-center gap-3 mt-2 text-xs">
                <code className="text-gray-600 font-mono">
                  {revealedKeys.has(key.id)
                    ? key.prefix + key.suffix
                    : key.prefix + "..." + key.suffix.slice(-4)}
                </code>
                <button
                  onClick={() => {
                    setRevealedKeys(prev => {
                      const next = new Set(prev);
                      next.has(key.id) ? next.delete(key.id) : next.add(key.id);
                      return next;
                    });
                  }}
                  className="text-gray-500 hover:text-gray-300"
                >
                  {revealedKeys.has(key.id) ? <FiEyeOff className="w-3 h-3" /> : <FiEye className="w-3 h-3" />}
                </button>
              </div>
            </div>
            <span className={`
              px-2 py-1 rounded-full text-xs font-medium
              ${key.last_used_at
                ? "bg-green-950/50 text-green-400 border border-green-800"
                : "bg-gray-800 text-gray-500 border border-gray-700"}
            `}>
              {key.last_used_at ? "Active" : "Inactive"}
            </span>
            <button
              onClick={() => {
                if (confirm("Revoke this API key? This cannot be undone.")) {
                  revokeKey.mutate(key.id);
                }
              }}
              className="p-2 text-gray-600 hover:text-red-400 hover:bg-red-950/30 
                         rounded-lg transition-colors"
            >
              <FiTrash2 className="w-4 h-4" />
            </button>
          </div>
        ))}

        {apiKeys?.length === 0 && !isLoading && (
          <div className="text-center py-12 text-gray-500">
            <FiKey className="w-12 h-12 mx-auto mb-3 opacity-30" />
            <p>No API keys yet. Create one to get started.</p>
          </div>
        )}
      </div>
    </div>
  );
}
```

---

## 4. GitHub App

### 4.1 One-Click Installation + Auto-Scan on PR

```yaml
# github-app.yml (manifest for GitHub App registration)
name: BugSwarm
description: "Zero-effort code security. Paste a URL, get results."
url: "https://bugswarm.dev"
hook_attributes:
  url: "https://api.bugswarm.dev/api/v1/webhook/github"
  active: true
setup_url: "https://bugswarm.dev/setup/complete"
redirect_url: "https://bugswarm.dev/setup/callback"
public: true
default_permissions:
  contents: read
  pull_requests: write
  checks: write
  metadata: read
  issues: write
events:
  - pull_request
  - push
  - check_suite
  - check_run
```

```python
# app/integrations/github_app.py
from github import Github, GithubIntegration
from app.services.scan_service import ScanService
from app.services.redis_pubsub import publish_event

class GitHubAppHandler:
    def __init__(self, app_id: int, private_key: str, scan_service: ScanService):
        self.app_id = app_id
        self.private_key = private_key
        self.scan_service = scan_service
        self.integration = GithubIntegration(app_id, private_key)

    async def handle_pull_request(self, payload: dict):
        """Auto-scan on PR open/synchronize."""
        action = payload.get("action")
        if action not in ("opened", "synchronize", "reopened"):
            return

        pr = payload["pull_request"]
        repo = payload["repository"]
        installation_id = payload["installation"]["id"]

        # Start scan
        scan = await self.scan_service.create_scan(
            user=None,  # Anonymous GitHub App trigger
            request=ScanRequest(
                repo_url=repo["clone_url"],
                branch=pr["head"]["ref"],
                commit_sha=pr["head"]["sha"],
                scope=ScanScope.DIFF_ONLY,
                trigger=ScanTrigger.PULL_REQUEST,
                pr_number=pr["number"],
            ),
        )

        # Get installation client
        gh = self.integration.get_installation(installation_id)
        repo_obj = gh.get_repo(repo["full_name"])
        pull = repo_obj.get_pull(pr["number"])

        # Create pending check run
        check_run = repo_obj.create_check_run(
            name="BugSwarm Security Scan",
            head_sha=pr["head"]["sha"],
            status="in_progress",
            started_at=datetime.now(timezone.utc),
            output={
                "title": "BugSwarm scan in progress...",
                "summary": f"Analyzing {repo['full_name']} (PR #{pr['number']})",
            },
        )

        # Store check_run_id for later update
        await redis.set(f"scan:{scan.id}:check_run_id", check_run.id)

        # Enqueue scan pipeline
        await self.scan_service.enqueue_scan(scan)

        # Post initial PR comment
        pull.create_issue_comment(
            f"BugSwarm scan started: [`{scan.id[:8]}`](https://bugswarm.dev/scan/{scan.id})\n\n"
            f"Estimated completion: <1 minute"
        )

    async def update_check_run(self, scan_id: str, findings: list):
        """Update GitHub check run with scan results."""
        check_run_id = await redis.get(f"scan:{scan_id}:check_run_id")
        if not check_run_id:
            return

        # Group findings by severity
        severity_counts = {
            "critical": sum(1 for f in findings if f["severity"] == "critical"),
            "high": sum(1 for f in findings if f["severity"] == "high"),
            "medium": sum(1 for f in findings if f["severity"] == "medium"),
            "low": sum(1 for f in findings if f["severity"] == "low"),
        }
        total = sum(severity_counts.values())

        if total == 0:
            conclusion = "success"
            title = "No bugs found"
        elif severity_counts["critical"] > 0:
            conclusion = "failure"
            title = f"{severity_counts['critical']} critical bugs found"
        elif severity_counts["high"] > 0:
            conclusion = "failure"
            title = f"{severity_counts['high']} high-severity bugs found"
        else:
            conclusion = "neutral"
            title = f"{total} findings (no critical/high)"

        # Create annotations for inline PR comments
        annotations = []
        for f in findings[:50]:  # GitHub limits: 50 annotations per check run
            annotations.append({
                "path": f["file_path"],
                "start_line": f["line_start"],
                "end_line": f["line_end"],
                "annotation_level": self._severity_to_annotation_level(f["severity"]),
                "title": f"{f['cwe_id']}: {f['title']}",
                "message": f["description"][:500],
            })

        # Update check run
        check_run = gh_repo.get_check_run(int(check_run_id))
        check_run.edit(
            status="completed",
            conclusion=conclusion,
            completed_at=datetime.now(timezone.utc),
            output={
                "title": title,
                "summary": (
                    f"BugSwarm found **{total} potential issues** in this PR.\n\n"
                    f"| Severity | Count |\n|----------|-------|\n"
                    f"| Critical | {severity_counts['critical']} |\n"
                    f"| High | {severity_counts['high']} |\n"
                    f"| Medium | {severity_counts['medium']} |\n"
                    f"| Low | {severity_counts['low']} |\n\n"
                    f"[View full report](https://bugswarm.dev/scan/{scan_id})"
                ),
                "annotations": annotations,
            },
        )

    def _severity_to_annotation_level(self, severity: str) -> str:
        return {
            "critical": "failure",
            "high": "failure",
            "medium": "warning",
            "low": "notice",
        }.get(severity, "warning")
```

---

## 5. VS Code Extension

```json
// package.json (VS Code extension manifest)
{
  "name": "bugswarm",
  "displayName": "BugSwarm",
  "version": "1.0.0",
  "publisher": "bugswarm",
  "description": "Zero-effort code security. Scan files for vulnerabilities inline.",
  "engines": { "vscode": "^1.85.0" },
  "categories": ["Linters", "Other"],
  "activationEvents": [
    "onCommand:bugswarm.scanFile",
    "onCommand:bugswarm.scanWorkspace",
    "workspaceContains:.bugswarm.toml"
  ],
  "main": "./dist/extension.js",
  "contributes": {
    "commands": [
      {
        "command": "bugswarm.scanFile",
        "title": "BugSwarm: Scan Current File"
      },
      {
        "command": "bugswarm.scanWorkspace",
        "title": "BugSwarm: Scan Entire Workspace"
      },
      {
        "command": "bugswarm.showFindings",
        "title": "BugSwarm: Show Findings Panel"
      },
      {
        "command": "bugswarm.fixAll",
        "title": "BugSwarm: Apply Fixes"
      }
    ],
    "configuration": {
      "title": "BugSwarm",
      "properties": {
        "bugswarm.apiKey": {
          "type": "string",
          "description": "Your BugSwarm API key"
        },
        "bugswarm.scanOnSave": {
          "type": "boolean",
          "default": false,
          "description": "Scan file automatically on save"
        },
        "bugswarm.severityThreshold": {
          "type": "string",
          "default": "low",
          "enum": ["low", "medium", "high", "critical"],
          "description": "Minimum severity to report"
        },
        "bugswarm.annotations.enabled": {
          "type": "boolean",
          "default": true,
          "description": "Show inline annotations in editor"
        }
      }
    }
  }
}
```

```typescript
// src/extension.ts
import * as vscode from "vscode";
import { BugSwarmClient } from "./client";
import { DecoratorProvider } from "./decorator";
import { StatusBarManager } from "./statusBar";
import { FindingsPanel } from "./findingsPanel";

let client: BugSwarmClient;
let decorator: DecoratorProvider;
let statusBar: StatusBarManager;

export async function activate(context: vscode.ExtensionContext) {
  client = new BugSwarmClient(context);
  statusBar = new StatusBarManager();
  decorator = new DecoratorProvider();

  // Register commands
  context.subscriptions.push(
    vscode.commands.registerCommand("bugswarm.scanFile", async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) return;
      await scanCurrentFile(editor);
    }),

    vscode.commands.registerCommand("bugswarm.scanWorkspace", async () => {
      await scanWorkspace();
    }),

    vscode.commands.registerCommand("bugswarm.showFindings", () => {
      FindingsPanel.render(context.extensionUri);
    }),

    vscode.commands.registerCommand("bugswarm.fixAll", async () => {
      await applyAllFixes();
    }),
  );

  // Scan on save if enabled
  const config = vscode.workspace.getConfiguration("bugswarm");
  if (config.get("scanOnSave")) {
    context.subscriptions.push(
      vscode.workspace.onDidSaveTextDocument(async (doc) => {
        if (["python", "javascript", "typescript", "go", "rust", "java", "csharp", "php", "ruby"]
            .some(ext => doc.languageId === ext)) {
          await scanDocument(doc);
        }
      })
    );
  }

  // Listen for active editor changes
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(async (editor) => {
      if (editor) {
        await decorator.updateDecorations(editor, []);
      }
    })
  );

  statusBar.show();
}

async function scanCurrentFile(editor: vscode.TextEditor) {
  const document = editor.document;
  statusBar.setScanning(document.fileName);

  try {
    const findings = await client.scanDocument(document);
    decorator.updateDecorations(editor, findings);
    FindingsPanel.update(findings);

    if (findings.length === 0) {
      vscode.window.showInformationMessage("BugSwarm: No issues found.");
    } else {
      const criticals = findings.filter(f => f.severity === "critical").length;
      const highs = findings.filter(f => f.severity === "high").length;
      if (criticals > 0) {
        vscode.window.showErrorMessage(
          `BugSwarm: ${criticals} critical, ${highs} high-severity issues found.`
        );
      }
    }
  } catch (err) {
    vscode.window.showErrorMessage(`BugSwarm scan failed: ${err}`);
  } finally {
    statusBar.setIdle();
  }
}


// src/decorator.ts
import * as vscode from "vscode";

const severityDecorationTypes = {
  critical: vscode.window.createTextEditorDecorationType({
    backgroundColor: "rgba(239, 68, 68, 0.2)",
    borderColor: "rgba(239, 68, 68, 0.8)",
    borderStyle: "solid",
    borderWidth: "0 0 0 3px",
    overviewRulerColor: "rgba(239, 68, 68, 0.8)",
    overviewRulerLane: vscode.OverviewRulerLane.Right,
    after: {
      contentText: "CRITICAL",
      color: "rgba(239, 68, 68, 0.8)",
      margin: "0 0 0 8px",
      fontWeight: "bold",
      fontSize: "10px",
    },
  }),
  high: vscode.window.createTextEditorDecorationType({
    backgroundColor: "rgba(249, 115, 22, 0.15)",
    borderColor: "rgba(249, 115, 22, 0.6)",
    borderStyle: "solid",
    borderWidth: "0 0 0 3px",
    overviewRulerColor: "rgba(249, 115, 22, 0.6)",
    overviewRulerLane: vscode.OverviewRulerLane.Right,
  }),
  medium: vscode.window.createTextEditorDecorationType({
    backgroundColor: "rgba(234, 179, 8, 0.1)",
    overviewRulerColor: "rgba(234, 179, 8, 0.4)",
    overviewRulerLane: vscode.OverviewRulerLane.Right,
  }),
  low: vscode.window.createTextEditorDecorationType({
    overviewRulerColor: "rgba(59, 130, 246, 0.3)",
    overviewRulerLane: vscode.OverviewRulerLane.Right,
  }),
};

export class DecoratorProvider {
  private activeEditor: vscode.TextEditor | undefined;

  updateDecorations(editor: vscode.TextEditor, findings: Finding[]) {
    this.activeEditor = editor;

    const decorations = {
      critical: [] as vscode.DecorationOptions[],
      high: [] as vscode.DecorationOptions[],
      medium: [] as vscode.DecorationOptions[],
      low: [] as vscode.DecorationOptions[],
    };

    for (const f of findings) {
      if (f.file_path !== editor.document.fileName) continue;

      const range = new vscode.Range(
        f.line_start - 1, 0,
        f.line_end - 1, editor.document.lineAt(f.line_end - 1).text.length
      );

      const hoverMessage = new vscode.MarkdownString(
        `**${f.cwe_id}: ${f.title}**\n\n${f.description}\n\n---\n` +
        `*Severity:* ${f.severity} | *Confidence:* ${f.confidence}%\n\n` +
        `[View full details](https://bugswarm.dev/finding/${f.id})`
      );
      hoverMessage.isTrusted = true;

      decorations[f.severity].push({
        range,
        hoverMessage,
        renderOptions: {
          after: f.severity === "critical" ? {
            contentText: ` CWE-${f.cwe_id}`,
            color: "rgba(239, 68, 68, 0.6)",
            fontSize: "10px",
          } : undefined,
        },
      });
    }

    // Apply all decoration types
    for (const [severity, opts] of Object.entries(decorations)) {
      const type = severityDecorationTypes[severity as keyof typeof severityDecorationTypes];
      editor.setDecorations(type, opts);
    }
  }
}
```

---

## 6. Slack/Discord Integration

```python
# app/integrations/slack.py
from slack_bolt.async_app import AsyncApp
from slack_bolt.adapter.fastapi import AsyncSlackRequestHandler
from app.services.scan_service import ScanService
from app.config import settings

slack_app = AsyncApp(
    token=settings.SLACK_BOT_TOKEN,
    signing_secret=settings.SLACK_SIGNING_SECRET,
)
handler = AsyncSlackRequestHandler(slack_app)

@slack_app.command("/bugswarm")
async def bugswarm_command(ack, respond, command):
    await ack()

    args = command["text"].strip().split()
    if not args:
        await respond(
            "Usage:\n"
            "`/bugswarm scan <github-url>` - Scan a repository\n"
            "`/bugswarm status <scan-id>` - Check scan status\n"
            "`/bugswarm settings` - Configure notifications\n"
            "`/bugswarm help` - Show this help"
        )
        return

    subcommand = args[0].lower()

    if subcommand == "scan" and len(args) > 1:
        repo_url = args[1]
        scan = await scan_service.create_scan_from_slack(
            slack_user_id=command["user_id"],
            slack_channel=command["channel_id"],
            repo_url=repo_url,
        )

        await respond({
            "blocks": [
                {
                    "type": "header",
                    "text": {"type": "plain_text", "text": f"Scan started for {repo_url}"}
                },
                {
                    "type": "section",
                    "text": {"type": "mrkdwn", "text": f"Scan ID: `{scan.id[:8]}`"}
                },
                {
                    "type": "section",
                    "text": {"type": "mrkdwn", "text": f"Status: *Queued*\nETA: ~2 minutes"}
                },
                {
                    "type": "actions",
                    "elements": [
                        {
                            "type": "button",
                            "text": {"type": "plain_text", "text": "View Dashboard"},
                            "url": f"https://bugswarm.dev/scan/{scan.id}",
                        }
                    ]
                }
            ]
        })

    elif subcommand == "status" and len(args) > 1:
        scan_id = args[1]
        scan = await scan_service.get_scan(scan_id)
        if not scan:
            await respond(f"Scan `{scan_id}` not found.")
            return

        await respond({
            "blocks": [
                {
                    "type": "section",
                    "text": {"type": "mrkdwn", "text": f"*Scan {scan_id[:8]}* — {scan.status.upper()}"}
                },
                {
                    "type": "section",
                    "text": {"type": "mrkdwn", "text": (
                        f":red_circle: {scan.severity_counts.get('critical', 0)} Critical\n"
                        f":orange_circle: {scan.severity_counts.get('high', 0)} High\n"
                        f":yellow_circle: {scan.severity_counts.get('medium', 0)} Medium\n"
                        f":blue_circle: {scan.severity_counts.get('low', 0)} Low"
                    )}
                },
            ]
        })


@celery_app.task(name="post_slack_notification")
def post_slack_notification(scan_id: str, channel_id: str):
    """Post scan results to Slack channel."""
    scan = get_scan(scan_id)
    findings = get_findings(scan_id)

    slack_app.client.chat_postMessage(
        channel=channel_id,
        blocks=[
            {
                "type": "header",
                "text": {"type": "plain_text", "text": f"BugSwarm scan complete: {scan.repo_url}"}
            },
            {
                "type": "divider"
            },
            {
                "type": "section",
                "text": {"type": "mrkdwn", "text": (
                    f"*Summary:* {len(findings)} potential issues found\n"
                    f":red_circle: {sum(1 for f in findings if f.severity == 'critical')} Critical\n"
                    f":orange_circle: {sum(1 for f in findings if f.severity == 'high')} High\n"
                    f":yellow_circle: {sum(1 for f in findings if f.severity == 'medium')} Medium\n"
                    f":blue_circle: {sum(1 for f in findings if f.severity == 'low')} Low"
                )}
            },
            {
                "type": "actions",
                "elements": [
                    {
                        "type": "button",
                        "text": {"type": "plain_text", "text": "View Full Report"},
                        "url": f"https://bugswarm.dev/scan/{scan_id}",
                        "style": "primary",
                    },
                    {
                        "type": "button",
                        "text": {"type": "plain_text", "text": "Download SARIF"},
                        "url": f"https://bugswarm.dev/api/v1/scan/{scan_id}/report?format=sarif",
                    },
                ]
            },
        ],
    )
```

---

## 7. CI/CD: Terraform + Kubernetes

### 7.1 Terraform Infrastructure (AWS)

```hcl
# terraform/main.tf
terraform {
  required_version = ">= 1.5"
  required_providers {
    aws = { source = "hashicorp/aws", version = "~> 5.0" }
    kubernetes = { source = "hashicorp/kubernetes", version = "~> 2.23" }
    helm = { source = "hashicorp/helm", version = "~> 2.12" }
  }

  backend "s3" {
    bucket = "bugswarm-tfstate"
    key    = "prod/terraform.tfstate"
    region = "us-east-1"
  }
}

provider "aws" { region = var.aws_region }

module "vpc" {
  source = "terraform-aws-modules/vpc/aws"
  name   = "bugswarm-vpc"
  cidr   = "10.0.0.0/16"
  azs    = ["us-east-1a", "us-east-1b", "us-east-1c"]
  private_subnets = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
  public_subnets  = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]
  enable_nat_gateway = true
  single_nat_gateway = false
}

module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "~> 19.0"

  cluster_name    = "bugswarm-prod"
  cluster_version = "1.29"
  vpc_id          = module.vpc.vpc_id
  subnet_ids      = module.vpc.private_subnets

  node_groups = {
    api = {
      desired_capacity = 3
      max_capacity     = 10
      min_capacity     = 2
      instance_types   = ["c6i.xlarge"]
      labels = { role = "api" }
    }
    workers = {
      desired_capacity = 5
      max_capacity     = 50
      min_capacity     = 2
      instance_types   = ["c6i.2xlarge", "c6i.4xlarge"]
      labels = { role = "worker" }
      taints = [{ key = "dedicated", value = "worker", effect = "NO_SCHEDULE" }]
    }
    gpu = {
      desired_capacity = 1
      max_capacity     = 5
      min_capacity     = 0
      instance_types   = ["g5.xlarge"]
      labels = { role = "gpu" }
      taints = [{ key = "nvidia.com/gpu", value = "true", effect = "NO_SCHEDULE" }]
    }
  }
}

resource "aws_db_instance" "postgres" {
  engine         = "postgres"
  engine_version = "16.1"
  instance_class = "db.r6g.xlarge"
  allocated_storage    = 200
  max_allocated_storage = 1000
  storage_encrypted    = true
  multi_az             = true
  publicly_accessible  = false
  vpc_security_group_ids = [aws_security_group.postgres.id]
  db_subnet_group_name   = aws_db_subnet_group.main.name

  backup_retention_period = 30
  backup_window           = "03:00-04:00"
  maintenance_window      = "sun:04:00-sun:05:00"

  username = "bugswarm_admin"
  password = random_password.postgres.result

  skip_final_snapshot = false
  final_snapshot_identifier = "bugswarm-postgres-final"
}

resource "aws_elasticache_cluster" "redis" {
  cluster_id           = "bugswarm-redis"
  engine              = "redis"
  node_type           = "cache.r6g.large"
  num_cache_nodes     = 2
  parameter_group_name = "default.redis7"
  port                = 6379
  subnet_group_name   = aws_elasticache_subnet_group.main.name
  security_group_ids  = [aws_security_group.redis.id]
}

resource "aws_s3_bucket" "artifacts" {
  bucket = "bugswarm-scan-artifacts"
  force_destroy = false
}

resource "aws_s3_bucket_versioning" "artifacts" {
  bucket = aws_s3_bucket.artifacts.id
  versioning_configuration { status = "Enabled" }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "artifacts" {
  bucket = aws_s3_bucket.artifacts.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "aws:kms"
    }
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "artifacts" {
  bucket = aws_s3_bucket.artifacts.id
  rule {
    id = "expire-old-scans"
    status = "Enabled"
    expiration { days = 90 }
    filter { prefix = "" }
  }
}

# Route53 + ACM for TLS
resource "aws_acm_certificate" "bugswarm" {
  domain_name       = "bugswarm.dev"
  validation_method = "DNS"
  subject_alternative_names = ["*.bugswarm.dev", "api.bugswarm.dev"]
}

resource "aws_route53_zone" "bugswarm" {
  name = "bugswarm.dev"
}
```

### 7.2 Kubernetes Helm Chart

```yaml
# helm/bugswarm/Chart.yaml
apiVersion: v2
name: bugswarm
description: BugSwarm Zero-CLI Platform
type: application
version: 1.0.0
appVersion: "1.0.0"
dependencies:
  - name: redis
    version: "18.x"
    repository: "https://charts.bitnami.com/bitnami"
    condition: redis.enabled
  - name: postgresql
    version: "13.x"
    repository: "https://charts.bitnami.com/bitnami"
    condition: postgresql.enabled
  - name: minio
    version: "5.x"
    repository: "https://charts.min.io/"
    condition: minio.enabled
```

```yaml
# helm/bugswarm/values.yaml
replicaCount:
  api: 3
  worker: 5
  websocket: 2

images:
  api:
    repository: ghcr.io/bugswarm/bugswarm-api
    tag: "1.0.0"
    pullPolicy: IfNotPresent
  worker:
    repository: ghcr.io/bugswarm/bugswarm-worker
    tag: "1.0.0"
    pullPolicy: IfNotPresent
  web:
    repository: ghcr.io/bugswarm/bugswarm-web
    tag: "1.0.0"
    pullPolicy: IfNotPresent

api:
  resources:
    requests: { cpu: "500m", memory: "512Mi" }
    limits: { cpu: "2000m", memory: "2Gi" }
  env:
    DATABASE_URL: "postgresql+asyncpg://user:pass@postgres:5432/bugswarm"
    REDIS_URL: "redis://redis:6379/0"
    S3_ENDPOINT: "http://minio:9000"
    GITHUB_APP_ID: "123456"
    JWT_PUBLIC_KEY_PATH: "/secrets/jwt-public.pem"
  autoscaling:
    enabled: true
    minReplicas: 2
    maxReplicas: 20
    targetCPUUtilizationPercentage: 70
    targetMemoryUtilizationPercentage: 80

worker:
  resources:
    requests: { cpu: "1000m", memory: "2Gi" }
    limits: { cpu: "4000m", memory: "8Gi" }
  env:
    CELERY_CONCURRENCY: "4"
    MAX_FILE_SIZE_MB: "10"
    SCAN_TIMEOUT_SECONDS: "3600"
  autoscaling:
    enabled: true
    minReplicas: 2
    maxReplicas: 50
    targetCPUUtilizationPercentage: 65
    scaleUp:
      stabilizationWindowSeconds: 30
      policies:
        - type: Pods, value: 3, periodSeconds: 60
    scaleDown:
      stabilizationWindowSeconds: 300

web:
  resources:
    requests: { cpu: "100m", memory: "128Mi" }
    limits: { cpu: "500m", memory: "512Mi" }
  env:
    VITE_API_URL: "https://api.bugswarm.dev"
    VITE_WS_URL: "wss://api.bugswarm.dev"
    VITE_GITHUB_CLIENT_ID: "Iv1.xxxxxx"

ingress:
  enabled: true
  className: "nginx"
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
    nginx.ingress.kubernetes.io/proxy-body-size: "100m"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "3600"
    nginx.ingress.kubernetes.io/proxy-send-timeout: "3600"
    nginx.ingress.kubernetes.io/websocket-services: "bugswarm-api"
  hosts:
    - host: bugswarm.dev
      paths:
        - path: /
          pathType: Prefix
          service: bugswarm-web
          port: 80
    - host: api.bugswarm.dev
      paths:
        - path: /
          pathType: Prefix
          service: bugswarm-api
          port: 8000
  tls:
    - secretName: bugswarm-tls
      hosts:
        - bugswarm.dev
        - api.bugswarm.dev

monitoring:
  enabled: true
  serviceMonitor:
    enabled: true
    interval: 30s
  grafanaDashboard:
    enabled: true
    labels: { grafana_dashboard: "1" }
```

---

## 8. Implementation Timeline — 12 Weeks, 5 Engineers

### Week 1-2: Foundation

| Task | Owner(s) | Duration | Deliverable |
|------|---------|----------|------------|
| Project scaffolding: FastAPI + React monorepo | E1 (BE), E4 (FE) | 2d | CI/CD pipeline green |
| Database schema design + Alembic migrations | E1 | 2d | All SQLAlchemy models |
| Auth middleware: JWT + API key + GitHub OAuth | E2 | 3d | Auth module with tests |
| Rate limiting + Redis integration | E2 | 2d | Tiered rate limiter |
| Frontend routing + layout + design system | E4 | 3d | AppShell, Sidebar, theme |
| Landing page (Hero + RepoInput) | E4 | 2d | Polished landing page |
| Celery setup + broker configuration | E3 | 2d | Task queue operational |
| Docker Compose local dev environment | E1 | 1d | `docker compose up` works |
| AWS account + Terraform initial setup | E5 (DevOps) | 3d | VPC + EKS cluster |
| GitHub OAuth flow (FE + BE) | E2, E4 | 2d | Login with GitHub |

### Week 3-4: Core Scan Pipeline

| Task | Owner(s) | Duration | Deliverable |
|------|---------|----------|------------|
| POST /scan endpoint + scan service | E1 | 2d | Scan submission API |
| Repo cloning task (shallow clone + language detection) | E3 | 2d | Clone task |
| CPG builder task (integrate bugswarm-cpg binary) | E3 | 3d | CPG task |
| Bug analyzer task (taint + pattern matching) | E2 | 4d | Findings generation |
| WebSocket manager + real-time event streaming | E1 | 3d | WS /scan/{id}/stream |
| Scan status endpoint + progress tracking | E1 | 1d | GET /scan/{id} |
| Findings endpoint with pagination + filtering | E1 | 2d | GET /scan/{id}/findings |
| Dashboard page (heatmap, severity donut, timeline) | E4 | 4d | Interactive dashboard |
| Scan detail page (progress bar + findings table) | E5 (FE) | 3d | Scan results view |
| S3 artifact storage (receipts, PoCs, reports) | E3 | 2d | S3 integration |

### Week 5-6: Code Viewer + Reports

| Task | Owner(s) | Duration | Deliverable |
|------|---------|----------|------------|
| Code viewer (Shiki highlighting + annotations) | E4 | 4d | Syntax-highlighted code viewer |
| Annotation overlay (bug badges + taint paths) | E4 | 3d | Inline bug annotations |
| File tree browser | E5 (FE) | 2d | Repository file navigator |
| Exploit chain visualization (D3 force graph) | E4 | 2d | Interactive exploit chains |
| Report builder (SARIF v2.1.0 export) | E2 | 3d | SARIF export |
| PDF report generation (WeasyPrint) | E2 | 2d | PDF reports |
| JSON report export | E2 | 1d | Machine-readable export |
| Download report endpoint | E1 | 1d | GET /scan/{id}/report |
| Report page (customize + preview) | E5 (FE) | 2d | Report configuration UI |
| Backend test suite (90% coverage target) | E1 | 3d | Pytest suite |

### Week 7-8: GitHub App + Integrations

| Task | Owner(s) | Duration | Deliverable |
|------|---------|----------|------------|
| GitHub App manifest + registration | E2 | 1d | App registration |
| Webhook receiver + signature verification | E2 | 2d | POST /webhook/github |
| PR auto-scan on open/synchronize | E2 | 2d | Automated PR scanning |
| Check run creation + annotation posting | E2 | 2d | Inline PR annotations |
| PR comment with summary | E2 | 1d | Automated PR comments |
| GitHub App settings page in FE | E5 (FE) | 2d | Installation management |
| VS Code extension scaffolding | E3 | 2d | Extension boots |
| VS Code decorator + inline annotations | E3 | 3d | Inline editor bugs |
| VS Code findings panel (webview) | E3 | 2d | Sidebar findings list |
| Slack bolt app + /bugswarm command | E1 | 2d | Slack integration |
| Slack notification task | E1 | 1d | Post-scan alerts |
| Discord bot integration | E2 | 1d | Discord integration |
| Terraform prod environment | E5 (DevOps) | 3d | Production EKS + DBs |

### Week 9-10: Settings + Enterprise

| Task | Owner(s) | Duration | Deliverable |
|------|---------|----------|------------|
| User settings page (profile, preferences) | E5 (FE) | 2d | Settings management |
| API key manager (create, revoke, audit) | E4 | 2d | API key UI |
| Notification preferences (email, Slack, Discord) | E1, E4 | 2d | Notification settings |
| Scheduled scans (cron-based) | E2 | 3d | Recurring scans |
| Organization/team management | E1 | 3d | Team management API |
| SSO integration (SAML/OIDC) | E2 | 3d | Enterprise SSO |
| Audit logging | E1 | 2d | All actions logged |
| Rate limit + quota enforcement | E2 | 2d | Tier enforcement |
| Prometheus metrics instrumentation | E3 | 2d | Metrics endpoint |
| Grafana dashboards | E5 (DevOps) | 2d | Operational dashboards |
| Helm chart polishing + CI/CD pipeline | E5 (DevOps) | 3d | One-command deploy |

### Week 11-12: Polish + Post-MVP

| Task | Owner(s) | Duration | Deliverable |
|------|---------|----------|------------|
| End-to-end testing (Playwright) | E4, E5 (FE) | 3d | Critical path tests |
| Load testing (k6) | E5 (DevOps) | 2d | 1k concurrent scans |
| Performance optimization (FE: code splitting, BE: query tuning) | All | 3d | < 100ms p95 latency |
| Accessibility audit (WCAG 2.1 AA) | E4 | 2d | a11y compliance |
| Bug bash + stabilization | All | 3d | < 10 known bugs |
| Documentation: API reference, setup guide, user guide | E1 | 3d | Public docs site |
| Landing page + marketing site | E4, E5 (FE) | 2d | Polished marketing |
| Security review (pentest) | External | 5d | Third-party audit |
| Marketplace architecture design | E1, E2 | 2d | Plugin API spec |
| Beta launch + invite system | All | 2d | Closed beta |

---

## 9. Post-MVP Roadmap

### Phase 2 (Month 4-6): Marketplace

- **Plugin API**: Allow third-party developers to write custom analyzers, taint models, and mutation operators.
- **Plugin registry**: npm-style registry for sharing plugins. Users browse + install with one click.
- **Built-in plugins**:
  - Dependency vulnerability scanner (CVE database cross-reference)
  - License compliance checker
  - Infrastructure-as-Code scanner (Terraform, CloudFormation, Pulumi)
  - Secrets/password leak detector
  - SBOM generation (SPDX, CycloneDX)
- **Revenue model**: Free plugins + premium verified plugins with 70/30 revenue share.

### Phase 3 (Month 7-9): Enterprise SSO + Audit

- **SAML/OIDC SSO**: Okta, Azure AD, Google Workspace, OneLogin
- **SCIM provisioning**: Automated user lifecycle management
- **Audit logging**: Every action across the org logged with immutable audit trail
- **SIEM integration**: Ship audit logs to Splunk, ELK, Datadog
- **Compliance reporting**: SOC 2, ISO 27001, HIPAA, PCI DSS readiness reports
- **Data residency**: Per-tenant region selection (US, EU, APAC)
- **Private instances**: Dedicated VPC deployment for large enterprises
- **Role-based access control**: Admin, Security Lead, Developer, Viewer roles
- **Custom policies**: Org-wide severity thresholds, auto-merge criteria for PRs

### Phase 4 (Month 10-12): AI + Advanced Analysis

- **LLM-powered explanation**: GPT-4/Claude generates human-readable explanations for each finding
- **LLM-powered fix generation**: Automated code fix suggestions with confidence scores
- **Anomaly detection**: Unsupervised learning to detect unusual patterns (zero-day potential)
- **Predictive analytics**: "What bugs is your team most likely to introduce next sprint?"
- **Cross-repo correlation**: "This same bug pattern was found in 3 other repos in your org"
- **Developer training**: Personalized learning paths based on bug history

---

## 10. Key Metrics & KPIs

| Metric | Target | Measurement |
|--------|--------|-------------|
| Time from URL paste to first finding | < 60 seconds | p95 latency |
| Complete scan time (100k LOC) | < 5 minutes | p95 latency |
| WebSocket event latency | < 100ms | p95 latency |
| API response time (GET scan status) | < 50ms | p95 latency |
| Findings accuracy (true positive rate) | > 85% | Manual review sample |
| Findings completeness (false negative rate) | < 20% | Juliet test suites |
| Platform uptime | 99.9% | 3-zone HA |
| Docker image size (API) | < 200MB | `docker images` |
| Frontend bundle size (initial load) | < 300KB gzipped | Webpack analyzer |
| Lighthouse score | > 95 | Google Lighthouse |
| User onboarding time | < 30 seconds | Analytics |
| GitHub App installation time | < 10 seconds | GitHub API |

---

## 11. Summary

| Dimension | Value |
|-----------|-------|
| Backend technology | FastAPI + Celery + Redis + Postgres + S3 |
| Frontend technology | React 18 + TypeScript + Tailwind + Vite |
| Deployment | Kubernetes (EKS) + Terraform + Helm |
| Integrations | GitHub App, VS Code Extension, Slack, Discord |
| Engineering effort | 5 engineers x 12 weeks = 60 person-weeks |
| Total services | 6 (API, Web, Worker, Redis, Postgres, S3/MinIO) |
| Total API endpoints | 18 REST + 1 WebSocket |
| Total frontend pages | 8 pages, 40+ components |
| Post-MVP expansions | Marketplace, Enterprise SSO, AI analysis |
