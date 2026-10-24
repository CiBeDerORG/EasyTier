"""Reuse the isolated fixture on the current schema; this is NOT an upgrade test.

Usage: python prepare-current-fixture.py REPO DIRECTORY CURRENT_BINARY
The upstream helper owns all seeding and safety checks. Only the requirement
for a pre-binding binary is inverted for this current-schema API audit.
"""
from pathlib import Path
import subprocess
import sys

repo, directory, binary = map(Path, sys.argv[1:])
source = repo / "easytier-web/frontend/tests/node-instance-binding-fixture.py"
old = 'assert connection.execute("SELECT COUNT(*) FROM seaql_migrations WHERE version=\'m20261023_000013_asset_instance_bindings\'").fetchone()[0] == 0'
new = old[:-1] + '1'
text = source.read_text()
assert text.count(old) == 1
directory.mkdir(mode=0o700, parents=True, exist_ok=True)
helper = directory / 'current-schema-fixture.py'
helper.write_text(text.replace(old, new))
subprocess.run([sys.executable, str(helper), 'prepare', '--directory', str(directory),
                '--old-binary', str(binary)], check=True)
print('Fixture created on current schema; no old-binary migration claim.')
