import pandas as pd
import numpy as np
import joblib
import shap
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from fastapi.responses import FileResponse
from pydantic import BaseModel, Field
from typing import Optional, List
import os
import logging
import sys
from datetime import datetime
from pytz import timezone
from starlette.middleware.base import BaseHTTPMiddleware
from fastapi import UploadFile, File, Form
import PyPDF2
from pathlib import Path
import tempfile
from sarvam_stt import get_sarvam_service
import re
from sarvam_tts import get_tts_base64


import sqlite3 as _sqlite3

# ---------------------------------------------------------------------------
# Progress store — SQLite-backed, multi-worker safe.
# Uses stdlib sqlite3 (no extra deps). WAL mode allows concurrent readers.
# Same DB file as Agno's SqliteDb so everything stays in one place.
# ---------------------------------------------------------------------------
_PROGRESS_DB = "tmp/placement_chat.db"
_DEFAULT_PROGRESS = {"points": 0, "level": "Seed (Vithu)", "kurals": 0}

def _progress_conn():
    """Open a short-lived connection to the progress DB with WAL mode."""
    conn = _sqlite3.connect(_PROGRESS_DB, check_same_thread=False)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("""
        CREATE TABLE IF NOT EXISTS paati_progress (
            session_id TEXT PRIMARY KEY,
            points     INTEGER DEFAULT 0,
            level      TEXT    DEFAULT 'Seed (Vithu)',
            kurals     INTEGER DEFAULT 0
        )
    """)
    conn.commit()
    return conn

def get_progress(session_id: str) -> dict:
    """Read progress for a session. Returns default dict if not found."""
    with _progress_conn() as conn:
        row = conn.execute(
            "SELECT points, level, kurals FROM paati_progress WHERE session_id = ?",
            (session_id,)
        ).fetchone()
    if row:
        return {"points": row[0], "level": row[1], "kurals": row[2]}
    return dict(_DEFAULT_PROGRESS)

def save_progress(session_id: str, prog: dict) -> None:
    """Upsert progress for a session."""
    with _progress_conn() as conn:
        conn.execute(
            "INSERT INTO paati_progress (session_id, points, level, kurals) "
            "VALUES (?, ?, ?, ?) "
            "ON CONFLICT(session_id) DO UPDATE SET "
            "points=excluded.points, level=excluded.level, kurals=excluded.kurals",
            (session_id, prog["points"], prog["level"], prog["kurals"])
        )


os.environ['TZ'] = 'Asia/Kolkata'

# Setup IST timezone logging
IST = timezone('Asia/Kolkata')

# Configure logging with IST timezone
class ISTFormatter(logging.Formatter):
    """Custom formatter for IST timestamps"""
    def formatTime(self, record, datefmt=None):
        dt = datetime.fromtimestamp(record.created, tz=IST)
        if datefmt:
            return dt.strftime(datefmt)
        else:
            return dt.strftime("%Y-%m-%d %H:%M:%S IST")

def setup_logging():
    """Setup logging for all loggers to use IST timestamps"""
    formatter = ISTFormatter('[%(asctime)s] %(levelname)s - %(name)s - %(message)s')
    
    # Configure root logger
    root_logger = logging.getLogger()
    root_logger.setLevel(logging.INFO)
    
    # Remove existing handlers
    for handler in root_logger.handlers[:]:
        root_logger.removeHandler(handler)
    
    # Add new handlers with custom formatter
    console_handler = logging.StreamHandler(sys.stdout)
    console_handler.setFormatter(formatter)
    root_logger.addHandler(console_handler)
    
    # Configure uvicorn loggers
    for logger_name in ['uvicorn', 'uvicorn.error', 'fastapi']:
        lg = logging.getLogger(logger_name)
        lg.setLevel(logging.INFO)
        # Clear existing handlers
        for handler in lg.handlers[:]:
            lg.removeHandler(handler)
        # Use root logger's handlers
        lg.handlers = root_logger.handlers
        lg.propagate = True
    
    # Disable uvicorn access logging (we use our own middleware)
    access_logger = logging.getLogger('uvicorn.access')
    access_logger.setLevel(logging.CRITICAL)  # Suppress access logs
    access_logger.disabled = True
    
    return logging.getLogger(__name__)

