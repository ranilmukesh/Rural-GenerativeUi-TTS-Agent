import os
import re
from sarvamai import SarvamAI

_client = None

def get_tts_base64(text: str, language_code: str = "ta-IN", speaker: str = "priya") -> str:
    """Converts cleaned text to speech using Sarvam Bulbul v3 REST API."""
    global _client
    if _client is None:
        api_key = os.environ.get("SARVAM_API_KEY", "")
        if not api_key:
            return ""
        _client = SarvamAI(api_subscription_key=api_key)
    
    # 1. Strip JSON code blocks entirely (we don't want Paati reading JSON)
    clean_text = re.sub(r'```json.*?```', '', text, flags=re.DOTALL)
    clean_text = re.sub(r'\n```.*?```', '', clean_text, flags=re.DOTALL)
    clean_text = re.sub(r'\{.*?"steps".*?\}', '', clean_text, flags=re.DOTALL)
    
    # 2. Strip standard markdown symbols 
    clean_text = clean_text.replace('**', '').replace('*', '').replace('#', '')
    clean_text = clean_text.strip()
    
    if not clean_text:
        return ""
        
    # Bulbul v3 character limit is 2500
    clean_text = clean_text[:2499]

    try:
        response = _client.text_to_speech.convert(
            text=clean_text,
            target_language_code=language_code,
            model="bulbul:v3",
            speaker=speaker
        )
        if response.audios and len(response.audios) > 0:
            return response.audios[0]
        return ""
    except Exception as e:
        print(f"[TTS Error] {e}")
        return ""
