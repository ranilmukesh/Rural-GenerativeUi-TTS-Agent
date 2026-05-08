"""
PlacementPredictor+ LLM Chat Module
AI-powered career chat using Agno SDK + Nvidia LLM
Context Injection pattern: student data is injected into the system prompt.
"""

import os
import uuid

# Load .env file written by start_placement_predictor.bat
# override=True ensures .env always wins (fixes Windows subprocess env inheritance issues)
try:
    from dotenv import load_dotenv
    env_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".env")
    load_dotenv(dotenv_path=env_path, override=True)
    print(f"[LLM] Loaded .env from: {env_path}")
except ImportError:
    print("[LLM] python-dotenv not installed, relying on env var directly")

# Debug: check if key is present
_key = os.environ.get("NVIDIA_API_KEY", "")
if _key:
    print(f"[LLM] NVIDIA_API_KEY found: {_key[:12]}...{_key[-4:]}")
else:
    print("[LLM] WARNING: NVIDIA_API_KEY is NOT set!")

from agno.agent import Agent
from agno.models.nvidia import Nvidia
from agno.db.sqlite import SqliteDb
from llm_tools import award_paati_points, check_government_schemes, get_tenali_puzzle, generate_mini_game, safe_web_search

# Ensure tmp directory exists for chat DB
os.makedirs("tmp", exist_ok=True)

# SQLite for multi-turn chat persistence (exactly as shown in Agno docs)
_chat_db = SqliteDb(db_file="tmp/placement_chat.db")

# In-memory registry: session_id -> Agent instance
_sessions: dict = {}


def build_system_context(
    student_data: dict,
    prediction: dict,
    explanation: dict,
    whatif: dict,
) -> str:
    """Build structured context string from student analysis data."""
    student_data = student_data or {}
    prediction = prediction or {}
    explanation = explanation or {}
    whatif = whatif or {}

    # ── Student Profile ──
    student_lines = [
        f"Gender: {student_data.get('Gender', 'N/A')}",
        f"Age: {student_data.get('Age', 'N/A')} years",
        f"Stream: {student_data.get('Stream', 'N/A')}",
        f"Internships: {student_data.get('Internships', 'N/A')}",
        f"CGPA: {student_data.get('CGPA', 'N/A')}",
        f"Hostel: {'Yes' if student_data.get('Hostel') else 'No'}",
        f"History of Backlogs: {'Yes' if student_data.get('HistoryOfBacklogs') else 'No'}",
        f"Skills: {', '.join(student_data.get('skills', []))}",
        f"Desired Role: {student_data.get('desired_role', 'Not Specified')}"
    ]
    student_block = "\n".join(f"  - {l}" for l in student_lines)

    # ── Prediction ──
    pred_block = (
        f"  - Placement Chance: {prediction.get('probability_percentage', '?')}%\n"
        f"  - Risk Level: {prediction.get('risk_level', '?')}\n"
        f"  - Confidence: {prediction.get('confidence', '?')}\n"
        f"  - Recommended Role: {prediction.get('recommended_job', 'N/A')}"
    )

    # ── SHAP Factors ──
    factors = explanation.get("top_contributing_factors", [])
    factor_lines = []
    for f in factors:
        factor_lines.append(
            f"  - {f.get('feature', '?')} | {f.get('direction', '?')} | "
            f"{f.get('interpretation', '')}"
        )
    shap_block = "\n".join(factor_lines) if factor_lines else "  (None available)"

    # ── What-If Scenarios ──
    scenarios = whatif.get("scenarios", [])
    scenario_lines = []
    for s in scenarios:
        scenario_lines.append(
            f"  - {s.get('title', '?')}: "
            f"{s.get('original_risk', '?')}% -> {s.get('modified_risk', '?')}% "
            f"(delta: {s.get('risk_delta', 0):+.1f}%) | {s.get('description', '')}"
        )
    whatif_block = "\n".join(scenario_lines) if scenario_lines else "  (None generated)"

    combined = whatif.get("combined_risk")
    combined_line = ""
    if combined is not None:
        combined_line = (
            f"\n  BEST COMBINED OUTCOME (all changes): "
            f"{combined}% ({whatif.get('combined_risk_level', '?')})"
        )

    # Add this block for the resume
    resume_content = student_data.get('resume_text', '')
    resume_block = f"STUDENT RESUME DATA:\n{resume_content}\n\n" if resume_content else "STUDENT RESUME DATA: None provided.\n\n"

    return (
        "=== PAATI-KURAL LEAGUE ASSESSMENT ===\n\n"
        f"STUDENT PROFILE:\n{student_block}\n\n"
        f"{resume_block}"
        f"PLACEMENT CHANCE PREDICTION:\n{pred_block}\n\n"
        f"TOP CONTRIBUTING FACTORS (SHAP):\n{shap_block}\n\n"
        f"WHAT-IF SCENARIOS:\n{whatif_block}{combined_line}\n\n"
        "======================================"
    )