logger = setup_logging()

def get_client_ip(request: Request) -> str:
    """Extract client IP from request headers or socket"""
    if 'x-forwarded-for' in request.headers:
        return request.headers['x-forwarded-for'].split(',')[0].strip()
    elif 'x-real-ip' in request.headers:
        return request.headers['x-real-ip']
    else:
        return request.client.host if request.client else "0.0.0.0"

class LoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        client_ip = get_client_ip(request)
        method = request.method
        path = request.url.path
        logger.info(f"📨 [IP: {client_ip}] {method} {path}")
        
        response = await call_next(request)
        
        logger.info(f"✅ [IP: {client_ip}] {method} {path} → {response.status_code}")
        return response

try:
    from llm import start_chat_session, get_chat_response, paati_agent
    CHAT_AVAILABLE = True
    logger.info("[OK] LLM Chat module loaded")
except Exception as _llm_err:
    CHAT_AVAILABLE = False
    paati_agent = None
    logger.warning(f"[!] LLM Chat unavailable: {_llm_err}")

app = FastAPI(
    title="PlacementPredictor+",
    description="AI-powered placement prediction with explainable insights and career routing",
    version="1.0.0"
)

# Add logging middleware first
app.add_middleware(LoggingMiddleware)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

ARTIFACTS_PATH = "placement_artifacts.pkl"

from routing_engine import RoutingEngine

model = None
le_gender = None
le_stream = None
explainer = None
routing_engine = None
shap_model = None
preprocessor = None

def load_artifacts():
    global model, le_gender, le_stream, explainer, routing_engine, shap_model, preprocessor
    if not os.path.exists(ARTIFACTS_PATH):
        raise FileNotFoundError(f"Artifacts file '{ARTIFACTS_PATH}' not found.")
    
    artifacts = joblib.load(ARTIFACTS_PATH)
    model = artifacts['model']
    shap_model = artifacts['shap_model']
    preprocessor = artifacts['preprocessor']
    le_gender = artifacts['le_gender']
    le_stream = artifacts['le_stream']
    routing_engine = artifacts.get('routing_engine')
    explainer = shap.TreeExplainer(shap_model)
    logger.info("[OK] All artifacts loaded successfully!")

@app.on_event("startup")
async def startup_event():
    logger.info("=" * 60)
    logger.info("Application Startup")
    logger.info("=" * 60)
    try:
        load_artifacts()
    except Exception as e:
        logger.error(f"[!] Startup Error: {e}")

class StudentData(BaseModel):
    Age: int = Field(..., ge=15, le=40)
    Gender: str = Field(..., description="Male or Female")
    Stream: str = Field(..., description="Stream of study")
    Internships: int = Field(..., ge=0)
    CGPA: float = Field(..., ge=0, le=10)
    Hostel: int = Field(..., description="1 if hostel, 0 if not")
    HistoryOfBacklogs: int = Field(..., description="1 if backlogs, 0 if not")
    skills: List[str] = Field(default=[], description="List of user skills for routing")
    desired_role: Optional[str] = Field(default=None, description="User's desired job role")
    resume_text: Optional[str] = Field(default="", description="Injected resume content")

class PredictionResponse(BaseModel):
    prediction: int
    probability_percentage: float
    risk_level: str
    confidence: str
    recommended_job: Optional[str] = None
    missing_skills: Optional[List[str]] = None
    graph_data: Optional[str] = None
    bridges: Optional[List[dict]] = None

class FactorImpact(BaseModel):
    feature: str
    impact: float
    direction: str
    interpretation: str

