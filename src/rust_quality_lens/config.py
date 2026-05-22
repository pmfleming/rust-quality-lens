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
        config_dir = config_path.parent if config_path is not None else Path.cwd()
        if config_path is not None and config_path.exists():
            data = tomllib.loads(config_path.read_text(encoding="utf-8"))

        project_root = cls._resolve_config_path(
            data.get("project_root") or ".",
            config_dir,
        )
        source_roots = tuple(data.get("source_roots") or ["src"])
        output_dir = cls._resolve_project_path(
            data.get("output_dir") or "target/analysis",
            project_root,
        )

        rust = data.get("rust") or {}
        helper_manifest = cls._resolve_project_path(
            rust.get("helper_manifest")
            or Path(__file__).resolve().parents[2] / "rust_helpers" / "Cargo.toml",
            project_root,
        )

        return cls(
            project_name=str(data.get("project_name") or project_root.name),
            project_root=project_root,
            source_roots=source_roots,
            output_dir=output_dir,
            helper_manifest=helper_manifest,
        )

    @staticmethod
    def _resolve_config_path(path: str | Path, config_dir: Path) -> Path:
        resolved = Path(path)
        return resolved if resolved.is_absolute() else (config_dir / resolved).resolve()

    @staticmethod
    def _resolve_project_path(path: str | Path, project_root: Path) -> Path:
        resolved = Path(path)
        return resolved if resolved.is_absolute() else (project_root / resolved).resolve()
