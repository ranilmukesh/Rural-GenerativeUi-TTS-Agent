@echo off
title PlacementPredictor+ Launcher
color 0A

echo ============================================================
echo          PlacementPredictor+ - AI Career Router
echo ============================================================
echo.

:: Check if Python is installed
python --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Python is not installed or not in PATH!
    echo Please install Python from https://python.org
    pause
    exit /b 1
)

echo [1/6] Installing/Updating dependencies...
pip install pandas numpy scikit-learn xgboost shap fastapi uvicorn joblib imbalanced-learn pydantic agno sqlalchemy python-dotenv --quiet
if errorlevel 1 (
    echo [WARNING] Some packages may have failed to install.
) else (
    echo       Done!
)
echo.

:: Also clean legacy CardioSense artifacts if present
if exist "cardiosense_artifacts.pkl" (
    del /f /q "cardiosense_artifacts.pkl"
    echo       Legacy CardioSense artifacts removed.
)

:: Smart PKL Caching: check if --force-train was passed
set FORCE_TRAIN=0
for %%a in (%*) do (
    if "%%a"=="--force-train" set FORCE_TRAIN=1
)

if "%FORCE_TRAIN%"=="1" (
    echo [2/6] Force-train requested. Deleting old artifacts...
    if exist "placement_artifacts.pkl" del /f /q "placement_artifacts.pkl"
    echo       Old artifacts deleted!
    goto :DO_TRAIN
)

:: If pkl exists, skip training
if exist "placement_artifacts.pkl" (
    echo [2/6] Found existing model artifacts. Skipping training...
    echo       TIP: Use --force-train flag to retrain: start_placement_predictor.bat --force-train
    goto :SKIP_TRAIN
)

echo [2/6] No model artifacts found. Training required...

:DO_TRAIN
:: Set Nvidia API Key first (needed before training)
echo [3/6] Setting up AI Chat credentials...






echo       API Keys configured!
echo.

echo [4/6] Training PlacementPredictor+ model...
echo       This trains the XGBoost Classifier for Placement.
echo.
python train_model.py
if errorlevel 1 (
    echo [ERROR] Model training failed!
    echo Please check if all diabetes CSV dataset files exist in this folder.
    pause
    exit /b 1
)
echo       Model trained successfully!
echo.
goto :START_SERVERS

:SKIP_TRAIN
:: Set Nvidia API Key
echo [3/6] Setting up AI Chat credentials...






echo       API Keys configured!
echo.
echo [4/6] Skipped training (using cached model).
echo.

:START_SERVERS
echo [5/6] Starting API Server on http://127.0.0.1:8000 ...
start "PlacementPredictor+ API" cmd /k "title PlacementPredictor+ API Server && color 0B && python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 1"
echo       Waiting for API to start...
timeout /t 30 /nobreak >nul

:: Validate the model loaded correctly by hitting /health
echo       Validating model artifacts...
curl -s http://127.0.0.1:8000/health 2>nul | findstr /C:"model_loaded" | findstr /C:"true" >nul 2>&1
if errorlevel 1 (
    echo [!] Model failed to load - version mismatch or corrupted pkl.
    echo       Killing API server and retraining...
    taskkill /FI "WINDOWTITLE eq PlacementPredictor+ API Server" /F >nul 2>&1
    timeout /t 2 /nobreak >nul
    if exist "placement_artifacts.pkl" del /f /q "placement_artifacts.pkl"
    echo.
    echo [4/6] Retraining model...
    python train_model.py
    if errorlevel 1 (
        echo [ERROR] Retraining also failed!
        pause
        exit /b 1
    )
    echo       Retrained successfully!
    echo.
    echo [5/6] Restarting API Server...
    start "PlacementPredictor+ API" cmd /k "title PlacementPredictor+ API Server && color 0B && python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 1"
    timeout /t 80 /nobreak >nul
)
echo       API Server started!
echo.

echo [6/6] Starting Frontend Server on http://127.0.0.1:3000 ...
start "PlacementPredictor+ Frontend" cmd /k "title PlacementPredictor+ Frontend && color 0E && python -m http.server 3000"
timeout /t 2 /nobreak >nul
echo       Frontend Server started!
echo.

echo ============================================================
echo                    SERVERS ARE RUNNING!
echo ============================================================
echo.
echo   API Documentation:  http://127.0.0.1:8000/docs
echo   Frontend UI:        http://127.0.0.1:3000
echo.
echo   TIP: Use the "Load JSON" button in the UI for quick testing
echo   TIP: Click the chat bubble after results for AI assistant
echo   TIP: Use --force-train flag to force model retraining
echo.
echo ============================================================
echo.

:: Open the frontend in default browser
echo Opening PlacementPredictor+ in your browser...
timeout /t 2 /nobreak >nul
start http://127.0.0.1:3000

echo.
echo Press any key to close this launcher (servers will keep running)...
pause >nul
