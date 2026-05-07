/**
 * CardioSense+ Frontend Application
 * AI-Powered Stroke Risk Assessment Interface
 * 
 * Connects to FastAPI backend for predictions and SHAP explanations
 */

// API Configuration 
// Smart API Configuration: Use port 8000 locally, but use the current origin for remote (HF/Production)
const API_BASE_URL = (window.location.hostname === "127.0.0.1" || window.location.hostname === "localhost")
    ? window.location.protocol + "//" + window.location.hostname + ":8000"
    : window.location.origin;

// DOM Elements
const elements = {
    loadingOverlay: document.getElementById('loadingOverlay'),
    patientForm: document.getElementById('patientForm'),
    submitBtn: document.getElementById('submitBtn'),
    resultsSection: document.getElementById('resultsSection'),
    assessmentForm: document.getElementById('assessmentForm'),
    backBtn: document.getElementById('backBtn'),
    riskCard: document.getElementById('riskCard'),
    riskPercentage: document.getElementById('riskPercentage'),
    riskLevel: document.getElementById('riskLevel'),
    riskConfidence: document.getElementById('riskConfidence'),
    progressRing: document.getElementById('progressRing'),
    meterPointer: document.getElementById('meterPointer'),
    factorsContainer: document.getElementById('factorsContainer'),
    recommendationsGrid: document.getElementById('recommendationsGrid'),
    whatifSection: document.getElementById('whatifSection'),
    whatifLoading: document.getElementById('whatifLoading'),
    whatifScenariosGrid: document.getElementById('whatifScenariosGrid'),
    whatifCombinedCard: document.getElementById('whatifCombinedCard'),
    combinedOriginalRisk: document.getElementById('combinedOriginalRisk'),
    combinedModifiedRisk: document.getElementById('combinedModifiedRisk'),
    combinedDelta: document.getElementById('combinedDelta')
};

// State
let currentPrediction = null;
let currentExplanation = null;
let currentWhatIf = null;
let currentFormData = null;
let availableSkills = [];
let selectedSkills = [];

/**
 * Initialize the application
 */
function init() {
    setupEventListeners();
    checkAPIHealth();
    fetchOptions();
}

/**
 * Fetch available streams and skills from API
 */
async function fetchOptions() {
    try {
        const response = await fetch(`${API_BASE_URL}/options`);
        const data = await response.json();

        const streamSelect = document.getElementById('stream');
        streamSelect.innerHTML = '<option value="">Select stream</option>';
        data.streams.forEach(stream => {
            const opt = document.createElement('option');
            opt.value = stream;
            opt.textContent = stream;
            streamSelect.appendChild(opt);
        });

        const roleSelect = document.getElementById('desired_role');
        roleSelect.innerHTML = '<option value="">Select Target Job Role</option>';
        if (data.jobs && data.jobs.length > 0) {
            data.jobs.forEach(job => {
                const opt = document.createElement('option');
                opt.value = job;
                opt.textContent = job;
                roleSelect.appendChild(opt);
            });
        }

        if (data.skills) {
            availableSkills = data.skills;
            setupSkillsAutocomplete();
        }
    } catch (e) {
        console.error('Error fetching options:', e);
        const streamSelect = document.getElementById('stream');
        if (streamSelect) streamSelect.innerHTML = '<option value="">Error loading streams</option>';
        const roleSelect = document.getElementById('desired_role');
        if (roleSelect) roleSelect.innerHTML = '<option value="">Error loading roles</option>';
    }
}

/**
 * Setup Skills Autocomplete Logic
 */
function setupSkillsAutocomplete() {
    const wrapper = document.getElementById('skillsWrapper');
    const input = document.getElementById('skillsInput');
    const hiddenInput = document.getElementById('skills');
    const tagsContainer = document.getElementById('skillsTags');
    const dropdown = document.getElementById('skillsDropdown');

    function renderTags() {
        tagsContainer.innerHTML = '';
        selectedSkills.forEach((skill, index) => {
            const tag = document.createElement('span');
            tag.style.cssText = 'background: #047857; color: white; padding: 2px 8px; border-radius: 4px; font-size: 0.85rem; display: flex; align-items: center; gap: 4px;';
            tag.innerHTML = `${skill} <span style="cursor:pointer;font-weight:bold;" onclick="removeSkill(${index})">&times;</span>`;
            tagsContainer.appendChild(tag);
        });
        hiddenInput.value = selectedSkills.join(',');
    }

    window.removeSkill = function (index) {
        selectedSkills.splice(index, 1);
        renderTags();
    };

    function showDropdown(query = '') {
        const filtered = availableSkills.filter(s => s.toLowerCase().includes(query.toLowerCase()) && !selectedSkills.includes(s));

        dropdown.innerHTML = '';
        if (filtered.length === 0) {
            dropdown.style.display = 'none';
            return;
        }

        filtered.slice(0, 50).forEach(skill => {
            const div = document.createElement('div');
            div.style.cssText = 'padding: 8px 12px; cursor: pointer; color: var(--text-color); border-bottom: 1px solid var(--gray-200);';
            div.textContent = skill;
            div.onmouseover = () => div.style.background = 'var(--gray-100)';
            div.onmouseout = () => div.style.background = 'transparent';
            div.onclick = () => {
                selectedSkills.push(skill);
                renderTags();
                input.value = '';
                dropdown.style.display = 'none';
                input.focus();
            };
            dropdown.appendChild(div);
        });
        dropdown.style.display = 'block';
    }

    input.addEventListener('focus', () => showDropdown(input.value));
    input.addEventListener('input', (e) => showDropdown(e.target.value));

    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
            e.preventDefault();
            const val = input.value.trim();
            if (val && !selectedSkills.includes(val)) {
                const actualSkill = availableSkills.find(s => s.toLowerCase() === val.toLowerCase());
                if (actualSkill) {
                    selectedSkills.push(actualSkill);
                    renderTags();
                    input.value = '';
                    dropdown.style.display = 'none';
                }
            }
        } else if (e.key === 'Backspace' && input.value === '' && selectedSkills.length > 0) {
            selectedSkills.pop();
            renderTags();
        }
    });

    document.addEventListener('click', (e) => {
        if (!wrapper.contains(e.target)) {
            dropdown.style.display = 'none';
            wrapper.style.borderColor = 'transparent';
        }
    });

    wrapper.addEventListener('click', () => {
        input.focus();
        wrapper.style.borderColor = 'var(--black)';
    });
}

/**
 * Setup event listeners
 */
