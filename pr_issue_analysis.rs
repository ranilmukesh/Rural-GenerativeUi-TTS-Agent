// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)// # PR Issue Analysis — `gemini-code-assist-review-16364590137272262165`

// > Cross-checked against actual codebase. No file changes made.

// ---

// ## Issues Found (Confirmed in Code)

// ### Issue 1 — CRITICAL: Multiple Uvicorn Workers + In-Memory Session State

// **Files:** `main.py` (line 661), `paati-ui/start.bat` (line 20), `start_legacy.bat` (lines 98 & 122)

// **What's wrong:**
// All three startup paths now launch uvicorn with `--workers 4`. The session state (`user_progress_db`, line 26 of `main.py`) is a plain Python dict living in process memory. With 4 workers, each request can land on a different OS process — those processes don't share memory. Result:

// - `/chat/start` initializes a session in Worker A
// - `/chat/message` or `/chat/audio` hits Worker B — `user_progress_db.get(sid)` returns the `{}` default, points reset to 0
// - Gamification (points, level, kurals) is **completely broken** across multi-worker deploys
// - TTS/audio responses still work but the scoreboard is wrong every time

// **Confirmed in code:**
// ```python
// # main.py L26
// user_progress_db = {}  # dies on any other worker

// # main.py L661
// uvicorn.run("main:app", host="0.0.0.0", port=port, workers=4)  # BREAKS the above

// # paati-ui/start.bat L20
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem

// # start_legacy.bat L98 + L122
// python -m uvicorn main:app --host 127.0.0.1 --port 8000 --workers 4  # same problem
// ```

// The Gemini Code Assist bot flagged this on all 3 files. **None were fixed.** Jules introduced `--workers 4` without migrating state to Redis/DB.

// ---

// ### Issue 2 — MEDIUM: `RiskCard.jsx` — RAF + setTimeout Without Cleanup

// **File:** `paati-ui/src/components/common/RiskCard.jsx` (lines 8–31)

// **What's wrong:**
// - `requestAnimationFrame` loop runs until animation completes (~1.5 seconds)
// - `setTimeout` fires at 100ms
// - No cleanup function returned from `useEffect`
// - If user navigates away (e.g. clicks "Chat with Paati" before animation finishes), the component unmounts but the RAF loop and setTimeout keep firing
// - `setDisplayPct(...)` is called on an unmounted component → React warning / silent state corruption in React 18
// - `ringRef.current` / `pointerRef.current` DOM mutations after unmount touch stale DOM nodes

// **Confirmed:** The current `RiskCard.jsx` is exactly the unfixed version the bot flagged.

// ---

// ### Issue 3 — MEDIUM: `Simulator.jsx` — Stale `formData` Closure + No Cleanup

// **File:** `paati-ui/src/components/common/Simulator.jsx` (lines 12–21)

// **Two sub-problems:**

// 1. **Missing cleanup:** `clearTimeout(debounceRef.current)` is called at the top of the effect, but if the component unmounts while the 300ms timer is pending, `apiPredict(data)` still fires and `setSimResult(r)` is called on an unmounted component. No `return () => clearTimeout(...)` exists.

// 2. **Stale `formData` in dep array:** `formData` is spread into `data` inside the callback but is NOT in the dep array `[simAge, simIntern, simCGPA, simBacklog]`. If the parent re-renders with different `formData`, the closure captures the old value.

// **Confirmed:** The current `Simulator.jsx` is the unfixed version.

// ---

// ### Issue 4 — BONUS (not in PR but found on inspection): `AssessmentForm.jsx` Half-Migration

// **File:** `paati-ui/src/components/AssessmentForm.jsx` (lines 93–118)

// The PR description claims "Refactored to controlled components" — and `formData` state IS used. BUT `handleSubmit` still reads from `e.target` (DOM refs) instead of the `formData` state:

// ```jsx
// // reads from DOM, not from React state
// const fd = e.target;
// const data = {
//   Gender: fd.gender.value,     // DOM, not formData.gender
//   Age: parseInt(fd.age.value), // DOM
//   ...
// };
// ```

// This is a half-migration. Inputs are controlled (correct) but submission bypasses controlled state entirely. In practice it's in-sync, but it's fragile and defeats the purpose of controlled components.

// ---

// ### Issue 5 — LOW-SECURITY: API Keys in `.env` at Root

// **File:** `.env` (root level)

// The `.gitignore` DOES list `.env`, so it's not being pushed. The PR correctly removed hardcoded keys from `start_legacy.bat` (now blank lines at 60–64, 84–90). But the real keys are sitting in `.env` at the root. Anyone running `git add -A` carelessly could expose them.

// ---

// ### Issue 6 — LOW: `ChatView.jsx` Empty Dep Array useEffect Workaround

// **File:** `paati-ui/src/components/ChatView.jsx` (line 51)

// ```jsx
// }, []); // empty deps — runs only on first mount; guard handles re-mounts
// ```

// The `useEffect` reads `initialized`, `studentData`, `prediction`, `explanation`, `whatif` from props but has an empty dep array. The comment explicitly acknowledges the workaround. Functionally correct due to the `if (initialized) return` guard, but will get eslint exhaustive-deps warnings. The proper fix is a `useRef` guard instead.

// ---

// ### Issue 7 — LOW: `get_placement_level` Confidence Label Bug

