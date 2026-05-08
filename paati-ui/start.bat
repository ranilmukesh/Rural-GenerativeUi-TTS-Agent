@echo off
title Paati-Kural League - New UI
cd /d "%~dp0"

echo.
echo  ==========================================
echo   Paati-Kural League - Starting Services
echo  ==========================================
echo.

REM --- Load .env from parent directory (optional, no error if missing) ---
if exist "..\\.env" (
    for /f "usebackq tokens=1,2 delims==" %%a in ("..\\.env") do (
        if not "%%a"=="" if not "%%b"=="" set "%%a=%%b"
    )
)

REM --- Start FastAPI backend in a new window ---
echo [1/2] Starting FastAPI backend on port 8000...
start "Paati Backend" cmd /k "cd /d "%~dp0.." && python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4"

REM --- Wait a moment for backend to init ---
timeout /t 3 /nobreak >nul

REM --- Start Vite React dev server (stays in paati-ui dir) ---
echo [2/2] Starting Vite React UI on port 5173...
echo.
echo  Open your browser at: http://localhost:5173
echo.

npm run dev