class ExplainResponse(BaseModel):
    top_contributing_factors: List[FactorImpact]
    base_value: float
    prediction_value: float

class HealthResponse(BaseModel):
    status: str
    model_loaded: bool

class OptionsResponse(BaseModel):
    streams: List[str]
    skills: List[str]
    jobs: List[str]

def prepare_input(data: StudentData) -> pd.DataFrame:
    try:
        g = le_gender.transform([data.Gender])[0]
        s = le_stream.transform([data.Stream])[0]
    except Exception as e:
        # Fallback for unknown
        g = 0
        s = 0
    df = pd.DataFrame([{
        'Age': data.Age,
        'Gender': g,
        'Stream': s,
        'Internships': data.Internships,
        'CGPA': data.CGPA,
        'Hostel': data.Hostel,
        'HistoryOfBacklogs': data.HistoryOfBacklogs
    }])
    # Extract columns naturally based on dict order which matches train features order
    return df

@app.get("/api/health", response_model=HealthResponse)
async def health_check():
    return HealthResponse(status="healthy", model_loaded=model is not None)

@app.get("/options", response_model=OptionsResponse)
async def get_options():
    if le_stream is None or routing_engine is None:
        raise HTTPException(status_code=503, detail="Artifacts not loaded.")
    return OptionsResponse(
        streams=list(le_stream.classes_),
        skills=routing_engine.get_skill_list(),
        jobs=routing_engine.get_job_list()
    )

def get_placement_level(probability: float):
    # Map probability of placement to levels
    # Since we want to reuse the JS/CSS, we map:
    # High chance -> good (represented as "LOW" risk so it shows green in UI)
    # Medium chance -> medium
    # Low chance -> bad (represented as "HIGH" risk so it shows red in UI)
    if probability >= 0.7:
        return "LOW", "Very High Confidence"  # LOW risk of being unplaced -> High chance
    elif probability >= 0.5:
        return "LOW", "High Confidence"
    elif probability >= 0.3:
        return "MEDIUM", "Moderate Confidence"
    else:
        return "HIGH", "High Confidence" # HIGH risk of being unplaced -> Low chance

@app.post("/predict", response_model=PredictionResponse)
async def predict_placement(data: StudentData):
    if model is None or preprocessor is None:
        raise HTTPException(status_code=503, detail="Model not loaded.")
    
    df = prepare_input(data)
    df_processed = preprocessor.transform(df)
    prediction = int(model.predict(df_processed)[0])
    probability = float(model.predict_proba(df_processed)[0][1])
    risk_level, confidence = get_placement_level(probability)
    
    rec_job = None
    miss_skills = []
    graph_base64 = None
    
    if routing_engine and data.skills:
        # Get machine recommendation
        job, ms = routing_engine.recommend(data.skills)
        if job:
            rec_job = job
            miss_skills = ms
            
        # Graph logic: prioritize desired role if it exists, but show both in gap analysis if possible.
        # Since the UI only has one img slot, we can generate a combined view or stick to desired.
        # The user specifically mentioned suggested vs desired job graph.
        target_graph = data.desired_role if data.desired_role else rec_job
        if target_graph:
            graph_base64 = routing_engine.get_subgraph_figure_base64(target_graph, data.skills)
            
    # NEW: Automated Government Scheme Search
    # If the student is in a rural area or has lower CGPA, prioritize welfare schemes
    bridges = []
    if data.CGPA < 7.0:
        bridges.append({"title": "Skill Training (Free)", "desc": "Eligible for TN Government TNSDC training."})
    else:
        bridges.append({"title": "Direct Interview", "desc": "Paati is matching you with local micro-jobs."})
            
    return PredictionResponse(
        prediction=prediction,
        probability_percentage=round(probability * 100, 2),
        risk_level=risk_level,
        confidence=confidence,
        recommended_job=rec_job,
        missing_skills=miss_skills,
        graph_data=graph_base64,
        bridges=bridges
    )

