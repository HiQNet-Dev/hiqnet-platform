"""Gate acotado a archivos versionados; no sustituye GitHub secret scanning."""

import re
import subprocess
import sys
from pathlib import Path


def main() -> int:
    tracked = subprocess.check_output(["git", "ls-files", "-z"]).split(b"\0")
    forbidden = re.compile(
        r"(^|/)(\.env(?!\.example$)|secrets/|signing/)|"
        r"\.(?:db|sqlite|sqlite3|dump|sql\.gz|backup|pem|key|p12|pfx|jks|keystore)$",
        re.IGNORECASE,
    )
    bad = []
    for entry in tracked:
        if not entry:
            continue
        path = Path(entry.decode("utf-8"))
        if forbidden.search(path.as_posix()):
            bad.append(str(path))
        elif path.is_file() and path.stat().st_size < 2_000_000:
            text = path.read_bytes()
            if re.search(rb"-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----", text):
                bad.append(str(path))
    if bad:
        print("Archivos prohibidos o claves privadas:", *bad, sep="\n", file=sys.stderr)
        return 1
    print("Archivos versionados: sin dumps, .env ni claves privadas PEM")
    return 0


if __name__ == "__main__":
    sys.exit(main())