function setupEventListeners() {
    // Form submission
    elements.patientForm.addEventListener('submit', handleFormSubmit);

    // Back button
    elements.backBtn.addEventListener('click', showForm);

    // Input animations
    const inputs = document.querySelectorAll('input, select');
    inputs.forEach(input => {
        input.addEventListener('focus', () => {
            input.closest('.input-group')?.classList.add('focused');
        });
        input.addEventListener('blur', () => {
            input.closest('.input-group')?.classList.remove('focused');
        });
    });

    // Resume Upload Handler
    const resumeUpload = document.getElementById('resumeUpload');
    const resumeStatus = document.getElementById('resumeStatus');

    if (resumeUpload) {
        resumeUpload.addEventListener('change', async (e) => {
            const file = e.target.files[0];
            if (!file) return;

            resumeStatus.textContent = "⏳ Paati is reading your resume...";
            resumeStatus.style.color = "#FF9800";

            const fd = new FormData();
            fd.append('file', file);

            try {
                const res = await fetch(`${API_BASE_URL}/upload/resume`, { method: 'POST', body: fd });
                const data = await res.json();
                if (data.status === "success") {
                    document.getElementById('resumeTextHidden').value = data.resume_text;
                    resumeStatus.textContent = "✅ Resume loaded! Paati knows your skills now.";
                    resumeStatus.style.color = "#4CAF50";
                } else {
                    throw new Error("Parsing failed");
                }
            } catch (error) {
                resumeStatus.textContent = "❌ Failed to read resume. Try a different PDF.";
                resumeStatus.style.color = "#E85D4C";
            }
        });
    }
}

/**
 * Check API health status
 */
async function checkAPIHealth() {
    try {
        const response = await fetch(`${API_BASE_URL}/health`);
        const data = await response.json();

        if (!data.model_loaded) {
            showNotification('Model not loaded. Please run train_model.py first.', 'warning');
        }
    } catch (error) {
        showNotification('Cannot connect to API. Make sure the server is running.', 'error');
    }
}

/**
 * Handle form submission
 */
async function handleFormSubmit(e) {
    e.preventDefault();

    // Collect form data
    const formData = collectFormData();

    if (!validateFormData(formData)) {
        return;
    }

    // Show loading
    showLoading(true);

    try {
        // Make parallel API calls for prediction and explanation
        const [predictionResult, explanationResult] = await Promise.all([
            fetchPrediction(formData),
            fetchExplanation(formData)
        ]);

        currentPrediction = predictionResult;
        currentExplanation = explanationResult;
        currentFormData = formData;

        // Display results
        displayResults(predictionResult, explanationResult);

        // Fetch What-If analysis in background (non-blocking)
        fetchWhatIfAnalysis(formData);

    } catch (error) {
        console.error('API Error:', error);
        showNotification('Failed to get prediction. Please try again.', 'error');
        showLoading(false);
    }
}

/**
 * Collect form data
 */
function collectFormData() {
    return {
        Gender: document.getElementById('gender').value,
        Age: parseInt(document.getElementById('age').value),
        Stream: document.getElementById('stream').value,
        Internships: parseInt(document.getElementById('internships').value) || 0,
        CGPA: parseFloat(document.getElementById('cgpa').value) || 0,
        Hostel: document.getElementById('hostel').checked ? 1 : 0,
        HistoryOfBacklogs: document.getElementById('backlogs').checked ? 1 : 0,
        skills: document.getElementById('skills').value.split(',').map(s => s.trim()).filter(s => s),
        desired_role: document.getElementById('desired_role').value || null,
        resume_text: document.getElementById('resumeTextHidden')?.value || ""
    };
}

/**
 * Validate form data
 */
function validateFormData(data) {
    const requiredFields = ['Gender', 'Age', 'Stream', 'Internships', 'CGPA'];

    for (const field of requiredFields) {
        if (!data[field] && data[field] !== 0) {
            showNotification(`Please fill in all required fields.`, 'warning');
            return false;
        }
    }

    if (data.Age < 15 || data.Age > 50) {
        showNotification('Please enter a valid age (15-50).', 'warning');
        return false;
    }

    if (data.CGPA < 0 || data.CGPA > 10) {
        showNotification('Please enter a valid CGPA (0-10).', 'warning');
        return false;
    }

    return true;
}

/**
 * Fetch prediction from API
 */
async function fetchPrediction(data) {
    const response = await fetch(`${API_BASE_URL}/predict`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json'
        },
        body: JSON.stringify(data)
    });

    if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
    }

    return response.json();
}

/**
 * Fetch explanation from API
 */
async function fetchExplanation(data) {
    const response = await fetch(`${API_BASE_URL}/explain`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json'
        },
        body: JSON.stringify(data)
    });

    if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
    }

    return response.json();
}

/**
 * Display results
 */
function displayResults(prediction, explanation) {
    // Hide loading after a short delay for smooth transition
    setTimeout(() => {
        showLoading(false);

        // Switch views
        elements.assessmentForm.classList.add('hidden');
        elements.resultsSection.classList.remove('hidden');

        // Scroll to top smoothly
        window.scrollTo({ top: 0, behavior: 'smooth' });

        // Animate results
        setTimeout(() => {
            animateRiskScore(prediction);
            displayFactors(explanation.top_contributing_factors);
            displayRecommendations(prediction.risk_level);

            // Render Graph Image if present
            if (prediction.graph_data) {
                document.getElementById('graphSection').style.display = 'block';
                document.getElementById('graphImage').src = prediction.graph_data;
            } else {
                document.getElementById('graphSection').style.display = 'none';
            }

            // Initialize Risk Simulator
            initSimulator();

            // Show chat widget after results are displayed
            showChatWidget();
        }, 300);

    }, 1000);
}

/**
 * Animate risk score display
 */
function animateRiskScore(prediction) {
    const percentage = prediction.probability_percentage;
    const riskLevel = prediction.risk_level;

    // Set risk card class
    elements.riskCard.className = 'risk-card ' + riskLevel.toLowerCase();

    // Update confidence
    elements.riskConfidence.textContent = prediction.confidence;

    // Animate percentage counter
    animateCounter(elements.riskPercentage, 0, percentage, 1500);

    // Animate progress ring
    const circumference = 2 * Math.PI * 54; // r = 54
    const offset = circumference - (percentage / 100) * circumference;

    setTimeout(() => {
        elements.progressRing.style.strokeDashoffset = offset;
    }, 100);

    // Animate risk level text
    setTimeout(() => {
        elements.riskLevel.textContent = riskLevel;
    }, 500);

    // Animate meter pointer
    setTimeout(() => {
        elements.meterPointer.style.left = `${percentage}%`;
    }, 100);
}