def interpret_feature(feature_name: str, impact: float) -> str:
    clean_name = feature_name.replace('_', ' ').title()
    abs_impact = abs(impact)
    intensity = "significantly" if abs_impact > 0.3 else ("moderately" if abs_impact > 0.1 else "slightly")
    
    if impact > 0:
        return f"{clean_name} {intensity} improves placement chances"
    else:
        return f"{clean_name} {intensity} reduces placement chances"

@app.post("/explain", response_model=ExplainResponse)
async def explain_pred(data: StudentData):
    if model is None or explainer is None or preprocessor is None:
        raise HTTPException(status_code=503, detail="Model not loaded.")
    
    df = prepare_input(data)
    df_processed = preprocessor.transform(df)
    shap_values = explainer.shap_values(df_processed)
    
    if isinstance(shap_values, list):
        vals = shap_values[1][0] if len(shap_values) > 1 else shap_values[0][0]
    else:
        vals = shap_values[0]
        if len(vals.shape) == 2:
            vals = vals[0]
            
    if hasattr(explainer.expected_value, '__iter__'):
        base_value = float(explainer.expected_value[1]) if len(explainer.expected_value) > 1 else float(explainer.expected_value[0])
    else:
        base_value = float(explainer.expected_value)
        
    feature_impact = list(zip(df.columns, vals.tolist()))
    feature_impact.sort(key=lambda x: abs(x[1]), reverse=True)
    
    top_factors = []
    for feature, impact in feature_impact[:5]:
        top_factors.append(FactorImpact(
            feature=feature,
            impact=round(float(impact), 4),
            direction="Improves Chances" if impact > 0 else "Reduces Chances",
            interpretation=interpret_feature(feature, impact)
        ))
        
    prediction_value = base_value + sum(vals)
    return ExplainResponse(
        top_contributing_factors=top_factors,
        base_value=round(base_value, 4),
        prediction_value=round(float(prediction_value), 4)
    )

