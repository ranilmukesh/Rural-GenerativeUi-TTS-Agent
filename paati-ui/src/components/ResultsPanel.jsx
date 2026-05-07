import { useEffect, useRef, useState } from 'react';
import { animateCounter, formatFeatureName, getRecommendations } from '../utils.js';
import { apiPredict } from '../api.js';

// ─── Risk Card ─────────────────────────────────────────────────────────
function RiskCard({ prediction }) {
  const [displayPct, setDisplayPct] = useState('0.0');
  const ringRef = useRef(null);
  const pointerRef = useRef(null);

  useEffect(() => {
    if (!prediction) return;
    const pct = prediction.probability_percentage;
    animateCounter(setDisplayPct, 0, pct, 1500);
    const circ = 2 * Math.PI * 54;
    const offset = circ - (pct / 100) * circ;
    setTimeout(() => {
      if (ringRef.current) ringRef.current.style.strokeDashoffset = offset;
      if (pointerRef.current) pointerRef.current.style.left = `${pct}%`;
    }, 100);
  }, [prediction]);

  if (!prediction) return null;

  return (
    <div className={`risk-card ${prediction.risk_level?.toLowerCase()}`} style={{ marginBottom: 24 }}>
      <div className="risk-header">
        <span className="risk-label">Placement Chance</span>
        <span className="risk-confidence">{prediction.confidence}</span>
      </div>
      <div className="risk-display">
        <div className="risk-circle">
          <svg className="progress-ring" viewBox="0 0 120 120">
            <circle className="progress-ring-bg" cx="60" cy="60" r="54" />
            <circle ref={ringRef} className="progress-ring-fill" cx="60" cy="60" r="54"
              style={{ strokeDasharray: 339.29, strokeDashoffset: 339.29, transition: 'stroke-dashoffset 1.5s ease' }} />
          </svg>
          <div className="risk-value">
            <span className="risk-percentage">{displayPct}</span>
            <span className="risk-symbol">%</span>
          </div>
        </div>
        <div className="risk-level">{prediction.risk_level}</div>
      </div>
      <div className="risk-meter">
        <div className="meter-track">
          <div className="meter-fill" />
          <div ref={pointerRef} className="meter-pointer" style={{ left: '0%', transition: 'left 1s ease' }}>
            <div className="pointer-line" />
          </div>
        </div>
        <div className="meter-labels"><span>Low</span><span>Medium</span><span>High</span></div>
      </div>
    </div>
  );
}

// ─── Factor Card ────────────────────────────────────────────────────────
function FactorCard({ factor, maxImpact, delay }) {
  const [animated, setAnimated] = useState(false);
  const isPositive = factor.impact > 0;
  const norm = (Math.abs(factor.impact) / maxImpact) * 100;

  useEffect(() => {
    const t = setTimeout(() => setAnimated(true), delay);
    return () => clearTimeout(t);
  }, [delay]);

  return (
    <div className={`factor-card ${animated ? 'animate' : ''}`}>
      <div className="factor-header">
        <span className="factor-name">{formatFeatureName(factor.feature)}</span>
        <span className={`factor-direction ${isPositive ? 'increases' : 'reduces'}`}>{factor.direction}</span>
      </div>
      <p className="factor-interpretation">{factor.interpretation}</p>
      <div className="factor-bar">
        <div className={`factor-bar-fill ${isPositive ? 'positive' : 'negative'}`}
          style={{ width: animated ? `${norm}%` : '0%', transition: 'width 1s ease' }} />
      </div>
    </div>
  );
}

