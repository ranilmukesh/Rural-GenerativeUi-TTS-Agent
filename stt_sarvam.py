"""Sarvam AI Speech-to-Text service wrapper.

Strategy:
  - Audio <= ``short_audio_threshold_seconds``: synchronous REST API
    (``client.speech_to_text.transcribe``).
  - Audio > threshold: Sarvam batch job API with diarization.

Both paths return a ``TranscriptionResult`` dataclass so the caller
doesn't need to know which path was taken.
"""

from __future__ import annotations

import logging
import os
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

from sarvamai import SarvamAI

from src.core.config import get_settings

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Result schema (lightweight dataclass; not persisted directly)
# ---------------------------------------------------------------------------

@dataclass
class TranscriptionResult:
    """Normalised output from either Sarvam API path."""

    transcript: str
    language_code: Optional[str] = None
    diarized_transcript: Optional[dict[str, Any]] = None
    request_id: Optional[str] = None
    used_batch_api: bool = False


# ---------------------------------------------------------------------------
# Custom exceptions
# ---------------------------------------------------------------------------

class SarvamTranscriptionError(Exception):
    """Raised when Sarvam STT fails in a non-recoverable way."""


class SarvamRateLimitError(SarvamTranscriptionError):
    """Raised on HTTP 429 / 503 so callers can implement back-off."""


# ---------------------------------------------------------------------------
# Service
# ---------------------------------------------------------------------------