# Basic what-if replacing Medical conditions with CGPA/Internships
@app.post("/whatif")
async def whatif_analysis(data: StudentData):
    if model is None or preprocessor is None:
        raise HTTPException(status_code=503, detail="Model not loaded.")
    
    def _predict(d: dict) -> float:
        df = prepare_input(StudentData(**d))
        df_processed = preprocessor.transform(df)
        return float(model.predict_proba(df_processed)[0][1]) * 100

    orig_dict = data.model_dump()
    orig_risk = _predict(orig_dict)
    orig_level, _ = get_placement_level(orig_risk / 100)
    
    scenarios = []
    sid = 1

    def add_scenario(mod_dict, title, description, change_summary, icon, factor_changed, original_value, suggested_value, mod_risk=None):
        nonlocal sid
        if mod_risk is None:
            mod_risk = _predict(mod_dict)
        scenarios.append({
            "scenario_id": sid,
            "title": title,
            "description": description,
            "change_summary": change_summary,
            "original_risk": orig_risk,
            "modified_risk": mod_risk,
            "risk_delta": mod_risk - orig_risk,
            "risk_reduction_percent": ((mod_risk - orig_risk) / (orig_risk if orig_risk > 0 else 1) * 100),
            "icon": icon,
            "factor_changed": factor_changed,
            "original_value": str(original_value),
            "suggested_value": str(suggested_value)
        })
        sid += 1

    # PR Specific: Career Path Suggestion (from Routing Engine)
    if routing_engine and data.skills and data.desired_role:
        rec_job, _ = routing_engine.recommend(data.skills)
        if rec_job:
            transition = routing_engine.get_career_transition_path(rec_job, data.desired_role)
            if transition and transition.get("skills_to_learn"):
                # Show full list of skills to learn (do not truncate to 3)
                skills_list = ", ".join(transition["skills_to_learn"])
                add_scenario(
                    orig_dict, "Career Path Suggestion", 
                    "Learning these skills can help you transition towards your desired role.",
                    f"Learn: {skills_list}", "🧠", "Skills",
                    "Current Skills", skills_list, mod_risk=orig_risk
                )
    
    # Increase CGPA
    if data.CGPA < 9.0:
        mod = orig_dict.copy()
        mod['CGPA'] = min(data.CGPA + 1.0, 10.0)
        add_scenario(
            mod, "+1.0 CGPA", "What if you improved your CGPA?",
            f"CGPA: {data.CGPA} → {mod['CGPA']}", "📚", "CGPA",
            str(data.CGPA), str(mod['CGPA'])
        )

    # Add Internships
    if data.Internships < 3:
        mod = orig_dict.copy()
        mod['Internships'] += 1
        add_scenario(
            mod, "Extra Internship", "What if you did one more internship?",
            f"Internships: {data.Internships} → {mod['Internships']}", "💼", "Internships",
            str(data.Internships), str(mod['Internships'])
        )
        
    # Clear Backlogs
    if data.HistoryOfBacklogs == 1:
        mod = orig_dict.copy()
        mod['HistoryOfBacklogs'] = 0
        add_scenario(
            mod, "Clear Backlogs", "What if you had no history of backlogs?",
            "Backlogs: Yes → No", "✅", "HistoryOfBacklogs",
            "Yes", "No"
        )
        
    # Change Stream
    if data.Stream != "Computer Science" and "Computer Science" in le_stream.classes_:
        mod = orig_dict.copy()
        mod['Stream'] = "Computer Science"
        mod_risk = _predict(mod)
        if mod_risk > orig_risk:
            add_scenario(
                mod, "Switch to CS", "What if you switched your stream to Computer Science?",
                f"Stream: {data.Stream} → Computer Science", "💻", "Stream",
                data.Stream, "Computer Science", mod_risk=mod_risk
            )
            
    # Stay in Hostel
    if data.Hostel == 0:
        mod = orig_dict.copy()
        mod['Hostel'] = 1
        mod_risk = _predict(mod)
        if mod_risk > orig_risk:
            add_scenario(
                mod, "Stay in Hostel", "What if you stayed in a hostel?",
                "Hostel: No → Yes", "🏢", "Hostel",
                "No", "Yes", mod_risk=mod_risk
            )

    scenarios.sort(key=lambda x: x['risk_delta'], reverse=True)
    
    # Combined Best Case
    best_case = orig_dict.copy()
    if data.CGPA < 9.0: best_case['CGPA'] = min(data.CGPA + 1.0, 10.0)
    if data.Internships < 3: best_case['Internships'] += 1
    if data.HistoryOfBacklogs == 1: best_case['HistoryOfBacklogs'] = 0
    
    combined_risk = _predict(best_case)
    combined_level, _ = get_placement_level(combined_risk / 100)

    return {
        "original_risk": orig_risk,
        "original_risk_level": orig_level,
        "scenarios": scenarios,
        "best_scenario": scenarios[0] if scenarios else None,
        "combined_risk": combined_risk, 
        "combined_risk_level": combined_level
    }

@app.post("/upload/resume")
async def upload_resume(file: UploadFile = File(...)):
    text = ""
    try:
        if file.filename.endswith('.pdf'):
            reader = PyPDF2.PdfReader(file.file)
            for page in reader.pages:
                extracted = page.extract_text()
                if extracted:
                    text += extracted + "\n"
        else:
            text = (await file.read()).decode('utf-8', errors='ignore')
        return {"status": "success", "resume_text": text}
    except Exception as e:
        logger.error(f"Resume parsing error: {e}")
        return {"status": "error", "resume_text": ""}

