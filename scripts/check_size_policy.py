"""Verifica la política de tamaño de archivos y evita nuevos monolíticos.

Uso: python scripts/check_size_policy.py [--strict]

Reglas:
- Más de 1000 líneas: falla salvo excepción registrada en la línea base.
- Entre 800 y 1000: advertencia visible (revisión arquitectónica obligatoria).
- Más de 500: advertencia informativa.
- Si un archivo de la línea base crece, falla: la deuda estructural no aumenta.

La línea base sólo se reduce. Para bajarla, refactoriza y actualiza el JSON en el
mismo PR; para subirla hace falta una justificación escrita en el propio PR.
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASELINE_PATH = ROOT / "scripts" / "maintenance_baseline.json"

HARD_LIMIT = 1000
REVIEW_LIMIT = 800
WARN_LIMIT = 500

EXTENSIONS = {".rs", ".ts", ".tsx"}
EXCLUDED_PARTS = {
    "node_modules",
    "target",
    "generated",
    "migrations",
    "openapi",
}
EXCLUDED_SUFFIXES = (".d.ts", "lock.yaml", "lock.json")


def tracked_files() -> list[str]:
    output = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout
    return [line for line in output.splitlines() if line.strip()]


def is_measured(path: str) -> bool:
    pure = Path(path)
    if pure.suffix not in EXTENSIONS:
        return False
    if any(part in EXCLUDED_PARTS for part in pure.parts):
        return False
    if path.endswith(EXCLUDED_SUFFIXES):
        return False
    return True


def line_count(path: Path) -> int:
    with path.open(encoding="utf-8", errors="replace") as handle:
        return sum(1 for _ in handle)


def load_baseline() -> dict[str, dict]:
    if not BASELINE_PATH.exists():
        return {}
    return json.loads(BASELINE_PATH.read_text(encoding="utf-8")).get("files", {})


def evaluate(sizes: dict[str, int], baseline: dict[str, dict]) -> tuple[list[str], list[str], list[str]]:
    """Devuelve (fallos, advertencias de revisión, avisos informativos)."""
    failures: list[str] = []
    reviews: list[str] = []
    warnings: list[str] = []

    for path, lines in sorted(sizes.items()):
        entry = baseline.get(path)
        allowed = entry["lines"] if entry else 0

        if entry and lines > allowed:
            failures.append(
                f"{path}: {lines} líneas supera la línea base ({allowed}). "
                "Reduce el archivo o justifica el aumento en el PR."
            )
            continue
        if lines > HARD_LIMIT and not entry:
            failures.append(
                f"{path}: {lines} líneas supera el límite duro ({HARD_LIMIT}) sin excepción registrada."
            )
        elif lines > REVIEW_LIMIT:
            reviews.append(f"{path}: {lines} líneas (revisión arquitectónica)")
        elif lines > WARN_LIMIT:
            warnings.append(f"{path}: {lines} líneas")

    return failures, reviews, warnings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--strict", action="store_true", help="trata las advertencias como fallos")
    args = parser.parse_args()

    sizes = {
        path: line_count(ROOT / path)
        for path in tracked_files()
        if is_measured(path) and (ROOT / path).exists()
    }
    failures, reviews, warnings = evaluate(sizes, load_baseline())

    print(f"Archivos medidos: {len(sizes)}")
    for failure in failures:
        print(f"ERROR {failure}")
    for review in reviews:
        print(f"REVISAR {review}")
    for warning in warnings:
        print(f"AVISO {warning}")

    if args.strict:
        failures.extend(reviews)

    if failures:
        return 1
    print("Política de tamaño: sin violaciones.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
