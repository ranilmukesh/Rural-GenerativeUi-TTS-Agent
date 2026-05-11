import React, { useState, useEffect } from 'react';
import { apiPredict } from '../../api';

export function Simulator({ formData, baselinePrediction }) {
  const [simAge, setSimAge] = useState(formData.Age);
  const [simIntern, setSimIntern] = useState(formData.Internships);
  const [simCGPA, setSimCGPA] = useState(formData.CGPA);
  const [simBacklog, setSimBacklog] = useState(formData.HistoryOfBacklogs === 1);
  const [simResult, setSimResult] = useState(baselinePrediction);

  useEffect(() => {
    const timer = setTimeout(async () => {
      const data = { ...formData, Age: simAge, Internships: simIntern, CGPA: simCGPA, HistoryOfBacklogs: simBacklog ? 1 : 0 };
      try {
        const r = await apiPredict(data);
        setSimResult(r);
      } catch { }
    }, 300);
    return () => clearTimeout(timer);
  }, [simAge, simIntern, simCGPA, simBacklog, formData]);

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