/**
 * Animate counter from start to end
 */
function animateCounter(element, start, end, duration) {
    const startTime = performance.now();
    const diff = end - start;

    function update(currentTime) {
        const elapsed = currentTime - startTime;
        const progress = Math.min(elapsed / duration, 1);

        // Easing function (ease-out)
        const easeOut = 1 - Math.pow(1 - progress, 3);

        const current = start + diff * easeOut;
        element.textContent = current.toFixed(1);

        if (progress < 1) {
            requestAnimationFrame(update);
        }
    }

    requestAnimationFrame(update);
}

/**
 * Display factor cards
 */
function displayFactors(factors) {
    elements.factorsContainer.innerHTML = '';

    // Find max impact for normalization
    const maxImpact = Math.max(...factors.map(f => Math.abs(f.impact)));

    factors.forEach((factor, index) => {
        const card = createFactorCard(factor, maxImpact);
        elements.factorsContainer.appendChild(card);

        // Trigger animation
        setTimeout(() => {
            card.classList.add('animate');
        }, 50);
    });
}

/**
 * Create a factor card element
 */
function createFactorCard(factor, maxImpact) {
    const card = document.createElement('div');
    card.className = 'factor-card';

    const isPositive = factor.impact > 0;
    const normalizedImpact = (Math.abs(factor.impact) / maxImpact) * 100;
    const featureName = formatFeatureName(factor.feature);

    card.innerHTML = `
        <div class="factor-header">
            <span class="factor-name">${featureName}</span>
            <span class="factor-direction ${isPositive ? 'increases' : 'reduces'}">
                ${factor.direction}
            </span>
        </div>
        <p class="factor-interpretation">${factor.interpretation}</p>
        <div class="factor-bar">
            <div class="factor-bar-fill ${isPositive ? 'positive' : 'negative'}" 
                 style="width: 0%"
                 data-width="${normalizedImpact}%"></div>
        </div>
    `;

    // Animate bar after card is added
    setTimeout(() => {
        const bar = card.querySelector('.factor-bar-fill');
        bar.style.width = `${normalizedImpact}%`;
    }, 300);

    return card;
}

/**
 * Format feature name for display
 */
function formatFeatureName(name) {
    // Handle common feature name patterns
    const nameMap = {
        'Age': 'Student Age',
        'Gender_Female': 'Gender: Female',
        'Gender_Male': 'Gender: Male',
        'Internships': 'Number of Internships',
        'CGPA': 'Cumulative GPA',
        'Hostel': 'Hostel Accommodation',
        'HistoryOfBacklogs': 'Academic Backlogs'
    };

    // For streams, handle dynamically
    if (name.startsWith('Stream_')) {
        return 'Stream: ' + name.replace('Stream_', '').replace(/_/g, ' ');
    }

    return nameMap[name] || name.replace(/_/g, ' ').replace(/\b\w/g, l => l.toUpperCase());
}

/**
 * Display recommendations based on risk level
 */
function displayRecommendations(riskLevel) {
    const recommendations = getRecommendations(riskLevel);
    elements.recommendationsGrid.innerHTML = '';

    recommendations.forEach(rec => {
        const card = document.createElement('div');
        // If it's the Career Path recommendation, add a special class
        card.className = `recommendation-card ${rec.title === 'Career Path' ? 'highlight-card' : ''}`;

        card.innerHTML = `
            <div class="recommendation-icon">${rec.icon}</div>
            <h4 class="recommendation-title">${rec.title}</h4>
            <p class="recommendation-text">${rec.text}</p>
        `;

        elements.recommendationsGrid.appendChild(card);
    });
}

/**
 * Get recommendations based on risk level
 */
function getRecommendations(riskLevel) {
    // Added recommended job functionality here
    const jobRec = currentPrediction?.recommended_job ? {
        icon: '🎯',
        title: 'Career Path',
        text: `Based on your specific skill profile, your ideal career path is <strong style="color:#ffd700; font-size:1.15em; text-decoration:underline;">${currentPrediction.recommended_job}</strong>.`
    } : null;

    // Added missing skills recommendation
    const skillsRec = currentPrediction?.missing_skills && currentPrediction.missing_skills.length > 0 ? {
        icon: '🛠️',
        title: 'Skill Gaps',
        // Show full list of missing skills rather than truncating
        text: `Focus on mastering: ${currentPrediction.missing_skills.join(', ')}`
    } : null;

    let base = [
        {
            icon: '📚',
            title: 'Continuous Learning',
            text: 'Keep building projects to stand out to recruiters.'
        }
    ];
    if (jobRec) base.unshift(jobRec);
    if (skillsRec) base.push(skillsRec);

    if (riskLevel === 'HIGH') {
        return [
            {
                icon: '🛠️',
                title: 'Skill Development',
                text: 'Focus heavily on matching industry required skills.'
            },
            {
                icon: '📈',
                title: 'Improve Academics',
                text: 'Work on your CGPA and try to secure internships.'
            },
            ...base
        ];
    } else if (riskLevel === 'MEDIUM') {
        return [
            {
                icon: '🤝',
                title: 'Networking',
                text: 'Connect with alumni and professionals in your target field.'
            },
            ...base
        ];
    } else {
        return [
            {
                icon: '🚀',
                title: 'Prepare for Interviews',
                text: 'You are in a great position. Start practicing mock interviews!'
            },
            ...base
        ];
    }
}

/**
 * Show/hide loading overlay
 */
function showLoading(show) {
    if (show) {
        elements.loadingOverlay.classList.add('active');
    } else {
        elements.loadingOverlay.classList.remove('active');
    }
}

/**
 * Show form and hide results
 */
function showForm() {
    elements.resultsSection.classList.add('hidden');
    elements.assessmentForm.classList.remove('hidden');

    // Reset form
    elements.patientForm.reset();

    // Reset What-If state
    resetWhatIf();

    // Hide chat widget
    hideChatWidget();

    // Scroll to form
    window.scrollTo({ top: 0, behavior: 'smooth' });
}

/**
 * Show notification toast
 */
