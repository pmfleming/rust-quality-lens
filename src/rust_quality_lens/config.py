from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any
import tomllib


@dataclass(frozen=True)
class LensConfig:
    project_name: str
    project_root: Path
    source_roots: tuple[str, ...]
    output_dir: Path
    helper_manifest: Path

    @classmethod
    def load(cls, path: Path | None) -> "LensConfig":
        data: dict[str, Any] = {}
        config_path = path.resolve() if path is not None else None
        if config_path is not None and config_path.exists():
            data = tomllib.loads(config_path.read_text(encoding="utf-8"))

        project_root = Path(data.get("project_root") or ".").resolve()
        source_roots = tuple(data.get("source_roots") or ["src"])
        output_dir = Path(data.get("output_dir") or "target/analysis")
        if not output_dir.is_absolute():
            output_dir = project_root / output_dir

        rust = data.get("rust") or {}
        helper_manifest = Path(
            rust.get("helper_manifest")
            or Path(__file__).resolve().parents[2] / "rust_helpers" / "Cargo.toml"
        )
        if not helper_manifest.is_absolute():
            helper_manifest = project_root / helper_manifest

        return cls(
            project_name=str(data.get("project_name") or project_root.name),
            project_root=project_root,
            source_roots=source_roots,
            output_dir=output_dir,
            helper_manifest=helper_manifest,
        )
