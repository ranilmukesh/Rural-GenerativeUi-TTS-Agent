---
title: PlacementPredictor+
emoji: 🎓
colorFrom: blue
colorTo: indigo
sdk: docker
pinned: false
license: mit
app_port: 7860
short_description: AI placement prediction and career routing.
---

# PlacementPredictor+ | AI Career Router

Advanced machine learning with real-time explainable insights for personalized career path routing. This campus placement AI app acts as a placement predictor and AI career router, giving students and graduates a realistic assessment of job prospects based on CGPA, internships, and skill sets.


## Features
- **XGBoost Prediction**: High accuracy placement probability powered by the best in campus placement AI and job skills analysis.
- **SHAP Explainability**: Understand the "why" behind your result and gain clarity on factors affecting your placement chances.
- **Career Routing**: Knowledge graph-based skill gap analysis.
- **AI Career Assistant**: Natural language chat for personalized guidance.

## Local Setup
```bash
pip install -r requirements.txt
python main.py
```

## Deployment
Running on Hugging Face Spaces via Docker.

## Table of Contents
1. [Overview](#overview)
2. [Features](#features)
3. [Tech Stack](#tech-stack)
4. [Local Setup](#local-setup)
5. [Usage](#usage)
   - [Web Interface](#web-interface)
   - [API Endpoints](#api-endpoints)
6. [Deployment](#deployment)
7. [Development](#development)
8. [Contributing](#contributing)
9. [License](#license)
10. [Contact & Support](#contact--support)

## Overview
PlacementPredictor+ is an open‑source, AI-driven placement predictor and career guidance tool designed specifically for college students and fresh graduates in India and beyond. This geo‑aware education AI leverages campus placement AI methodologies to estimate chances of securing jobs and internships. By blending predictive modeling, explainable AI, and a knowledge graph, it routes users toward suitable roles while highlighting skill gaps.


## Tech Stack
- **Backend**: FastAPI (Python)
- **Modeling**: XGBoost, SHAP for explanation, scikit-learn utilities
- **Routing Engine**: Custom NetworkX knowledge graph
- **Frontend**: Vanilla JavaScript, HTML5, CSS3
- **LLM Chat**: NVIDIA AGNO SDK (Llama/Minimax)
- **Deployment**: Docker, Hugging Face Spaces (port 7860)

## Usage

PlacementPredictor+ is optimized for SEO with keywords such as placement predictor, AI career router, job skills analysis, and education AI appearing throughout this document.
### Web Interface
1. Start the backend:
   ```bash
   pip install -r requirements.txt
   python main.py
   ```
2. Open a browser and navigate to `http://localhost:7860` (or the port configured in your environment).
3. Fill in your details, add existing skills, and optionally select a desired job role. Submit to view placement probability, factor explanations, recommended job, missing skills, and an interactive career path chart.

### API Endpoints
All endpoints return JSON and can be consumed programmatically.

| Path | Method | Description |
|------|--------|-------------|
| `/health` | GET | Check service health and model availability |
| `/options` | GET | Retrieve available streams, skills, and job roles |
| `/predict` | POST | Get placement prediction & recommendations (see `StudentData` schema) |
| `/explain` | POST | Obtain SHAP-based explanation for a prediction |
| `/whatif` | POST | Run hypothetical scenarios (increase CGPA, internships, clear backlogs) |

Use the `StudentData` Pydantic model schema from the backend as reference for payload structure.

## Development
- Artifacts (trained model, encoders, routing graph) are stored in `placement_artifacts.pkl`.
- To retrain, run `python train_model.py` and ensure `requirements.txt` contains needed libraries (e.g., `networkx`, `matplotlib`).
- Frontend assets live in the project root and are served statically by FastAPI when running in production.

## Contributing
1. Fork the repository and create a feature branch.
2. Make your changes and add tests where appropriate.
3. Open a pull request detailing the enhancement or bug fix.

Please follow standard GitHub flow and ensure your code passes linting/formatting rules. Issues and feature requests are welcome.

## License
This project is released under the [MIT License](LICENSE).


<!-- Hidden SEO keywords: placement predictor, AI career router, campus placement AI, job skills analysis, career guidance tool, XGBoost placement, SHAP explainability, education AI, student job forecasting, vocational guidance AI, placement probability calculator, career planning AI, personalized job recommendations, skill gap analysis, placement assistance, geo-targeted career tool, seo friendly placement app, aeo optimization, geo-specific education technology -->