function showNotification(message, type = 'info') {
    // Remove existing notifications
    const existing = document.querySelector('.notification');
    if (existing) {
        existing.remove();
    }

    // Create notification element
    const notification = document.createElement('div');
    notification.className = `notification notification-${type}`;
    notification.innerHTML = `
        <span class="notification-message">${message}</span>
        <button class="notification-close">&times;</button>
    `;

    // Add styles dynamically
    notification.style.cssText = `
        position: fixed;
        bottom: 20px;
        left: 50%;
        transform: translateX(-50%);
        background: ${type === 'error' ? '#E85D4C' : type === 'warning' ? '#FF9800' : '#2D9596'};
        color: white;
        padding: 12px 24px;
        border-radius: 8px;
        display: flex;
        align-items: center;
        gap: 12px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        z-index: 10000;
        max-width: 90%;
        animation: slideUp 0.3s ease;
    `;

    // Add animation keyframes
    const style = document.createElement('style');
    style.textContent = `
        @keyframes slideUp {
            from {
                opacity: 0;
                transform: translateX(-50%) translateY(20px);
            }
            to {
                opacity: 1;
                transform: translateX(-50%) translateY(0);
            }
        }
    `;
    document.head.appendChild(style);

    document.body.appendChild(notification);

    // Close button handler
    notification.querySelector('.notification-close').addEventListener('click', () => {
        notification.remove();
    });

    // Auto-dismiss after 5 seconds
    setTimeout(() => {
        notification.style.opacity = '0';
        notification.style.transform = 'translateX(-50%) translateY(20px)';
        notification.style.transition = 'all 0.3s ease';
        setTimeout(() => notification.remove(), 300);
    }, 5000);
}

/**
 * Demo mode - fill form with sample data
 */
function fillDemoData() {
    document.getElementById('gender').value = 'Female';
    document.getElementById('age').value = '21';
    document.getElementById('stream').value = 'Information Technology';
    document.getElementById('internships').value = '1';
    document.getElementById('cgpa').value = '7.5';
    document.getElementById('hostel').checked = true;
    document.getElementById('backlogs').checked = true; // Highlighting improvement area

    // Setup skills tags visually and hidden
    selectedSkills = ['Python', 'SQL', 'Git'];
    const hiddenInput = document.getElementById('skills');
    const tagsContainer = document.getElementById('skillsTags');

    if (tagsContainer && hiddenInput) {
        tagsContainer.innerHTML = '';
        selectedSkills.forEach((skill, index) => {
            const tag = document.createElement('span');
            tag.style.cssText = 'background: #047857; color: white; padding: 2px 8px; border-radius: 4px; font-size: 0.85rem; display: flex; align-items: center; gap: 4px;';
            tag.innerHTML = `${skill} <span style="cursor:pointer;font-weight:bold;" onclick="removeSkill(${index})">&times;</span>`;
            tagsContainer.appendChild(tag);
        });
        hiddenInput.value = selectedSkills.join(',');
    }

    // Attempt to set desired role if options exist
    const roleOpt = Array.from(document.getElementById('desired_role').options).find(opt => opt.value === 'Data Analyst');
    if (roleOpt) {
        document.getElementById('desired_role').value = 'Data Analyst';
    }
}

/**
 * =============================================
 * WHAT-IF SCENARIO ANALYSIS
 * =============================================
 */

/**
 * Fetch What-If analysis from API
 */
async function fetchWhatIfAnalysis(formData) {
    // Show the What-If section with loading state
    elements.whatifSection.style.display = 'block';
    elements.whatifLoading.style.display = 'flex';
    elements.whatifScenariosGrid.innerHTML = '';
    elements.whatifCombinedCard.style.display = 'none';

    try {
        const response = await fetch(`${API_BASE_URL}/whatif`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(formData)
        });

        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }

        const whatifData = await response.json();
        currentWhatIf = whatifData;

        // Hide loading, display results
        elements.whatifLoading.style.display = 'none';
        displayWhatIfAnalysis(whatifData);

    } catch (error) {
        console.error('What-If API Error:', error);
        elements.whatifLoading.style.display = 'none';
        elements.whatifScenariosGrid.innerHTML = `
            <div class="whatif-error">
                <p>⚠️ Could not generate What-If scenarios. The analysis will still work without this feature.</p>
            </div>
        `;
    }
}

/**
 * Display What-If analysis results
 */
function displayWhatIfAnalysis(data) {
    if (!data.scenarios || data.scenarios.length === 0) {
        elements.whatifScenariosGrid.innerHTML = `
            <div class="whatif-empty">
                <p>✅ Your current health parameters are already in healthy ranges. No significant changes to suggest!</p>
            </div>
        `;
        return;
    }

    // Render scenario cards with staggered animation
    data.scenarios.forEach((scenario, index) => {
        const card = createScenarioCard(scenario);
        elements.whatifScenariosGrid.appendChild(card);

        // Staggered fade-in animation
        setTimeout(() => {
            card.classList.add('animate');
        }, 100 + index * 150);
    });

    // Show combined outcome if available
    if (data.combined_risk !== null && data.combined_risk !== undefined && data.scenarios.length > 1) {
        setTimeout(() => {
            displayCombinedOutcome(data);
        }, 100 + data.scenarios.length * 150 + 200);
    }
}

/**
 * Create a single scenario card
 */
function createScenarioCard(scenario) {
    const card = document.createElement('div');
    card.className = 'whatif-card';

    const isReduction = scenario.risk_delta > 0;
    const deltaAbs = Math.abs(scenario.risk_delta).toFixed(1);
    const reductionPct = Math.abs(scenario.risk_reduction_percent).toFixed(1);

    // Determine the bar width (scale delta to percentage of original risk for visualization)
    const barWidth = Math.min(Math.abs(scenario.risk_reduction_percent), 100);

    card.innerHTML = `
        <div class="whatif-card-icon">${scenario.icon}</div>
        <div class="whatif-card-content">
            <h4 class="whatif-card-title">${scenario.title}</h4>
            <p class="whatif-card-desc">${scenario.description}</p>
            
            <div class="whatif-card-comparison">
                <div class="whatif-risk-original">
                    <span class="whatif-risk-label">Current</span>
                    <span class="whatif-risk-val">${scenario.original_risk.toFixed(1)}%</span>
                </div>
                <div class="whatif-arrow-container">
                    <svg class="whatif-arrow ${isReduction ? 'reduction' : 'increase'}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M5 12h14M12 5l7 7-7 7" />
                    </svg>
                </div>
                <div class="whatif-risk-modified">
                    <span class="whatif-risk-label">Modified</span>
                    <span class="whatif-risk-val ${isReduction ? 'improved' : 'worsened'}">${scenario.modified_risk.toFixed(1)}%</span>
                </div>
            </div>
            
            <div class="whatif-delta-bar">
                <div class="whatif-delta-fill ${isReduction ? 'positive' : 'negative'}" 
                     style="width: 0%" data-width="${barWidth}%"></div>
            </div>
            
            <div class="whatif-delta-text ${isReduction ? 'positive' : 'negative'}">
                ${isReduction ? '↓' : '↑'} ${deltaAbs}% risk ${isReduction ? 'reduction' : 'increase'}
                <span class="whatif-delta-pct">(${reductionPct}% ${isReduction ? 'improvement' : 'change'})</span>
            </div>
        </div>
    `;

    // Animate the delta bar after card is rendered
    setTimeout(() => {
        const bar = card.querySelector('.whatif-delta-fill');
        if (bar) {
            bar.style.width = `${barWidth}%`;
        }
    }, 500);

    return card;
}

