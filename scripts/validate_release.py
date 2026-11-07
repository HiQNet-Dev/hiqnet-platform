"""Valida tags Platform antes de construir o publicar; dispatch sólo simula."""

import os
import re
import sys
import tomllib
from pathlib import Path


IDENTIFIER = r"(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)"
SEMVER = re.compile(
    rf"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    rf"(?:-{IDENTIFIER}(?:\.{IDENTIFIER})*)?"
)


def validate(event: str, ref: str, version: str, workspace_version: str) -> str:
    if event == "push":
        prefix = "refs/tags/platform-v"
        if not ref.startswith(prefix):
            raise ValueError("Sólo se admiten tags platform-v; nunca ramas")
        version = ref[len(prefix) :]
    elif event != "workflow_dispatch":
        raise ValueError("Evento no admitido")

    if not SEMVER.fullmatch(version):
        raise ValueError("Versión SemVer inválida o contiene metadata + (incompatible con Docker tags)")
    # El tag puede ser prerelease de la versión del workspace (0.1.0-alpha.1).
    if version.split("-", 1)[0].split("+", 1)[0] != workspace_version:
        raise ValueError("Tag y versión del workspace no coinciden")
    return f"platform-v{version}"


if __name__ == "__main__":
    try:
        workspace = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))
        tag = validate(
            os.environ.get("RELEASE_EVENT", ""),
            os.environ.get("RELEASE_REF", ""),
            os.environ.get("RELEASE_VERSION", ""),
            workspace["workspace"]["package"]["version"],
        )
    except (ValueError, KeyError) as exc:
        print(f"Release rechazado: {exc}", file=sys.stderr)
        sys.exit(1)
    print(f"tag={tag}")
    print(f"prerelease={'true' if '-' in tag.removeprefix('platform-v') else 'false'}")
