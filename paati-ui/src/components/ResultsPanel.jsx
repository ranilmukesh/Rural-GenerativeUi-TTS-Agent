import { useEffect, useRef, useState } from 'react';
import { animateCounter, formatFeatureName, getRecommendations } from '../utils.js';
import { apiPredict } from '../api.js';
import { RiskCard, FactorCard, Simulator, WhatIfCard } from './common';

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