/**
 * Display combined best-case outcome card
 */
function displayCombinedOutcome(data) {
    elements.whatifCombinedCard.style.display = 'block';

    const originalRisk = data.original_risk;
    const combinedRisk = data.combined_risk;
    const totalDelta = originalRisk - combinedRisk;
    const totalReductionPct = originalRisk > 0 ? (totalDelta / originalRisk * 100) : 0;

    elements.combinedOriginalRisk.textContent = `${originalRisk.toFixed(1)}%`;
    elements.combinedModifiedRisk.textContent = `${combinedRisk.toFixed(1)}%`;

    // Color the modified risk value
    if (combinedRisk < originalRisk) {
        elements.combinedModifiedRisk.classList.add('improved');
    }

    // Delta summary
    const isReduction = totalDelta > 0;
    elements.combinedDelta.innerHTML = `
        <div class="combined-delta-badge ${isReduction ? 'positive' : 'negative'}">
            ${isReduction ? '↓' : '↑'} ${Math.abs(totalDelta).toFixed(1)}% total risk ${isReduction ? 'reduction' : 'increase'}
            <span>(${Math.abs(totalReductionPct).toFixed(1)}% overall ${isReduction ? 'improvement' : 'change'})</span>
        </div>
        <p class="combined-risk-level">Risk Level: <strong>${data.original_risk_level}</strong> → <strong class="${(data.combined_risk_level || '').toLowerCase()}">${data.combined_risk_level || 'N/A'}</strong></p>
    `;

    // Animate in
    setTimeout(() => {
        elements.whatifCombinedCard.classList.add('animate');
    }, 100);
}

/**
 * Reset What-If section state
 */
function resetWhatIf() {
    currentWhatIf = null;
    elements.whatifSection.style.display = 'none';
    elements.whatifScenariosGrid.innerHTML = '';
    elements.whatifCombinedCard.style.display = 'none';
    elements.whatifCombinedCard.classList.remove('animate');
    elements.combinedModifiedRisk.classList.remove('improved');
}

/**
 * =============================================
 * INTERACTIVE RISK SIMULATOR
 * =============================================
 */

let simTimeout = null;

function initSimulator() {
    if (!currentFormData || !currentPrediction) return;

    // Show the simulator section
    const simCard = document.getElementById('interactiveSimulator');
    if (simCard) simCard.classList.remove('hidden');

    // Populate baseline stats
    const baselineRiskStr = currentPrediction.probability_percentage.toFixed(1) + '%';
    document.getElementById('simBaselineStat').textContent = baselineRiskStr;

    // Initial UI update with baseline data
    updateSimulatorUI(currentPrediction, currentPrediction, currentFormData);

    // Setup Event Listeners
    setupSimControl('Age', 'Age', 15, 50);
    setupSimControl('Intern', 'Internships', 0, 10);
    setupSimControl('CGPA', 'CGPA', 0, 10);

    // Toggle Setup
    const toggle = document.getElementById('simBacklogToggle');
    toggle.checked = (currentFormData.HistoryOfBacklogs === 1);
    toggle.onchange = () => { triggerSimulate(); };
}

function setupSimControl(idPrefix, fieldName, min, max) {
    const slider = document.getElementById(`sim${idPrefix}Slider`);
    const num = document.getElementById(`sim${idPrefix}Num`);

    if (slider && num && currentFormData) {
        let val = Number(currentFormData[fieldName]) || min;
        if (val < min) val = min;
        if (val > max) val = max;

        slider.value = val;
        num.value = val;

        slider.oninput = (e) => { num.value = e.target.value; triggerSimulate(); };
        num.oninput = (e) => {
            let v = Number(e.target.value);
            if (v >= min && v <= max) { slider.value = v; triggerSimulate(); }
        };
    }
}

function triggerSimulate() {
    clearTimeout(simTimeout);
    simTimeout = setTimeout(simulateRisk, 300); // 300ms debounce
}

async function simulateRisk() {
    if (!currentFormData) return;

    // Create new student data object
    const simulatedData = { ...currentFormData };

    // Override with simulator values
    simulatedData.Age = parseInt(document.getElementById('simAgeNum').value);
    simulatedData.Internships = parseInt(document.getElementById('simInternNum').value);
    simulatedData.CGPA = parseFloat(document.getElementById('simCGPANum').value);

    const hasBacklog = document.getElementById('simBacklogToggle').checked;
    simulatedData.HistoryOfBacklogs = hasBacklog ? 1 : 0;

    try {
        const response = await fetchPrediction(simulatedData);
        updateSimulatorUI(currentPrediction, response, simulatedData);
    } catch (e) {
        console.error("Simulation failed", e);
    }
}

