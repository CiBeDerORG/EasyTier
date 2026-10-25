"""Private, authenticated execution API. Run exactly one uvicorn worker."""
from contextlib import asynccontextmanager
import fcntl

from fastapi import Depends, FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import FileResponse, JSONResponse

from .config import Settings
from .errors import ExecutorError
from .runtime import Runtime
from .validation import identifier
from .auth import authenticated


@asynccontextmanager
async def lifespan(app):
    settings = Settings.from_env()
    settings.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    lock = (settings.data_dir / "executor.lock").open("a+")
    try:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        lock.close()
        raise RuntimeError("The executor data directory already has an active worker; use --workers 1") from None
    runtime = None
    try:
        runtime = Runtime(settings)
        app.state.runtime = runtime
        app.state.settings = settings
        yield
    finally:
        if runtime:
            await runtime.shutdown()
        fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
        lock.close()


async def authenticate(request: Request):
    supplied = request.headers.get("authorization", "")
    if not authenticated(supplied, request.app.state.settings.token):
        raise ExecutorError("authentication_required", "执行器认证失败", 401)


app = FastAPI(title="EasyTier Automation Executor", version="1.0", lifespan=lifespan,
              docs_url=None, redoc_url=None, openapi_url=None,
              dependencies=[Depends(authenticate)])


@app.middleware("http")
async def bounded_request(request: Request, call_next):
    # Bound both content-length and chunked bodies before JSON parsing.
    if request.method in ("POST", "PUT", "PATCH"):
        supplied = request.headers.get("authorization", "")
        if not authenticated(supplied, request.app.state.settings.token):
            return JSONResponse({"code": "authentication_required", "message": "执行器认证失败"}, status_code=401)
        chunks, size = [], 0
        async for chunk in request.stream():
            size += len(chunk)
            if size > 512 * 1024:
                return JSONResponse({"code": "request_too_large", "message": "请求超过大小限制"}, status_code=413)
            chunks.append(chunk)
        request._body = b"".join(chunks)
    return await call_next(request)


@app.exception_handler(RequestValidationError)
async def validation_error(_request, _exc):
    # Pydantic errors normally contain the original input, including pairing codes.
    return JSONResponse({"code": "invalid_request", "message": "请求格式无效"}, status_code=422)


@app.exception_handler(ExecutorError)
async def executor_error(_request, exc):
    return JSONResponse({"code": exc.code, "message": exc.message}, status_code=exc.status)


@app.exception_handler(Exception)
async def unexpected_error(_request, exc):
    # MediaError is deliberately independent of the HTTP layer.
    if exc.__class__.__name__ == "MediaError" and all(hasattr(exc, k) for k in ("code", "message", "status")):
        return JSONResponse({"code": exc.code, "message": exc.message}, status_code=exc.status)
    return JSONResponse({"code": "executor_error", "message": "执行器未能完成操作，结果需核实"}, status_code=500)


@app.get("/internal/v1/health")
async def health(request: Request):
    return request.app.state.runtime.health()


@app.post("/internal/v1/prepare")
async def prepare(request: Request, body: dict):
    return await request.app.state.runtime.prepare(body)


@app.post("/internal/v1/preparations")
async def start_preparation(request: Request, body: dict):
    return await request.app.state.runtime.start_preparation(body)


@app.get("/internal/v1/preparations/device/{did}")
async def preparation_history(request: Request, did: str, owner: int):
    return request.app.state.runtime.preparation_history(identifier(did), owner)


@app.get("/internal/v1/preparations/{pid}")
async def preparation_detail(request: Request, pid: str):
    return request.app.state.runtime.preparation(identifier(pid))


@app.get("/internal/v1/preparations/{pid}/logs")
async def preparation_logs(request: Request, pid: str, after_seq: int = 0):
    return request.app.state.runtime.preparation_logs(identifier(pid), max(0, after_seq))


@app.post("/internal/v1/preparations/{pid}/cancel")
async def preparation_cancel(request: Request, pid: str):
    return await request.app.state.runtime.cancel_preparation(identifier(pid))


@app.post("/internal/v1/adb/status")
async def adb_status(request: Request, body: dict):
    return await request.app.state.runtime.adb_status(body)


@app.post("/internal/v1/automation/status")
async def automation_status(request: Request, body: dict):
    return await request.app.state.runtime.automation_status(body)


@app.post("/internal/v1/sessions")
async def create_session(request: Request, body: dict):
    return await request.app.state.runtime.create_session(body)


@app.post("/internal/v1/sessions/{sid}/offer")
async def offer(request: Request, sid: str, body: dict):
    return await request.app.state.runtime.offer(identifier(sid), body)


@app.post("/internal/v1/sessions/{sid}/commands")
async def command(request: Request, sid: str, body: dict):
    return await request.app.state.runtime.command(identifier(sid), body)


@app.post("/internal/v1/sessions/{sid}/renew")
async def renew(request: Request, sid: str, body: dict):
    return await request.app.state.runtime.renew(identifier(sid), body)


@app.post("/internal/v1/sessions/{sid}/close")
async def close_session(request: Request, sid: str, body: dict):
    return await request.app.state.runtime.close_session_request(identifier(sid), body)


@app.post("/internal/v1/runs")
async def create_run(request: Request, body: dict):
    return await request.app.state.runtime.create_run(body)


@app.get("/internal/v1/runs/{rid}")
async def get_run(request: Request, rid: str):
    return request.app.state.runtime.journal.run(identifier(rid))


@app.post("/internal/v1/runs/{rid}/cancel")
async def cancel_run(request: Request, rid: str, body: dict):
    return await request.app.state.runtime.cancel_run(identifier(rid), body)


@app.get("/internal/v1/runs/{rid}/artifacts/{aid}")
async def get_artifact(request: Request, rid: str, aid: str):
    path = request.app.state.runtime.artifact(rid, aid)
    with path.open("rb") as handle:
        prefix = handle.read(8)
    media_type = "image/png" if prefix == b"\x89PNG\r\n\x1a\n" else "image/jpeg"
    return FileResponse(path, media_type=media_type, headers={"Cache-Control": "no-store", "X-Content-Type-Options": "nosniff"})


@app.post("/internal/v1/runs/{rid}/devices/{did}/withdraw")
async def withdraw_device(request: Request, rid: str, did: str, body: dict):
    return await request.app.state.runtime.withdraw_device(identifier(rid),identifier(did),body)


@app.post("/internal/v1/targets/{did}/verify-unused")
async def verify_unused_target(request: Request, did: str, body: dict):
    return await request.app.state.runtime.verify_unused_target(identifier(did), body)


@app.post("/internal/v1/targets/{did}/retire")
async def retire_target(request: Request, did: str, body: dict):
    return await request.app.state.runtime.retire_target(identifier(did),body)
