"""Explicit offline artifact removal; never scheduled and never opens ADB."""
import argparse
import fcntl
import os
from pathlib import Path
import sqlite3
from uuid import UUID


def main():
    parser = argparse.ArgumentParser(description="Remove screenshot evidence for exactly one run while the executor is stopped")
    choice = parser.add_mutually_exclusive_group(required=True)
    choice.add_argument("--delete-artifacts-for-run", type=UUID)
    choice.add_argument("--delete-unregistered-artifacts", action="store_true",
                        help="Remove UUID-named files that were never registered after an interrupted screenshot write")
    args = parser.parse_args()
    root = Path(os.environ.get("AUTOMATION_DATA_DIR", "/var/lib/easytier-automation")).resolve()
    database = root / "execution.sqlite3"
    if not database.is_file():
        parser.error("Existing execution.sqlite3 not found; no data was created")
    with (root / "executor.lock").open("a+") as lock:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("Stop the executor before removing screenshot evidence")
        with sqlite3.connect(str(database), isolation_level=None) as db:
            if args.delete_unregistered_artifacts:
                registered = {row[0] for row in db.execute("SELECT id FROM artifacts")}
                rows = []
                for path in (root / "artifacts").iterdir():
                    try:
                        aid = str(UUID(path.name))
                    except ValueError:
                        continue
                    if aid not in registered and path.is_file() and not path.is_symlink():
                        rows.append((aid, str(path), path.stat().st_size))
            else:
                rows = db.execute("SELECT id,path,size FROM artifacts WHERE run_id=?",
                                  (str(args.delete_artifacts_for_run),)).fetchall()
            if any(Path(row[1]).resolve().parent != root / "artifacts" for row in rows):
                parser.error("Artifact path is outside the configured artifact directory; no files were removed")
            deleted = 0
            for aid, filename, size in rows:
                # A crash after unlink leaves a retryable missing file, never a live file
                # excluded from quota. Run outcomes remain as historical evidence.
                Path(filename).unlink(missing_ok=True)
                db.execute("DELETE FROM artifacts WHERE id=?", (aid,))
                deleted += size
            print(f"Removed {len(rows)} screenshot records ({deleted} bytes); run history is retained.")


if __name__ == "__main__":
    main()