function updateSimulatorUI(baselinePred, targetPred, simData) {
    const risk = targetPred.probability_percentage;
    const levelStr = targetPred.risk_level.toUpperCase();

    // Text values
    document.getElementById('simRiskValue').textContent = risk.toFixed(1) + '%';
    document.getElementById('simRiskLabel').textContent = levelStr + ' RISK';
    document.getElementById('simTargetStat').textContent = risk.toFixed(1) + '%';

    // Delta
    const delta = (risk - baselinePred.probability_percentage).toFixed(1);
    const deltaEl = document.getElementById('simDeltaStat');
    deltaEl.textContent = (delta > 0 ? '+' : '') + delta + '%';
    deltaEl.className = 'sim-stat-val ' + (delta > 0 ? 'negative' : (delta < 0 ? 'positive' : ''));

    // Dynamic Arc length = 219.91
    const trackFilled = 219.91 * (1 - (risk / 100)); // risk mapped as 0-100%
    const simTrack = document.getElementById('simTrack');
    if (simTrack) {
        simTrack.style.strokeDashoffset = trackFilled;

        // Color
        let color = 'var(--risk-low)';
        if (targetPred.risk_level === 'MEDIUM') color = 'var(--risk-medium)';
        if (targetPred.risk_level === 'HIGH') color = 'var(--risk-high)';
        simTrack.style.stroke = color;
    }

    // Baseline Marker (Angle 0 is left, angle 180 is right)
    const baseRisk = baselinePred.probability_percentage;
    const baseAngle = 180 * (baseRisk / 100);
    const baseGroup = document.getElementById('simBaselineGroup');
    const baseTextGroup = document.getElementById('simBaselineTextGroup');
    if (baseGroup) baseGroup.style.transform = `rotate(${baseAngle}deg)`;
    if (baseTextGroup) baseTextGroup.style.transform = `translate(18px, 90px) rotate(${-baseAngle}deg)`;

    // Target Marker
    const targetAngle = 180 * (risk / 100);
    const targetGroup = document.getElementById('simTargetGroup');
    const targetTextGroup = document.getElementById('simTargetTextGroup');
    if (targetGroup) targetGroup.style.transform = `rotate(${targetAngle}deg)`;
    if (targetTextGroup) targetTextGroup.style.transform = `translate(18px, 110px) rotate(${-targetAngle}deg)`;
}


/**
 * =============================================
 * AI CHAT FUNCTIONALITY
 * =============================================
 */

let chatSessionId = null;
let chatIsOpen = false;
let chatInitialized = false;
let chatElems = {};

function updatePaatiScoreboard(points, level, kurals) {
    document.getElementById('userPoints').textContent = points;
    document.getElementById('userLevel').textContent = level;
    document.getElementById('userKurals').textContent = kurals;
}

let currentPaatiAudio = null;

function playAudioBase64(base64String) {
    if (!base64String) return;
    if (currentPaatiAudio) {
        currentPaatiAudio.pause();
        currentPaatiAudio.currentTime = 0;
    }
    currentPaatiAudio = new Audio("data:audio/wav;base64," + base64String);
    currentPaatiAudio.play().catch(e => console.error("Audio play failed:", e));
}

function toggleFullscreen() {
    chatElems.panel.classList.toggle('fullscreen');
}

function initChatElements() {
    chatElems = {
        widget: document.getElementById('chatWidget'),
        toggle: document.getElementById('chatToggle'),
        panel: document.getElementById('chatPanel'),
        minimize: document.getElementById('chatMinimize'),
        expand: document.getElementById('chatExpand'),
        messages: document.getElementById('chatMessages'),
        input: document.getElementById('chatInput'),
        send: document.getElementById('chatSend'),
        status: document.getElementById('chatStatus'),
        iconOpen: document.querySelector('.chat-icon-open'),
        iconClose: document.querySelector('.chat-icon-close'),
        mic: document.getElementById('chatMic'),
    };

    chatElems.toggle.addEventListener('click', toggleChat);
    chatElems.minimize.addEventListener('click', toggleChat);
    if (chatElems.expand) chatElems.expand.addEventListener('click', toggleFullscreen);
    chatElems.send.addEventListener('click', sendChatMessage);
    if (chatElems.mic) {
        chatElems.mic.addEventListener('click', toggleRecording);
    }
    chatElems.input.addEventListener('keypress', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            sendChatMessage();
        }
    });
    chatElems.input.addEventListener('input', () => {
        chatElems.send.disabled = !chatElems.input.value.trim();
    });
}

function showChatWidget() {
    if (!chatElems.widget) initChatElements();
    chatElems.widget.style.display = 'block';
}

function hideChatWidget() {
    if (!chatElems.widget) return;
    chatElems.widget.style.display = 'none';
    chatElems.panel.style.display = 'none';
    chatIsOpen = false;
    chatInitialized = false;
    chatSessionId = null;
    if (chatElems.messages) chatElems.messages.innerHTML = '';
    if (chatElems.iconOpen) chatElems.iconOpen.style.display = 'block';
    if (chatElems.iconClose) chatElems.iconClose.style.display = 'none';
}

function toggleChat() {
    chatIsOpen = !chatIsOpen;
    chatElems.panel.style.display = chatIsOpen ? 'flex' : 'none';
    chatElems.iconOpen.style.display = chatIsOpen ? 'none' : 'block';
    chatElems.iconClose.style.display = chatIsOpen ? 'block' : 'none';

    if (chatIsOpen && !chatInitialized) {
        initializeChat();
    }
    if (chatIsOpen) {
        chatElems.input.focus();
    }
}

async function initializeChat() {
    chatInitialized = true;
    addTypingIndicator();
    setChatStatus('Connecting...');

    const formData = collectFormData();

    try {
        const response = await fetch(`${API_BASE_URL}/chat/start`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                student_data: formData,
                prediction: currentPrediction,
                explanation: currentExplanation,
                whatif: currentWhatIf || {}
            })
        });

        if (!response.ok) throw new Error(`HTTP ${response.status}`);

        const data = await response.json();
        chatSessionId = data.session_id;

        removeTypingIndicator();
        addChatMessage('ai', data.message);

        // NEW: Play the greeting audio immediately!
        if (data.audio_base64) {
            playAudioBase64(data.audio_base64);
        }

        if (data.points_update) {
            updatePaatiScoreboard(data.new_points, data.new_level, data.new_kurals);
        }

        setChatStatus('Online');

    } catch (error) {
        console.error('Chat init error:', error);
        removeTypingIndicator();
        addChatMessage('system',
            '⚠️ Could not connect to Paati AI. Make sure NVIDIA_API_KEY is set and agno is installed.'
        );
        setChatStatus('Offline');
    }
}

async function sendChatMessage() {
    const message = chatElems.input.value.trim();
    if (!message || !chatSessionId) return;

    addChatMessage('user', message);
    chatElems.input.value = '';
    chatElems.send.disabled = true;

    addTypingIndicator();
    setChatStatus('Thinking...');

    try {
        const response = await fetch(`${API_BASE_URL}/chat/message`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                session_id: chatSessionId,
                message: message
            })
        });

        if (!response.ok) throw new Error(`HTTP ${response.status}`);

        const data = await response.json();
        removeTypingIndicator();
        addChatMessage('ai', data.response);

        // PLAY TTS AUDIO IF RECEIVED
        if (data.audio_base64) playAudioBase64(data.audio_base64);

        if (data.points_update) {
            updatePaatiScoreboard(data.new_points, data.new_level, data.new_kurals);
        }

        setChatStatus('Online');

    } catch (error) {
        console.error('Chat error:', error);
        removeTypingIndicator();
        addChatMessage('system', '⚠️ Failed to get a response. Please try again.');
        setChatStatus('Online');
    }
}

