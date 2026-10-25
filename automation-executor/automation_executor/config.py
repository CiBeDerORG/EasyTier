from dataclasses import dataclass
import os
from pathlib import Path
from urllib.parse import urlsplit


@dataclass(frozen=True)
class Settings:
    token: str
    callback_url: str
    data_dir: Path
    adb_binary: str
    max_running: int
    max_pending: int
    max_artifact_bytes: int

    @classmethod
    def from_env(cls):
        token = os.environ.get("AUTOMATION_TOKEN", "")
        if not 32 <= len(token.encode()) <= 4096 or any(ord(char) < 32 or ord(char) == 127 for char in token):
            raise RuntimeError("AUTOMATION_TOKEN must contain 32..4096 bytes without control characters")
        callback = os.environ.get("AUTOMATION_CALLBACK_URL", "").rstrip("/")
        if callback:
            parsed = urlsplit(callback)
            if (parsed.scheme not in ("http", "https") or not parsed.hostname
                    or parsed.username or parsed.password or parsed.query or parsed.fragment):
                raise RuntimeError("AUTOMATION_CALLBACK_URL must be a trusted HTTP(S) base URL")
        return cls(
            token, callback, Path(os.environ.get("AUTOMATION_DATA_DIR", "/var/lib/easytier-automation")),
            os.environ.get("AUTOMATION_ADB_BINARY", "adb"),
            max(1, int(os.environ.get("AUTOMATION_MAX_RUNNING", "3"))),
            max(1, int(os.environ.get("AUTOMATION_MAX_PENDING", "30"))),
            min(1024 * 1024 * 1024, max(1024 * 1024, int(os.environ.get("AUTOMATION_ARTIFACT_BYTES", "104857600")))),
        )