// ─── Simulator ─────────────────────────────────────────────────────────
function Simulator({ formData, baselinePrediction }) {
  const [simAge, setSimAge] = useState(formData.Age);
  const [simIntern, setSimIntern] = useState(formData.Internships);
  const [simCGPA, setSimCGPA] = useState(formData.CGPA);
  const [simBacklog, setSimBacklog] = useState(formData.HistoryOfBacklogs === 1);
  const [simResult, setSimResult] = useState(baselinePrediction);
  const debounceRef = useRef(null);

  useEffect(() => {
    clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      const data = { ...formData, Age: simAge, Internships: simIntern, CGPA: simCGPA, HistoryOfBacklogs: simBacklog ? 1 : 0 };
      try {
        const r = await apiPredict(data);
        setSimResult(r);
      } catch { }
    }, 300);
  }, [simAge, simIntern, simCGPA, simBacklog]);

  const risk = simResult?.probability_percentage ?? 0;
  const base = baselinePrediction?.probability_percentage ?? 0;
  const delta = (risk - base).toFixed(1);
  const arcLen = 219.91;
  const offset = arcLen * (1 - risk / 100);
  const strokeColor = simResult?.risk_level === 'HIGH' ? 'var(--risk-high)' : simResult?.risk_level === 'MEDIUM' ? 'var(--risk-medium)' : 'var(--risk-low)';

  return (
    <div className="sim-card">
      <h3 className="sim-title">⚡ Interactive Risk Simulator</h3>
      <div className="sim-chart-container">
        <svg viewBox="0 0 200 120" className="sim-svg">
          <path d="M 30 100 A 70 70 0 0 1 170 100" fill="none" className="sim-track-bg" />
          <path d="M 30 100 A 70 70 0 0 1 170 100" fill="none" className="sim-track-fill"
            style={{ strokeDasharray: arcLen, strokeDashoffset: offset, stroke: strokeColor }} />
          <path d="M 100 18 L 92 24 V 34 C 92 42 100 48 100 48 C 100 48 108 42 108 34 V 24 Z" className="sim-shield-icon" />
          <text x="100" y="36" className="sim-shield-cross">+</text>
        </svg>
        <div className="sim-center-text">
          <div className="sim-val">{risk.toFixed(1)}%</div>
          <div className="sim-lbl">{simResult?.risk_level ?? ''} CHANCE</div>
        </div>
      </div>
      <div className="sim-stats-row">
        <div className="sim-stat">
          <span className="sim-stat-lbl">Probability</span>
          <strong className="sim-stat-val">{risk.toFixed(1)}%</strong>
        </div>
        <div className="sim-stat">
          <span className="sim-stat-lbl">Delta</span>
          <strong className={`sim-stat-val ${delta > 0 ? 'negative' : delta < 0 ? 'positive' : ''}`}>
            {delta > 0 ? '+' : ''}{delta}%
          </strong>
        </div>
        <div className="sim-stat">
          <span className="sim-stat-lbl">Target</span>
          <strong className="sim-stat-val">{base.toFixed(1)}%</strong>
        </div>
      </div>
      <div className="sim-controls-grid">
        <div className="sim-control">
          <span className="sim-control-lbl">Age</span>
          <input type="range" className="sim-slider" min="15" max="50" value={simAge} onChange={e => setSimAge(+e.target.value)} />
          <input type="number" className="sim-num-input" min="15" max="50" value={simAge} onChange={e => setSimAge(+e.target.value)} />
        </div>
        <div className="sim-control">
          <span className="sim-control-lbl">Internships</span>
          <input type="range" className="sim-slider" min="0" max="10" value={simIntern} onChange={e => setSimIntern(+e.target.value)} />
          <input type="number" className="sim-num-input" min="0" max="10" value={simIntern} onChange={e => setSimIntern(+e.target.value)} />
        </div>
        <div className="sim-control">
          <span className="sim-control-lbl">CGPA</span>
          <input type="range" className="sim-slider" min="0" max="10" step="0.01" value={simCGPA} onChange={e => setSimCGPA(+e.target.value)} />
          <input type="number" className="sim-num-input" min="0" max="10" step="0.1" value={simCGPA} onChange={e => setSimCGPA(+e.target.value)} />
        </div>
        <div className="sim-control sim-toggle">
          <span className="sim-control-lbl">Backlogs</span>
          <label className="sim-switch">
            <input type="checkbox" checked={simBacklog} onChange={e => setSimBacklog(e.target.checked)} />
            <span className="sim-slider-round" />
          </label>
        </div>
      </div>
      <div className="sim-disclaimer">AI-generated, may include mistakes.</div>
    </div>
  );
}