// **File:** `main.py` (lines 245–252)

// ```python
// if probability >= 0.7:
//     return "LOW", "Very High Confidence"
// elif probability >= 0.5:
//     return "LOW", "High Confidence"   # Both 0.5–0.7 AND 0.7+ return "LOW"!
// elif probability >= 0.3:
//     return "MEDIUM", "Moderate Confidence"
// else:
//     return "HIGH", "High Confidence"  # "High Confidence" but HIGH risk?? copy-paste bug
// ```

// - 50% probability gets `risk_level = "LOW"` — misleading
// - The `else` branch (< 30% placement chance) returns `"High Confidence"` — copy-paste bug

// Pre-existing, not introduced by Jules.

// ---

// ## Summary Table

// | # | Severity | File | Issue | Jules fault? |
// |---|----------|------|-------|-------------|
// | 1 | CRITICAL | `main.py`, both `.bat` | `workers=4` breaks `user_progress_db` | Yes |
// | 2 | MEDIUM | `RiskCard.jsx` | No RAF/setTimeout cleanup | Not fixed |
// | 3 | MEDIUM | `Simulator.jsx` | No debounce cleanup + stale closure | Not fixed |
// | 4 | LOW-MED | `AssessmentForm.jsx` | Submit reads DOM not state | Partial fix |
// | 5 | LOW | `.env` | Real keys in repo root | Pre-existing |
// | 6 | LOW | `ChatView.jsx` | Empty dep array lint workaround | Pre-existing |
// | 7 | LOW | `main.py` | Confidence label copy-paste bug | Pre-existing |

// ---

// ## Plan 1 — Production-Level Fix

// **Goal:** Make this genuinely production-safe.

// ### Backend
// - **Migrate `user_progress_db` to Redis** using `aioredis` (async-native for FastAPI).
//   - Key format: `f"progress:{session_id}"`, TTL: 24h
//   - All reads/writes in `/chat/start`, `/chat/message`, `/chat/audio` become `await redis.hgetall(...)` / `await redis.hmset(...)`
// - **Revert to `workers=1`** until Redis is in place, then safely raise to 2–4
// - **Fix `get_placement_level`**: Correct the risk-level mapping (50% should be MEDIUM, not LOW) and fix the "High Confidence" label in the `else` branch
// - **Model hash check**: Add a hash of the pkl file to startup logs so pkl reload failures are detectable

// ### Frontend
// - **`RiskCard.jsx`**: Store RAF handle in a `useRef`. Return a cleanup that calls `cancelAnimationFrame(rafId)` + `clearTimeout(timer)` + sets `isMounted = false`
// - **`Simulator.jsx`**: Use `const timer = setTimeout(...)` + `return () => clearTimeout(timer)`. Add `formData` to the dep array
// - **`AssessmentForm.jsx`**: Remove all `e.target` DOM reads from `handleSubmit`. Read directly from `formData` state
// - **`ChatView.jsx`**: Replace the `initialized` lifted-state pattern with `const initializedRef = useRef(false)` — no empty dep array needed

// ### DevOps
// - `.env.example` already exists — good. Add a pre-commit hook scanning for `nvapi-` or `sk_` patterns to block accidental key commits
// - Add Redis service to `Dockerfile` / `docker-compose.yml`

// **Estimated effort: ~1 day of focused work**

// ---

// ## Plan 2 — 80/20 Fix (~30 min total)

// **Goal:** Stop the bleeding without Redis migration.

// ### Step 1 — Fix worker count (5 min, highest impact)

// Three one-line changes, `workers=4` → `workers=1`:

// - `main.py` L661
// - `paati-ui/start.bat` L20
// - `start_legacy.bat` L98 and L122

// This alone eliminates the gamification data-loss bug.

// ### Step 2 — Fix `RiskCard.jsx` cleanup (10 min)

// Gemini Code Assist gave you the exact fix in the PR. Apply it: add `isMounted` flag, store RAF in a `let rafId`, return `() => { isMounted = false; cancelAnimationFrame(rafId); clearTimeout(timer); }`.

// ### Step 3 — Fix `Simulator.jsx` cleanup (5 min)

// Move timeout to `const timer = setTimeout(...)` inside the effect. Return `() => clearTimeout(timer)`. Add `formData` to the dep array. Exact fix given in the PR.

// ### Step 4 — Fix `AssessmentForm.jsx` submit (10 min)

// Replace:
// ```js
// const fd = e.target;
// Gender: fd.gender.value,
// Age: parseInt(fd.age.value),
// ```
// With:
// ```js
// // read from React state, not the DOM
// Gender: formData.gender,
// Age: parseInt(formData.age),
// Stream: formData.stream,
// Internships: parseInt(formData.internships) || 0,
// CGPA: parseFloat(formData.cgpa) || 0,
// Hostel: formData.hostel ? 1 : 0,
// HistoryOfBacklogs: formData.backlogs ? 1 : 0,
// skills: selectedSkills,
// desired_role: formData.desired_role || null,
// resume_text: resumeText,
// ```

// ### What you skip in 80/20
// - Redis migration (single worker is fine at hackathon/demo scale)
// - `ChatView.jsx` dep array refactor (functional, just a lint warning)
// - `get_placement_level` confidence label (cosmetic, doesn't break anything)
// - Pre-commit key scanner (nice-to-have)