@app.post("/chat/transcribe")
async def chat_transcribe(audio_file: UploadFile = File(...)):
    """Only transcribes audio to text (STT) without triggering LLM/TTS"""
    if not CHAT_AVAILABLE:
        raise HTTPException(status_code=503, detail="Chat not available")
        
    with tempfile.NamedTemporaryFile(delete=False, suffix=".webm") as tmp:
        tmp.write(await audio_file.read())
        tmp_path = tmp.name

    try:
        stt = get_sarvam_service()
        result = stt.transcribe_sync(Path(tmp_path), language_code="ta-IN")
        transcript = result.transcript

        if not transcript or transcript.strip() == "":
            return {"transcript": ""}

        return {"transcript": transcript}
    except Exception as e:
        logger.error(f"Transcription error: {e}")
        return {"transcript": ""}
    finally:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)

@app.post("/chat/audio")
async def chat_audio(session_id: str = Form(...), audio_file: UploadFile = File(...)):
    if not CHAT_AVAILABLE:
        raise HTTPException(status_code=503, detail="Chat not available")
        
    with tempfile.NamedTemporaryFile(delete=False, suffix=".webm") as tmp:
        tmp.write(await audio_file.read())
        tmp_path = tmp.name

    try:
        stt = get_sarvam_service()
        result = stt.transcribe_sync(Path(tmp_path), language_code="ta-IN")
        transcript = result.transcript

        if not transcript or transcript.strip() == "":
            return {"transcript": "(Inaudible)", "response": "Kanna, your voice broke! Ennaku kekala. Can you repeat?"}

        resp = await get_chat_response(session_id, transcript)
        audio_b64 = get_tts_base64(resp, speaker="priya") # Using a female voice for Paati
        
        prog = get_progress(session_id)
        points_update = False
        
        points_match = re.search(r'(\d+)\s*(?:Paati[-\u2011\s]*Points|points)', resp, re.IGNORECASE)
        if points_match:
            points_update = True
            prog["points"] += int(points_match.group(1))
            prog["kurals"] += 1
            
        level_match = re.search(r'(Sapling|Tree)', resp, re.IGNORECASE)
        if level_match:
            points_update = True
            prog["level"] = level_match.group(1).capitalize()
            if prog["level"] == "Sapling":
                prog["level"] += " (Chedi)"
            elif prog["level"] == "Tree":
                prog["level"] += " (Maram)"
                
        save_progress(session_id, prog)
        
        return {
            "transcript": transcript,
            "response": resp,
            "audio_base64": audio_b64,
            "points_update": points_update,
            "new_points": prog["points"],
            "new_level": prog["level"],
            "new_kurals": f"{prog['kurals']}/1330"
        }
    finally:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)

@app.post("/chat/start")
async def chat_s(req: dict):
    if not CHAT_AVAILABLE:
        raise HTTPException(status_code=503)
    
    # Handle both new and old keys, and default to empty dict to prevent AttributeErrors
    student_data = req.get('student_data') or req.get('patient_data') or {}
    prediction = req.get('prediction') or {}
    explanation = req.get('explanation') or {}
    whatif = req.get('whatif') or {}
    
    sid, greet = await start_chat_session(student_data, prediction, explanation, whatif)
    
    # Generate TTS audio for the initial greeting
    audio_b64 = get_tts_base64(greet, speaker="priya") 
    
    save_progress(sid, dict(_DEFAULT_PROGRESS))
    
    return {
        "session_id": sid, 
        "message": greet,
        "audio_base64": audio_b64
    }

@app.post("/chat/message")
async def chat_m(req: dict):
    if not CHAT_AVAILABLE:
        raise HTTPException(status_code=503)
        
    sid = req.get('session_id')
    resp = await get_chat_response(sid, req.get('message'))
    audio_b64 = get_tts_base64(resp, speaker="priya") # Paati voice
    
    prog = get_progress(sid)
    points_update = False
    
    points_match = re.search(r'(\d+)\s*(?:Paati[-\u2011\s]*Points|points)', resp, re.IGNORECASE)
    if points_match:
        points_update = True
        prog["points"] += int(points_match.group(1))
        prog["kurals"] += 1
        
    level_match = re.search(r'(Sapling|Tree)', resp, re.IGNORECASE)
    if level_match:
        points_update = True
        prog["level"] = level_match.group(1).capitalize()
        if prog["level"] == "Sapling":
            prog["level"] += " (Chedi)"
        elif prog["level"] == "Tree":
            prog["level"] += " (Maram)"
            
    save_progress(sid, prog)
    
    return {
        "response": resp,
        "audio_base64": audio_b64,
        "points_update": points_update,
        "new_points": prog["points"],
        "new_level": prog["level"],
        "new_kurals": f"{prog['kurals']}/1330"
    }