def start_chat_session(
    student_data: dict,
    prediction: dict,
    explanation: dict,
    whatif: dict,
) -> tuple:
    """
    Start a new chat session with student context injected.
    Returns (session_id, greeting_message_text).
    """
    student_data = student_data or {}
    prediction = prediction or {}
    explanation = explanation or {}
    whatif = whatif or {}
    session_id = f"cs-{uuid.uuid4().hex[:8]}"
    system_context = build_system_context(student_data, prediction, explanation, whatif)

    # Create agent following exact Agno SDK pattern from the docs
    agent = Agent(
        model=Nvidia(
            max_tokens=16384,
            temperature=0.3,  # Increased slightly for better storytelling
            top_p=0.95,
            id="nvidia/nemotron-3-super-120b-a12b"
        ),
        tools=[award_paati_points, check_government_schemes, get_tenali_puzzle, generate_mini_game, safe_web_search],
        description=(
            "You are 'Paati' (Grandma), an endearing, wise elder from a Tamil village who acts as a career coach. "
            "You run the 'Paati-Kural League', a gamified learning platform bridging rural students to real jobs. "
            "The student has just completed their assessment and uploaded their data/resume below.\n\n"
            + system_context
        ),
        instructions=[
            "Speak in Tanglish as Paati.",
            "GOAL 1: If the student answers correctly, call 'award_paati_points' with points and a reason.",
            "GOAL 2: Use 'get_tenali_puzzle' to teach a concept instead of explaining it directly.",
            "GOAL 3: Use 'check_government_schemes' to find real jobs/training for them when they reach 500 points. USE safe_web_search to search the live web for recent application deadlines or links regarding that scheme.",
            "GOAL 4: If the student needs to learn a new concept (like SQL, Compound Interest, etc.), use 'generate_mini_game' to build a visual interactive game. CRITICAL: You MUST include the raw JSON output from the tool exactly as-is in your response, wrapped in a ```json markdown block, otherwise the game UI will not render. Keep your own intro very brief.",
            "If the student asks for an 'analysis' of their game performance or career progress, provide a detailed, encouraging 'Paati Analysis' using their specific Resume data, SHAP factors, and recent game success.",
            "Keep track of their 'Level': Seed -> Sapling -> Tree.",
            "CRITICAL: Always analyze and acknowledge their specific Resume content (experience, projects, education) in your VERY FIRST message. Don't wait for them to ask.",
            "Reference their specific Resume data, skills, and SHAP factors directly.",
            "Instead of sterile corporate terms, talk about 'earning badges', 'village leaderboards', and 'micro-job opportunities'.",
            "If their placement chance is low, give them a 'Paati challenge' (e.g., learn a specific skill to earn 100 points).",
            "Always respond in text ONLY (No TTS). Keep your responses to 2-3 short, highly engaging paragraphs.",
            "Never fabricate data — only reference the assessment and resume provided.",
            "Explain career terms and gaps mapped in plain language.",
            "When discussing What-If scenarios, give practical academic or internship tips.",
            "If placement chances are LOW, be reassuring but give robust constructive criticism.",
            "Always remind them you are an AI, not an official hiring manager.",
            "If the student asks to compare their target job with the recommended job, focus on skill matching between the two."
        ],
        expected_output=(
            "Warm, Tanglish-flavored career advice rooted in Tamil folklore/Thirukkural, ending with a specific gamified challenge based on their data."
        ),
        db=_chat_db,
        add_history_to_context=True,
        num_history_runs=10,
        add_datetime_to_context=True,
        tool_call_limit=5,
    )

    _sessions[session_id] = agent

    # Auto-generate greeting
    greeting_prompt = (
        "Introduce yourself as Paati. IMMEDIATELY analyze their placement score AND summarize 2-3 key findings from their uploaded resume. "
        "Relate their resume experience to their target job. "
        "Give them a welcoming Thirukkural or proverb, and ask them if they are ready for their first Paati-Kural Challenge."
    )
    response = agent.run(greeting_prompt, session_id=session_id)
    
    # Error checking
    content = response.content or ""
    if not isinstance(content, str):
        content = str(content)
    if any(err in content for err in ("Connection error", "404", "Unknown model error")) \
       or str(getattr(response, "status", "")).lower() == "error":
        raise ConnectionError(f"LLM API Error: {content}")
        
    return session_id, content


def get_chat_response(session_id: str, user_message: str) -> str:
    """Get a response in an existing session. Returns response text."""
    agent = _sessions.get(session_id)
    if agent is None:
        raise ValueError(f"Session '{session_id}' not found or expired.")

    response = agent.run(user_message, session_id=session_id)
    
    # Error checking
    content = response.content or ""
    if not isinstance(content, str):
        content = str(content)
    if any(err in content for err in ("Connection error", "404", "Unknown model error")) \
       or str(getattr(response, "status", "")).lower() == "error":
        raise ConnectionError(f"LLM API Error: {content}")
        
    return content