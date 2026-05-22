import json
import os
from pathlib import Path
from typing import Callable, Optional

REPORT_MODES = ("cli", "analysis", "visibility")


def add_mode_argument(parser, *, default: str = "analysis") -> None:
    parser.add_argument(
        "--mode",
        choices=REPORT_MODES,
        default=default,
        help="`cli` prints a readable summary, `analysis` emits JSON, `visibility` writes viewer-ready JSON and prints where it went.",
    )


def emit_report(
    payload: object,
    *,
    mode: str,
    output_path: Optional[Path],
    visibility_path: Path,
    cli_renderer: Callable[[object], str],
    label: str,
) -> None:
    json_text = json.dumps(payload, indent=2)
    resolved_output = resolve_output_path(mode, output_path, visibility_path)

    if resolved_output is not None:
        resolved_output.parent.mkdir(parents=True, exist_ok=True)
        resolved_output.write_text(json_text + "\n", encoding="utf-8")

    if mode == "analysis":
        if resolved_output is None:
            print(json_text)
        return

    if mode == "visibility":
        print(f"Wrote {label} visibility data to {resolved_output}")

    print(cli_renderer(payload))


def resolve_output_path(mode: str, output_path: Optional[Path], visibility_path: Path) -> Optional[Path]:
    resolved_output = visibility_path if mode == "visibility" and output_path is None else output_path
    if resolved_output is not None and not resolved_output.is_absolute():
        output_dir = os.environ.get("RQLENS_OUTPUT_DIR")
        parts = resolved_output.parts
        if output_dir and len(parts) >= 2 and parts[0] == "target" and parts[1] == "analysis":
            return Path(output_dir).joinpath(*parts[2:])
    return resolved_output


def resolve_analysis_path(path: str | Path) -> Optional[Path]:
    output_dir = os.environ.get("RQLENS_OUTPUT_DIR")
    if not output_dir:
        return None

    resolved = Path(path)
    if resolved.is_absolute():
        return resolved

    parts = resolved.parts
    if len(parts) >= 2 and parts[0] == "target" and parts[1] == "analysis":
        return Path(output_dir).joinpath(*parts[2:])
    return Path(output_dir) / resolved.name


def analysis_path(path: str | Path) -> Path:
    return resolve_analysis_path(path) or Path(path)
