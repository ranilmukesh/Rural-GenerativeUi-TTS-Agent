# 👵 Paati-Kural League (பாட்டி-குறள் லீக்)

> **Karka Kasadara... Karpavai Katrapin Nitka Atharku Thaga.**
> *"Learn flawlessly... and after learning, live by those values." — Thirukkural*

![Paati-Kural League Poster](./paati%20poster.png)

**Paati-Kural League** is a culturally-rooted, gamified AI learning ecosystem designed to bridge the gap for rural students. It combines state-of-the-art AI with the warmth of a grandmother's wisdom, using **Thirukkural ethics** and **Tenali Raman folklore** to teach modern employability skills (SQL, Logic, Career Planning).

---

## 🎯 The Mission

### ❗ The Problem
- **Low Engagement:** High disengagement in rural digital learning.
- **Cultural Gap:** Standard EdTech feels alien and corporate to village students.
- **Guidance Deficit:** Lack of localized career coaching and placement mentorship.

### ✅ Our Solution
A voice-first, AI-powered "Paati" (Grandma) tutor that makes education **relatable, interactive, and rewarding**.
- **Voice-First:** Uses Sarvam AI for rural Tamil/English interaction.
- **Culture-Rooted:** Teaches through stories and ethical puzzles.
- **Outcome-Driven:** Direct linkage between learning games and placement probability.

---

## 🚀 Core Innovations

| Feature | Cultural Flavor | Technical Backbone |
|---|---|---|
| 👵 **Paati AI Tutor** | Endearing Tanglish persona | **NVIDIA Nemotron-3** + **Agno AgentOS** |
| 🎙️ **Voice First** | "Our Language" (Rural Tamil) | **Sarvam AI** (ASR + TTS) |
| 🎮 **Gamified League** | Seed → Sapling → Tree levels | **SQLite** persistent progress engine |
| 🧩 **Tenali Puzzles** | Logic via folklore | Generative UI (React / PuzzleCards) |
| 📈 **XGBoost Prediction** | "Jathagam" of your career | **XGBoost** + **SHAP** Explainability |
| 🗺️ **Career Routing** | Skill-gap mapping | **NetworkX** Knowledge Graph |

---

## 🛠️ How It Works (Student Journey)

1. **Interact:** Student sends a voice note or message (WhatsApp-first vision).
2. **Paati Responds:** AI Paati replies with a Thirukkural, a story, or a concept explanation.
3. **Learn & Engage:** Students attempt generative mini-games (SQL sorting, bug hunts).
4. **Assessment:** The system predicts placement probability based on their current profile.
5. **Get Rewarded:** Earn **Paati Points** and certificates to unlock real micro-job links.
6. **Parent Updates:** Automated progress reports for the family.

---

## 📊 Impact Goals
- **100,000+** Students Impacted
- **90%+** Retention through gamified learning
- **Future-Ready** skills for better employability
- **Stronger, Smarter** rural communities

---

## Table of Contents