class SarvamSTTService:
    """Thin async-friendly wrapper around the SarvamAI SDK.

    The underlying SDK is synchronous; we run it in a thread pool executor
    when called from async contexts to avoid blocking the event loop.
    (FastAPI's ``run_in_executor`` pattern.)
    """

    def __init__(self) -> None:
        settings = get_settings()
        self._client = SarvamAI(api_subscription_key=settings.sarvam_api_key)
        self._threshold = settings.short_audio_threshold_seconds

    # ------------------------------------------------------------------
    # Public interface
    # ------------------------------------------------------------------

    def transcribe_sync(
        self,
        audio_path: Path,
        duration_seconds: Optional[float] = None,
        language_code: Optional[str] = None,
        with_diarization: bool = True,
        num_speakers: int = 2,
    ) -> TranscriptionResult:
        """Synchronously transcribe an audio file.

        Picks REST vs Batch based on estimated duration.
        If duration is unknown, tries REST first; auto-fallbacks to Batch
        when Sarvam returns a "duration > 30s" error.
        Safe to call from a thread-pool executor.
        """
        use_batch = (
            duration_seconds is not None
            and duration_seconds > self._threshold
        )

        if use_batch:
            return self._transcribe_batch(
                audio_path=audio_path,
                language_code=language_code,
                with_diarization=with_diarization,
                num_speakers=num_speakers,
            )

        # Try REST first; auto-fallback to batch on duration error
        try:
            return self._transcribe_rest(
                audio_path=audio_path,
                language_code=language_code,
            )
        except SarvamTranscriptionError as exc:
            exc_str = str(exc).lower()
            if "duration greater than 30" in exc_str or \
               "exceeds the maximum limit of 30" in exc_str or \
               "too long" in exc_str:
                logger.info(
                    "Audio too long for REST API, falling back to Batch API for %s",
                    audio_path.name,
                )
                return self._transcribe_batch(
                    audio_path=audio_path,
                    language_code=language_code,
                    with_diarization=with_diarization,
                    num_speakers=num_speakers,
                )
            raise

    # ------------------------------------------------------------------
    # Private: REST (short audio)
    # ------------------------------------------------------------------

    def _transcribe_rest(
        self,
        audio_path: Path,
        language_code: Optional[str] = None,
    ) -> TranscriptionResult:
        """Use synchronous Sarvam REST API for files <= threshold."""
        logger.info("Using Sarvam REST API for %s", audio_path.name)
        try:
            kwargs: dict[str, Any] = {
                "model": "saaras:v3",
                "mode": "translate",
            }
            if language_code:
                kwargs["language_code"] = language_code

            with audio_path.open("rb") as audio_file:
                response = self._client.speech_to_text.transcribe(
                    file=audio_file,
                    **kwargs,
                )

            return TranscriptionResult(
                transcript=response.transcript or "",
                language_code=getattr(response, "language_code", None),
                request_id=getattr(response, "request_id", None),
                used_batch_api=False,
            )
        except Exception as exc:
            self._handle_sarvam_exception(exc, "REST")

    # ------------------------------------------------------------------
    # Private: Batch (long audio)
    # ------------------------------------------------------------------

    def _transcribe_batch(
        self,
        audio_path: Path,
        language_code: Optional[str] = None,
        with_diarization: bool = True,
        num_speakers: int = 2,
    ) -> TranscriptionResult:
        """Use Sarvam batch job API for files > threshold."""
        logger.info("Using Sarvam Batch API for %s", audio_path.name)
        try:
            kwargs: dict[str, Any] = {
                "model": "saaras:v3",
                "mode": "translate",
                "with_diarization": with_diarization,
                "num_speakers": num_speakers,
            }
            if language_code:
                kwargs["language_code"] = language_code

            job = self._client.speech_to_text_job.create_job(**kwargs)
            job.upload_files(file_paths=[str(audio_path)])
            job.start()
            job.wait_until_complete()

            file_results = job.get_file_results()

            if not file_results.get("successful"):
                failed = file_results.get("failed", [])
                error_msg = failed[0].get("error_message", "Unknown error") if failed else "No output."
                raise SarvamTranscriptionError(
                    f"Batch job failed for {audio_path.name}: {error_msg}"
                )

            # Download to temp dir and read first result
            with tempfile.TemporaryDirectory() as tmpdir:
                job.download_outputs(output_dir=tmpdir)
                result_files = list(Path(tmpdir).glob("*.json"))
                if result_files:
                    import json
                    data = json.loads(result_files[0].read_text(encoding="utf-8"))
                    return TranscriptionResult(
                        transcript=data.get("transcript", ""),
                        language_code=data.get("language_code"),
                        diarized_transcript=data.get("diarized_transcript"),
                        used_batch_api=True,
                    )

            # Fallback: extract from file_results directly
            successful = file_results["successful"][0]
            return TranscriptionResult(
                transcript=successful.get("transcript", ""),
                language_code=successful.get("language_code"),
                diarized_transcript=successful.get("diarized_transcript"),
                used_batch_api=True,
            )
        except SarvamTranscriptionError:
            raise
        except Exception as exc:
            self._handle_sarvam_exception(exc, "Batch")

    # ------------------------------------------------------------------
    # Error handling
    # ------------------------------------------------------------------

    @staticmethod
    def _handle_sarvam_exception(exc: Exception, api_type: str) -> None:
        """Classify and re-raise Sarvam exceptions with context."""
        error_msg = str(exc)
        status_code: int | None = getattr(exc, "status_code", None)

        if status_code in (429, 503):
            raise SarvamRateLimitError(
                f"Sarvam {api_type} API rate-limited (HTTP {status_code}): {error_msg}"
            ) from exc
        if status_code == 403:
            raise SarvamTranscriptionError(
                f"Sarvam API key invalid or expired (HTTP 403): {error_msg}"
            ) from exc
        raise SarvamTranscriptionError(
            f"Sarvam {api_type} API error: {error_msg}"
        ) from exc


# ---------------------------------------------------------------------------
# Module-level singleton (lazy init)
# ---------------------------------------------------------------------------

_sarvam_service: Optional[SarvamSTTService] = None


def get_sarvam_service() -> SarvamSTTService:
    """Return the module-level SarvamSTTService singleton."""
    global _sarvam_service  # noqa: PLW0603
    if _sarvam_service is None:
        _sarvam_service = SarvamSTTService()
    return _sarvam_service
