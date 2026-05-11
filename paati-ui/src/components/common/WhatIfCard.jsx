import React, { useState, useEffect } from 'react';

export function WhatIfCard({ scenario, delay }) {
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