1. [Architecture](#architecture)
2. [Project Structure](#project-structure)
3. [Quick Start](#quick-start)
4. [Environment Variables](#environment-variables)
5. [API Reference](#api-reference)
6. [Data Schemas](#data-schemas)
7. [Training the Model](#training-the-model)
8. [Docker & Deployment](#docker--deployment)
9. [Tech Stack](#tech-stack)
10. [License](#license)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    PAATI-KURAL LEAGUE                       │
├───────────────────┬────────────────────┬────────────────────┤
│  React UI         │  FastAPI Backend   │  External APIs     │
│  (Vite, port 5173)│  (Python, port 7860│                    │
│                   │   or 8000 locally) │                    │
│  SidebarLeft      │  /predict          │  NVIDIA NIM        │
│  AssessmentForm   │  /explain          │  (Llama / AGNO)    │
│  ResultsPanel     │  /whatif           │                    │
│  ChatView         │  /chat/start       │  Sarvam AI         │
│  SidebarRight     │  /chat/message     │  (STT + TTS)       │
│  (Voice/Tools)    │  /chat/audio       │  ta-IN language    │
│                   │  /chat/transcribe  │                    │
│  api.js           │  /upload/resume    │                    │
│  utils.js         │  /options          │                    │
│                   │  /health           │                    │
└───────────────────┴────────────────────┴────────────────────┘
         ↑                    ↓
    Vite proxy          placement_artifacts.pkl
    (dev mode)          XGBoost + SHAP + RoutingEngine
```

---

## Project Structure

```
edu-hack-2026/
├── main.py                  # FastAPI app — all routes
├── llm.py                   # NVIDIA AGNO chat agent (Paati persona)
├── llm_tools.py             # Agent tools (search, progress tracking)
├── routing_engine.py        # NetworkX knowledge graph for skill routing
├── sarvam_stt.py            # Sarvam STT service wrapper
├── sarvam_tts.py            # Sarvam TTS service wrapper
├── train_model.py           # Model training script
├── models.py                # SQLModel DB models
├── requirements.txt         # Python dependencies
├── Dockerfile               # HuggingFace Spaces Docker config
├── placement_artifacts.pkl  # Trained model + encoders + graph (generated)
├── collegePlace.csv         # Training dataset
├── .env                     # API keys (see below)
│
└── paati-ui/                # React frontend (Vite)
    ├── index.html
    ├── vite.config.js       # Proxy → localhost:8000
    ├── start.bat            # One-click launcher (Windows)
    └── src/
        ├── main.jsx
        ├── App.jsx          # Root: view routing, lifted chat state
        ├── api.js           # All fetch calls to backend
        ├── utils.js         # Helpers: formatFeatureName, animateCounter, etc.
        ├── index.css        # Full design system
        ├── chat.css         # Chat bubble styles
        └── components/
            ├── SidebarLeft.jsx    # Nav + chat history + user card
            ├── SidebarRight.jsx   # Voice waveform + Thinking + Tools
            ├── AssessmentForm.jsx # Student form with skills autocomplete
            ├── ResultsPanel.jsx   # Risk card + SHAP factors + What-If
            └── ChatView.jsx       # Full chat UI (messages + mini-games + voice)
```

---

## Quick Start

### Backend (FastAPI)

**Prerequisites:** Python 3.10+

```bash
# 1. Clone the repo
git clone https://github.com/ranilmukesh/Rural-GenerativeUi-TTS-Agent
cd Rural-GenerativeUi-TTS-Agent

# 2. Install dependencies
pip install -r requirements.txt

# 3. Create your .env file (see Environment Variables below)
cp .env.example .env
# Edit .env and add your keys

# 4. Train the model (only needed once, or if you change the dataset)
python train_model.py

# 5. Start the backend
python main.py
# → Listening on http://localhost:7860  (or set PORT env var)
```

> **For local development with the React UI**, the backend should run on **port 8000**:
> ```bash
> PORT=8000 python main.py
> ```
> The Vite dev server proxies all API calls to `localhost:8000`.

---

### Frontend (React + Vite)

**Prerequisites:** Node.js 18+

```bash
cd paati-ui

# Install dependencies (first time only)
npm install

# Start dev server (proxies to backend on :8000)
npm run dev
# → http://localhost:5173
```

**One-click start (Windows — starts both backend + UI):**
```bat
paati-ui\start.bat
```

**Production build:**
```bash
npm run build
# Output in paati-ui/dist/ — serve statically or from FastAPI
```

---

## Environment Variables

Create a `.env` file in the project root:

```env
# ── NVIDIA NIM / AGNO (LLM for Paati AI chat) ──────────────────
# Get your key: https://build.nvidia.com/
NVIDIA_API_KEY=nvapi-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# ── Sarvam AI (STT + TTS — Tamil/English voice) ────────────────
# Get your key: https://dashboard.sarvam.ai/
SARVAM_API_KEY=sk_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

> **Note:** Both keys are required for full functionality.
> - Without `NVIDIA_API_KEY` → Paati AI chat will be unavailable (`503`)
> - Without `SARVAM_API_KEY` → Voice features (STT/TTS) will silently fail; text chat still works

**Sample `.env.example`:**
```env
NVIDIA_API_KEY=nvapi-your-key-here
SARVAM_API_KEY=sk_your-key-here
```

---

## API Reference

All endpoints served by `main.py` (FastAPI). Interactive docs at `http://localhost:8000/docs`.

---

### `GET /health`

Check service health and model status.

**Response:**
```json
{
  "status": "healthy",
  "model_loaded": true
}
```

---

### `GET /options`

Fetch available form options (populated from model encoders + routing graph).

**Response:**
```json
{
  "streams": ["Computer Science", "Information Technology", "Electronics", "..."],
  "skills": ["Python", "SQL", "Java", "React", "Machine Learning", "..."],
  "jobs": ["Software Engineer", "Data Analyst", "Web Developer", "..."]
}
```

---

### `POST /predict`

Run XGBoost placement prediction.

**Request body:**
```json
{
  "Age": 21,
  "Gender": "Female",
  "Stream": "Information Technology",
  "Internships": 1,
  "CGPA": 7.5,
  "Hostel": 1,
  "HistoryOfBacklogs": 0,
  "skills": ["Python", "SQL", "Git"],
  "desired_role": "Data Analyst",
  "resume_text": ""
}
```

**Response:**
```json
{
  "prediction": 1,
  "probability_percentage": 78.43,
  "risk_level": "LOW",
  "confidence": "Very High Confidence",
  "recommended_job": "Data Analyst",
  "missing_skills": ["Tableau", "Power BI"],
  "graph_data": "<base64-encoded PNG>",
  "bridges": [
    { "title": "Direct Interview", "desc": "Paati is matching you with local micro-jobs." }
  ]
}
```

| `risk_level` | Meaning |
|---|---|
| `LOW` | High placement chance (≥ 50%) |
| `MEDIUM` | Moderate chance (30–50%) |
| `HIGH` | Low placement chance (< 30%) |

---

### `POST /explain`

Get SHAP-based explanation for a prediction.

**Request body:** Same as `/predict` (`StudentData`)

**Response:**
```json
{
  "top_contributing_factors": [
    {
      "feature": "CGPA",
      "impact": 0.312,
      "direction": "Improves Chances",
      "interpretation": "Cgpa significantly improves placement chances"
    },
    {
      "feature": "HistoryOfBacklogs",
      "impact": -0.198,
      "direction": "Reduces Chances",
      "interpretation": "History Of Backlogs moderately reduces placement chances"
    }
  ],
  "base_value": 0.4821,
  "prediction_value": 0.7843
}
```

---

### `POST /whatif`

Run hypothetical scenario analysis (what changes would help the most).

**Request body:** Same as `/predict` (`StudentData`)

**Response:**
```json
{
  "original_risk": 44.4,
  "original_risk_level": "MEDIUM",
  "scenarios": [
    {
      "scenario_id": 1,
      "title": "+1.0 CGPA",
      "description": "What if you improved your CGPA?",
      "change_summary": "CGPA: 7.5 → 8.5",
      "original_risk": 44.4,
      "modified_risk": 98.1,
      "risk_delta": 53.7,
      "risk_reduction_percent": 121.0,
      "icon": "📚",
      "factor_changed": "CGPA",
      "original_value": "7.5",
      "suggested_value": "8.5"
    }
  ],
  "best_scenario": { "...": "..." },
  "combined_risk": 98.5,
  "combined_risk_level": "LOW"
}
```

---

### `POST /upload/resume`

Parse a PDF or TXT resume and return its text content.

**Request:** `multipart/form-data` with field `file` (`.pdf` or `.txt`)

**Response:**
```json
{
  "status": "success",
  "resume_text": "John Doe\nSoftware Engineer\nSkills: Python, SQL..."
}
```

---

### `POST /chat/start`

Initialize a Paati AI chat session with the student's context.

**Request body:**
```json
{
  "student_data": { "Age": 21, "Gender": "Female", "..." : "..." },
  "prediction": { "probability_percentage": 78.43, "risk_level": "LOW", "..." },
  "explanation": { "top_contributing_factors": [ "..." ] },
  "whatif": { "scenarios": [ "..." ] }
}
```

**Response:**
```json
{
  "session_id": "uuid-string",
  "message": "Vanakkam kanna! 👵 I'm Paati...",
  "audio_base64": "<base64-encoded WAV for TTS greeting>"
}
```

---

### `POST /chat/message`

Send a text message in an existing session.

**Request body:**
```json
{
  "session_id": "uuid-string",
  "message": "What should I do to improve my chances?"
}
```

**Response:**
```json
{
  "response": "Kanna, your CGPA is a bit low...",
  "audio_base64": "<base64 WAV>",
  "points_update": true,
  "new_points": 50,
  "new_level": "Sapling (Chedi)",
  "new_kurals": "1/1330"
}
```

---

### `POST /chat/transcribe`

Transcribe audio to text (STT only — no LLM call).

**Request:** `multipart/form-data` with `audio_file` (`.webm`)

**Response:**
```json
{
  "transcript": "What should I study for data science?"
}
```

---

### `POST /chat/audio`

Full voice round-trip: STT → LLM → TTS.

**Request:** `multipart/form-data` with:
- `audio_file`: `.webm` audio blob
- `session_id`: string

**Response:**
```json
{
  "transcript": "What should I study for data science?",
  "response": "Kanna, for data science you need...",
  "audio_base64": "<base64 WAV>",
  "points_update": false,
  "new_points": 50,
  "new_level": "Sapling (Chedi)",
  "new_kurals": "1/1330"
}
```

---

## Data Schemas

### `StudentData` (Pydantic model)

| Field | Type | Constraints | Description |
|---|---|---|---|
| `Age` | `int` | 15–40 | Student age |
| `Gender` | `str` | `"Male"` or `"Female"` | Gender |
| `Stream` | `str` | From `/options` | Branch of study |
| `Internships` | `int` | ≥ 0 | Number of internships |
| `CGPA` | `float` | 0–10 | Cumulative GPA |
| `Hostel` | `int` | `0` or `1` | Hostel resident |
| `HistoryOfBacklogs` | `int` | `0` or `1` | Any academic backlogs |
| `skills` | `List[str]` | Optional | User's current skills |
| `desired_role` | `str` | Optional | Target job role |
| `resume_text` | `str` | Optional | Parsed resume text |

### Gamification Levels

| Level | Tamil Name | Trigger |
|---|---|---|
| Seed | Vithu | Default (starting) |
| Sapling | Chedi | Mentioned in Paati's response |
| Tree | Maram | Advanced level |

---

## Training the Model

```bash
# Uses collegePlace.csv to train XGBoost + SHAP + RoutingEngine
python train_model.py

# Output: placement_artifacts.pkl
# Contains: model, shap_model, preprocessor, le_gender, le_stream, routing_engine
```

The routing engine builds a NetworkX knowledge graph from `Tech_Data_Cleaned.csv`, linking skills → job roles for gap analysis and career path recommendations.

---

## Docker & Deployment

The app is deployed on **Hugging Face Spaces** via Docker.

```dockerfile
# Relevant from Dockerfile:
# Port: 7860 (HF Spaces standard)
# Entry: python main.py
# PORT env var controls uvicorn port
```

**Build locally:**
```bash
docker build -t paati-kural .
docker run -p 7860:7860 \
  -e NVIDIA_API_KEY=nvapi-xxxx \
  -e SARVAM_API_KEY=sk_xxxx \
  paati-kural
```

**Environment for HF Spaces:** Set `NVIDIA_API_KEY` and `SARVAM_API_KEY` in your Space's **Settings → Repository Secrets**.

> **Note on ports:** When running locally for development, set `PORT=8000` so the Vite proxy works. In production (Docker / HF Spaces), the default port is `7860`.

---

## Tech Stack

| Layer | Technology |
|---|---|
| **Frontend** | React 18 + Vite, Lucide React icons, vanilla CSS design system |
| **Backend** | FastAPI (Python), Uvicorn |
| **ML Model** | XGBoost, scikit-learn, SHAP |
| **Knowledge Graph** | NetworkX, Matplotlib |
| **LLM / Agent** | NVIDIA AGNO SDK (Llama 3.1 / Minimax) |
| **Voice STT** | Sarvam AI — Tamil + English (`ta-IN`) |
| **Voice TTS** | Sarvam AI — `priya` voice |
| **Resume Parsing** | PyPDF2 |
| **Deployment** | Docker, Hugging Face Spaces |
| **Dataset** | `collegePlace.csv` (campus placement data) |

---

## License

Released under the [MIT License](LICENSE).

<!-- SEO: placement predictor, rural student AI, Tamil career advisor, campus placement AI, job skills analysis, SHAP explainability, education AI India, student job forecasting, placement probability calculator, career planning AI, skill gap analysis, Paati-Kural League -->