function addChatMessage(type, text) {
    const msgDiv = document.createElement('div');
    msgDiv.className = `chat-msg chat-msg-${type}`;

    if (type === 'ai' && text.includes('"step_type"') && text.includes('"steps"')) {
        try {
            let jsonStr = text;
            const match = text.match(/```json\s*([\s\S]*?)\s*```/) || text.match(/```\s*([\s\S]*?)\s*```/) || text.match(/(\{[\s\S]*"steps"[\s\S]*\})/);
            if (match) {
                jsonStr = match[1];
            }
            const gameData = JSON.parse(jsonStr);

            if (gameData.steps && gameData.title) {
                const bubble = document.createElement('div');
                bubble.className = 'chat-bubble mini-game-container';
                bubble.style.cssText = "background:var(--cream-dark); border:2px solid var(--accent-gold); border-radius:15px; padding:15px; width:100%; max-width:100%; box-shadow:var(--shadow-md);";

                let currentStep = 0;

                function renderStep() {
                    if (currentStep >= gameData.steps.length) {
                        bubble.innerHTML = `
                            <h4 style="margin:0 0 10px 0; color:var(--accent-teal);">🏆 Game Complete!</h4>
                            <p style="margin-bottom:15px;">You earned the <strong>${gameData.final_reward_badge}</strong> badge!</p>
                            <button class="claim-reward-btn" style="padding:10px; background:var(--black); color:var(--accent-gold); border:none; border-radius:8px; cursor:pointer; width:100%; font-weight:bold;">Claim Reward & Send</button>
                        `;
                        bubble.querySelector('.claim-reward-btn').onclick = () => {
                            const totalSteps = gameData.steps.length;
                            chatElems.input.value = `I completed the '${gameData.title}' mini-game! I finished all ${totalSteps} steps and earned the ${gameData.final_reward_badge} badge. Please analyze my performance and tell me how this helps my career!`;
                            chatElems.send.disabled = false;
                            chatElems.send.click();
                            bubble.style.opacity = '0.7';
                            bubble.style.pointerEvents = 'none';
                        };
                        return;
                    }

                    const step = gameData.steps[currentStep];
                    let html = `
                        <div style="font-size:0.75rem; font-weight:bold; color:var(--accent-coral); text-transform:uppercase; margin-bottom:8px;">${gameData.title} - Level: ${gameData.level}</div>
                        <div style="font-size:3rem; text-align:center; margin:15px 0;">${step.visual}</div>
                        <p style="font-style:italic; margin-bottom:15px; line-height:1.4;">👵 "${step.paati_says}"</p>
                    `;

                    if (step.question) {
                        html += `<p style="font-weight:600; margin-bottom:15px;">${step.question}</p>`;
                    }

                    // Render Sequence Puzzle
                    if (step.step_type === 'sequence' && step.sequence_items) {
                        html += `<p style="font-size:0.85rem; color:var(--gray-500); margin-bottom:10px;">Click the items in the correct order:</p>`;
                        html += `<div class="sequence-container" id="seq-${currentStep}">`;
                        step.sequence_items.forEach((item) => {
                            html += `<div class="sequence-item" data-item="${item.replace(/"/g, '&quot;')}">${item}</div>`;
                        });
                        html += `</div>`;

                        // NEW: Render Code Debug Puzzle
                    } else if (step.step_type === 'code_debug' && step.code_snippet) {
                        html += `<p style="font-size:0.85rem; color:var(--gray-500); margin-bottom:10px;">Find the bug! Type the line number:</p>`;
                        html += `<pre style="background:#1e1e1e; color:#d4d4d4; padding:10px; border-radius:6px; font-size:0.8rem; overflow-x:auto;"><code>${step.code_snippet}</code></pre>`;
                        html += `<div style="display:flex; gap:10px; margin-top:10px;">
                                    <input type="number" id="debug-line-input" placeholder="Line #" style="flex:1; padding:8px; border-radius:4px; border:1px solid #ccc; background:var(--cream); color:black;">
                                    <button id="debug-submit" style="padding:8px 16px; background:var(--accent-teal); color:white; border:none; border-radius:4px; cursor:pointer;">Fix Bug</button>
                                 </div>`;

                        // Render Options Quiz
                    } else if (step.options && step.options.length > 0) {
                        html += `<div style="display:flex; flex-direction:column; gap:8px;">`;
                        step.options.forEach(opt => {
                            html += `<button class="game-opt-btn" data-val="${opt.replace(/"/g, '&quot;')}" style="padding:10px; background:var(--cream); border:1px solid var(--gray-300); border-radius:8px; cursor:pointer; text-align:left; transition:0.2s;">${opt}</button>`;
                        });
                        html += `</div>`;

                        // Render Simple Dialogue
                    } else if (step.step_type === 'dialogue') {
                        html += `<button class="game-next-btn" style="margin-top:10px; padding:10px; background:var(--accent-teal); color:white; border:none; border-radius:8px; cursor:pointer; width:100%; font-weight:bold;">Next</button>`;
                    }

                    bubble.innerHTML = html;

                    // Sequence Logic
                    if (step.step_type === 'sequence' && step.sequence_items) {
                        let selectedOrder = [];
                        const seqContainer = bubble.querySelector(`#seq-${currentStep}`);
                        const items = seqContainer.querySelectorAll('.sequence-item');

                        items.forEach(itemEl => {
                            itemEl.onclick = (e) => {
                                if (itemEl.classList.contains('selected')) return;
                                itemEl.classList.add('selected');
                                const val = itemEl.getAttribute('data-item');
                                selectedOrder.push(val);

                                if (selectedOrder.length === step.sequence_items.length) {
                                    let isCorrect = true;
                                    for (let i = 0; i < selectedOrder.length; i++) {
                                        if (selectedOrder[i] !== step.correct_sequence[i]) {
                                            isCorrect = false;
                                            break;
                                        }
                                    }
                                    if (isCorrect) {
                                        seqContainer.style.borderColor = 'var(--risk-low)';
                                        setTimeout(() => { currentStep++; renderStep(); }, 1000);
                                    } else {
                                        seqContainer.style.borderColor = 'var(--risk-high)';
                                        setTimeout(() => {
                                            selectedOrder = [];
                                            items.forEach(i => i.classList.remove('selected'));
                                            seqContainer.style.borderColor = 'transparent';
                                        }, 800);
                                    }
                                }
                            };
                        });

                        // NEW: Code Debug Logic
                    } else if (step.step_type === 'code_debug' && step.code_snippet) {
                        const submitBtn = bubble.querySelector('#debug-submit');
                        const inputField = bubble.querySelector('#debug-line-input');
                        submitBtn.onclick = () => {
                            const val = parseInt(inputField.value);
                            if (val === step.bug_line) {
                                submitBtn.style.background = 'var(--risk-low)';
                                submitBtn.textContent = 'Correct!';
                                setTimeout(() => { currentStep++; renderStep(); }, 1000);
                            } else {
                                submitBtn.style.background = 'var(--risk-high)';
                                submitBtn.textContent = 'Try Again';
                                setTimeout(() => {
                                    submitBtn.style.background = 'var(--accent-teal)';
                                    submitBtn.textContent = 'Fix Bug';
                                }, 1000);
                            }
                        }

                        // Quiz Logic
                    } else if (step.options && step.options.length > 0) {
                        bubble.querySelectorAll('.game-opt-btn').forEach(btn => {
                            btn.onclick = (e) => {
                                const val = e.target.getAttribute('data-val');
                                if (val === step.correct_answer) {
                                    e.target.style.background = 'var(--risk-low)';
                                    e.target.style.color = 'white';
                                    e.target.style.borderColor = 'var(--risk-low)';
                                    setTimeout(() => { currentStep++; renderStep(); }, 1000);
                                } else {
                                    e.target.style.background = 'var(--risk-high)';
                                    e.target.style.color = 'white';
                                    e.target.style.borderColor = 'var(--risk-high)';
                                    setTimeout(() => {
                                        e.target.style.background = 'var(--cream)';
                                        e.target.style.color = 'inherit';
                                        e.target.style.borderColor = 'var(--gray-300)';
                                    }, 800);
                                }
                            };
                        });

                        // Dialogue Logic
                    } else if (step.step_type === 'dialogue') {
                        bubble.querySelector('.game-next-btn').onclick = () => {
                            currentStep++;
                            renderStep();
                        };
                    }
                }

                renderStep();

                const textBefore = text.split(/```json|\{/)[0].trim();
                if (textBefore) {
                    const txtBubble = document.createElement('div');
                    txtBubble.className = 'chat-bubble';
                    txtBubble.innerHTML = formatChatMarkdown(textBefore);
                    msgDiv.appendChild(txtBubble);
                }

                msgDiv.appendChild(bubble);
                chatElems.messages.appendChild(msgDiv);
                chatElems.messages.scrollTop = chatElems.messages.scrollHeight;
                return;
            }
        } catch (e) {
            console.error("Game rendering error:", e);
        }
    }

    const bubble = document.createElement('div');
    bubble.className = 'chat-bubble';
    bubble.innerHTML = formatChatMarkdown(text);

    msgDiv.appendChild(bubble);
    chatElems.messages.appendChild(msgDiv);
    chatElems.messages.scrollTop = chatElems.messages.scrollHeight;
}

function addTypingIndicator() {
    if (document.getElementById('chatTyping')) return;
    const indicator = document.createElement('div');
    indicator.id = 'chatTyping';
    indicator.className = 'chat-msg chat-msg-ai';
    indicator.innerHTML = `
        <div class="chat-bubble typing-indicator">
            <span class="dot"></span>
            <span class="dot"></span>
            <span class="dot"></span>
        </div>
    `;
    chatElems.messages.appendChild(indicator);
    chatElems.messages.scrollTop = chatElems.messages.scrollHeight;
}

function removeTypingIndicator() {
    const el = document.getElementById('chatTyping');
    if (el) el.remove();
}

function setChatStatus(text) {
    if (chatElems.status) {
        chatElems.status.innerHTML = `<span class="status-dot"></span> ${text}`;
    }
}

function formatChatMarkdown(text) {
    if (!text) return '';
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
        .replace(/\*(.+?)\*/g, '<em>$1</em>')
        .replace(/`(.+?)`/g, '<code>$1</code>')
        .replace(/^\s*[-•]\s+(.+)/gm, '<li>$1</li>')
        .replace(/^\s*(\d+)\.\s+(.+)/gm, '<li>$2</li>')
        .replace(/\n/g, '<br>');
}

let mediaRecorder;
let audioChunks = [];
let isRecording = false;

async function toggleRecording() {
    if (!isRecording) {
        try {
            const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
            mediaRecorder = new MediaRecorder(stream);
            mediaRecorder.ondataavailable = e => {
                if (e.data.size > 0) audioChunks.push(e.data);
            };
            mediaRecorder.onstop = sendAudioMessage;
            mediaRecorder.start();
            isRecording = true;
            chatElems.mic.classList.add('recording-pulse');
            chatElems.input.placeholder = "🎙️ Paati is listening... (Click mic to stop)";
            chatElems.input.disabled = true;
        } catch (err) {
            console.error("Microphone access denied", err);
            addChatMessage('system', '⚠️ Microphone access denied. Please allow permissions.');
        }
    } else {
        mediaRecorder.stop();
        isRecording = false;
        chatElems.mic.classList.remove('recording-pulse');
        chatElems.input.placeholder = "Reply to Paati...";
        chatElems.input.disabled = false;
    }
}

async function sendAudioMessage() {
    const audioBlob = new Blob(audioChunks, { type: 'audio/webm' });
    audioChunks = [];

    addTypingIndicator();
    setChatStatus('Transcribing...');

    const formData = new FormData();
    formData.append("audio_file", audioBlob, "voice_note.webm");
    formData.append("session_id", chatSessionId);

    try {
        const response = await fetch(`${API_BASE_URL}/chat/audio`, {
            method: 'POST',
            body: formData
        });

        if (!response.ok) throw new Error(`HTTP ${response.status}`);

        const data = await response.json();
        removeTypingIndicator();

        addChatMessage('user', `🎤 <i>${data.transcript}</i>`);
        addChatMessage('ai', data.response);

        // PLAY TTS AUDIO IF RECEIVED
        if (data.audio_base64) playAudioBase64(data.audio_base64);

        if (data.points_update) {
            updatePaatiScoreboard(data.new_points, data.new_level, data.new_kurals);
        }

        setChatStatus('Online');

    } catch (error) {
        console.error('Audio Chat error:', error);
        removeTypingIndicator();
        addChatMessage('system', '⚠️ Failed to send voice message. Try typing instead.');
        setChatStatus('Online');
    }
}

// Start the app when the page is loaded
document.addEventListener('DOMContentLoaded', init);

