import logging
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional
import os
from sarvamai import SarvamAI

logger = logging.getLogger(__name__)

@dataclass
class TranscriptionResult:
    transcript: str
    language_code: Optional[str] = None
    diarized_transcript: Optional[dict[str, Any]] = None
    request_id: Optional[str] = None
    used_batch_api: bool = False

class SarvamSTTService:
    def __init__(self) -> None:
        api_key = os.environ.get("SARVAM_API_KEY", "")
        self._client = SarvamAI(api_subscription_key=api_key)
        self._threshold = 30.0

    def transcribe_sync(
        self,
        audio_path: Path,
        language_code: Optional[str] = "ta-IN",
    ) -> TranscriptionResult:
        try:
            kwargs = {"model": "saaras:v3"}
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
            logger.error(f"Sarvam API Error: {exc}")
            return TranscriptionResult(transcript="")

_sarvam_service = None

def get_sarvam_service() -> SarvamSTTService:
    global _sarvam_service
    if _sarvam_service is None:
        _sarvam_service = SarvamSTTService()
    return _sarvam_service