// ─── WhatIf card ───────────────────────────────────────────────────────
function WhatIfCard({ scenario, delay }) {
  const [animated, setAnimated] = useState(false);
  useEffect(() => { const t = setTimeout(() => setAnimated(true), delay); return () => clearTimeout(t); }, [delay]);
  const isRed = scenario.risk_delta > 0;
  const barW = Math.min(Math.abs(scenario.risk_reduction_percent), 100);
  return (
    <div className={`whatif-card ${animated ? 'animate' : ''}`}>
      <div className="whatif-card-icon">{scenario.icon}</div>
      <div className="whatif-card-content" style={{ flex: 1 }}>
        <h4 className="whatif-card-title">{scenario.title}</h4>
        <p className="whatif-card-desc">{scenario.description}</p>
        <div className="whatif-card-comparison">
          <div><span className="whatif-risk-label">Current</span><span className="whatif-risk-val">{scenario.original_risk.toFixed(1)}%</span></div>
          <svg className={`whatif-arrow ${isRed ? 'increase' : 'reduction'}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M5 12h14M12 5l7 7-7 7" /></svg>
          <div><span className="whatif-risk-label">Modified</span><span className={`whatif-risk-val ${isRed ? 'worsened' : 'improved'}`}>{scenario.modified_risk.toFixed(1)}%</span></div>
        </div>
        <div className="whatif-delta-bar">
          <div className={`whatif-delta-fill ${isRed ? 'negative' : 'positive'}`} style={{ width: animated ? `${barW}%` : '0%', transition: 'width 1s ease' }} />
        </div>
        <div className={`whatif-delta-text ${isRed ? 'negative' : 'positive'}`}>
          {isRed ? '↑' : '↓'} {Math.abs(scenario.risk_delta).toFixed(1)}% risk {isRed ? 'increase' : 'reduction'}
          <span className="whatif-delta-pct"> ({Math.abs(scenario.risk_reduction_percent).toFixed(1)}%)</span>
        </div>
      </div>
    </div>
  );
}

// ─── Main Results Panel ────────────────────────────────────────────────
export default function ResultsPanel({ prediction, explanation, whatif, formData, onBack, scoreboard }) {
  const maxImpact = explanation?.top_contributing_factors?.length
    ? Math.max(...explanation.top_contributing_factors.map(f => Math.abs(f.impact)))
    : 1;
  const recs = getRecommendations(prediction?.risk_level, prediction);

  return (
    <div style={{ width: '100%', maxWidth: 720, margin: '0 auto', paddingBottom: 32 }}>
      <button onClick={onBack} style={{
        display: 'inline-flex', alignItems: 'center', gap: 8,
        padding: '8px 16px', background: 'transparent', border: '2px solid var(--gray-300)',
        borderRadius: 99, fontSize: 14, fontWeight: 500, color: 'var(--gray-600)', cursor: 'pointer', marginBottom: 24,
      }}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M19 12H5M12 19l-7-7 7-7" /></svg>
        New Assessment
      </button>

      {/* Scoreboard */}
      {scoreboard && (
        <div className="paati-scoreboard" style={{ marginBottom: 20 }}>
          <div className="score-item" style={{ border: '2px solid var(--accent-gold)' }}>
            <span style={{ color: 'var(--gray-400)' }}>Paati Points</span>
            <strong style={{ color: 'var(--accent-gold)', fontSize: '1.5rem' }}>{scoreboard.points}</strong>
          </div>
          <div className="score-item">
            <span style={{ color: 'var(--gray-400)' }}>League Level</span>
            <strong style={{ color: 'var(--cream)', fontSize: '1rem' }}>{scoreboard.level}</strong>
          </div>
          <div className="score-item">
            <span style={{ color: 'var(--gray-400)' }}>Kurals Earned</span>
            <strong style={{ color: 'var(--accent-teal)', fontSize: '1.5rem' }}>{scoreboard.kurals}</strong>
          </div>
        </div>
      )}

      <RiskCard prediction={prediction} />

      {/* Simulator */}
      {formData && <Simulator formData={formData} baselinePrediction={prediction} />}

      {/* Factors */}
      <div className="section-header" style={{ marginTop: 24 }}>
        <h2 className="section-title">Placement Factor Analysis</h2>
        <p className="section-subtitle">SHAP-based explainable AI insights</p>
      </div>
      <div className="factors-container">
        {explanation?.top_contributing_factors?.map((f, i) => (
          <FactorCard key={i} factor={f} maxImpact={maxImpact} delay={i * 80} />
        ))}
      </div>
      <div className="info-card">
        <div className="info-icon">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z" /></svg>
        </div>
        <div className="info-content">
          <h4>About SHAP Analysis</h4>
          <p>SHAP values show how each factor contributes to your individual prediction, providing transparent insights.</p>
        </div>
      </div>

      {/* Graph */}
      {prediction?.graph_data && (
        <div className="graph-section" style={{ marginTop: 24 }}>
          <div className="section-header"><h2 className="section-title">Knowledge Graph Gap Analysis</h2></div>
          <div className="graph-panel"><img src={prediction.graph_data} alt="Skill Gap Map" /></div>
        </div>
      )}

      {/* Recommendations */}
      <div className="section-header" style={{ marginTop: 24 }}>
        <h2 className="section-title">Personalized Insights</h2>
      </div>
      <div className="recommendations-grid">
        {recs.map((r, i) => (
          <div key={i} className={`recommendation-card ${r.title === 'Career Path' ? 'highlight-card' : ''}`}>
            <div className="recommendation-icon">{r.icon}</div>
            <h4 className="recommendation-title">{r.title}</h4>
            <p className="recommendation-text" dangerouslySetInnerHTML={{ __html: r.text }} />
          </div>
        ))}
      </div>

      {/* What-If */}
      {whatif?.scenarios?.length > 0 && (
        <div className="whatif-section" style={{ marginTop: 24 }}>
          <div className="section-header">
            <h2 className="section-title">🔮 What-If Scenario Analysis</h2>
            <p className="section-subtitle">See how modifying factors could change your placement chance</p>
          </div>
          <div className="whatif-scenarios-grid">
            {whatif.scenarios.map((s, i) => <WhatIfCard key={i} scenario={s} delay={i * 150} />)}
          </div>
          {whatif.combined_risk != null && whatif.scenarios.length > 1 && (
            <div className="whatif-combined-card animate" style={{ marginTop: 16 }}>
              <div className="combined-header">
                <span className="combined-icon">🌟</span>
                <div>
                  <h3 className="combined-title">Best Possible Outcome</h3>
                  <p className="combined-subtitle">If all recommended changes are applied together</p>
                </div>
              </div>
              <div className="combined-results">
                <div><span className="combined-label">Current Chance</span><span className="combined-value">{whatif.original_risk?.toFixed(1)}%</span></div>
                <div className="combined-arrow"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M5 12h14M12 5l7 7-7 7" /></svg></div>
                <div><span className="combined-label">Potential Chance</span><span className={`combined-value ${whatif.combined_risk < whatif.original_risk ? 'improved' : ''}`}>{whatif.combined_risk?.toFixed(1)}%</span></div>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