# --- Static File Serving for Frontend ---
# 1. Provide styles.css and script.js directly at root level
@app.get("/styles.css")
async def read_styles():
    return FileResponse("styles.css")

@app.get("/script.js")
async def read_js():
    return FileResponse("script.js")

# 2. Serve index.html at root
@app.get("/")
async def read_index():
    return FileResponse("index.html")

# 3. Handle favicon if needed
@app.get("/favicon.ico", include_in_schema=False)
async def favicon():
    return FileResponse("favicon.ico")


# ---------------------------------------------------------------------------
# AgentOS integration
# Wraps the existing FastAPI app with AgentOS so that os.agno.com can
# connect to this server and surface /agents, /sessions, /runs, /memories,
# /knowledge, /metrics, /evals endpoints automatically.
# All existing routes (/predict, /explain, /chat/*, static files, etc.) are
# preserved unchanged via base_app.
# ---------------------------------------------------------------------------
try:
    from agno.os import AgentOS
    from agno.os.config import AgentOSConfig, EvalsConfig

    _agents_list = [paati_agent] if paati_agent is not None else []
    if _agents_list:
        _os_config = AgentOSConfig(
            available_models=[
                "nvidia:nvidia/nemotron-3-super-120b-a12b",
                "nvidia:meta/llama-3.3-70b-instruct",
            ],
            evals=EvalsConfig(
                available_models=[
                    "nvidia:nvidia/nemotron-3-super-120b-a12b",
                    "nvidia:meta/llama-3.3-70b-instruct",
                ]
            ),
        )
        from llm import _chat_db as _paati_db
        agent_os = AgentOS(
            description="PlacementPredictor+ powered by Paati AI",
            agents=_agents_list,
            base_app=app,
            db=_paati_db,      # surfaces sessions in os.agno.com session browser
            tracing=True,      # enables trace viewer on os.agno.com
            config=_os_config, # enables Evals tab
        )
        app = agent_os.get_app()
        # Re-apply CORS on the outer AgentOS app.
        # AgentOS.get_app() returns a new ASGI wrapper; our original CORSMiddleware
        # only covers the inner FastAPI app. Adding it here makes it the outermost
        # layer so all routes — including AgentOS's /health, /sessions, etc. — get
        # the correct Access-Control-Allow-Origin headers.
        from fastapi.middleware.cors import CORSMiddleware as _CORS
        app.add_middleware(
            _CORS,
            allow_origins=["*"],
            allow_methods=["*"],
            allow_headers=["*"],
        )
        logger.info("[OK] AgentOS wrapper applied — os.agno.com endpoints + tracing + evals active")
except ImportError as _aos_imp_err:
    logger.warning(f"[!] agno.os not available — AgentOS features disabled: {_aos_imp_err}")
except Exception as _aos_err:
    logger.warning(f"[!] AgentOS unavailable (continuing without it): {_aos_err}")


if __name__ == "__main__":
    import uvicorn
    # Important for HF: Host 0.0.0.0 and Port 7860
    # workers=1: required while using SQLite for session state.
    # FastAPI/uvicorn handles many concurrent async requests on a single worker fine.
    # To scale beyond 1 worker, migrate SqliteDb -> PostgresDb (e.g. Neon cloud PG).
    port = int(os.environ.get("PORT", 7860))
    uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)