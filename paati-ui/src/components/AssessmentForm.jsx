import { useState, useEffect, useRef } from 'react';
import { apiOptions, apiUploadResume } from '../api.js';

export default function AssessmentForm({ onSubmit, isLoading }) {
  const [streams, setStreams] = useState([]);
  const [jobs, setJobs] = useState([]);
  const [availableSkills, setAvailableSkills] = useState([]);
  const [selectedSkills, setSelectedSkills] = useState([]);
  const [skillInput, setSkillInput] = useState('');
  const [showDropdown, setShowDropdown] = useState(false);
  const [resumeStatus, setResumeStatus] = useState('Upload your resume to let Paati analyze your background!');
  const [resumeText, setResumeText] = useState('');
  const skillsWrapperRef = useRef(null);

  useEffect(() => {
    apiOptions().then(data => {
      setStreams(data.streams || []);
      setJobs(data.jobs || []);
      setAvailableSkills(data.skills || []);
    }).catch(() => {});
  }, []);

  // Close dropdown on outside click
  useEffect(() => {
    const handler = (e) => {
      if (skillsWrapperRef.current && !skillsWrapperRef.current.contains(e.target)) {
        setShowDropdown(false);
      }
    };
    document.addEventListener('click', handler);
    return () => document.removeEventListener('click', handler);
  }, []);

  const filteredSkills = availableSkills.filter(
    s => s.toLowerCase().includes(skillInput.toLowerCase()) && !selectedSkills.includes(s)
  ).slice(0, 50);

  function addSkill(skill) {
    setSelectedSkills(prev => [...prev, skill]);
    setSkillInput('');
    setShowDropdown(false);
  }

  function removeSkill(i) {
    setSelectedSkills(prev => prev.filter((_, idx) => idx !== i));
  }

  function handleSkillKeyDown(e) {
    if (e.key === 'Enter') {
      e.preventDefault();
      const match = availableSkills.find(s => s.toLowerCase() === skillInput.toLowerCase());
      if (match && !selectedSkills.includes(match)) addSkill(match);
    } else if (e.key === 'Backspace' && skillInput === '' && selectedSkills.length > 0) {
      setSelectedSkills(prev => prev.slice(0, -1));
    }
  }

  async function handleResumeChange(e) {
    const file = e.target.files[0];
    if (!file) return;
    setResumeStatus('⏳ Paati is reading your resume...');
    try {
      const data = await apiUploadResume(file);
      if (data.status === 'success') {
        setResumeText(data.resume_text);
        setResumeStatus('✅ Resume loaded! Paati knows your skills now.');
      } else throw new Error();
    } catch {
      setResumeStatus('❌ Failed to read resume. Try a different PDF.');
    }
  }

  function handleSubmit(e) {
    e.preventDefault();
    const fd = e.target;
    const data = {
      Gender: fd.gender.value,
      Age: parseInt(fd.age.value),
      Stream: fd.stream.value,
      Internships: parseInt(fd.internships.value) || 0,
      CGPA: parseFloat(fd.cgpa.value) || 0,
      Hostel: fd.hostel.checked ? 1 : 0,
      HistoryOfBacklogs: fd.backlogs.checked ? 1 : 0,
      skills: selectedSkills,
      desired_role: fd.desired_role.value || null,
      resume_text: resumeText,
    };

    // Validate
    if (!data.Gender || !data.Age || !data.Stream) {
      alert('Please fill in all required fields.');
      return;
    }
    if (data.Age < 15 || data.Age > 50) { alert('Please enter a valid age (15-50).'); return; }
    if (data.CGPA < 0 || data.CGPA > 10) { alert('Please enter a valid CGPA (0-10).'); return; }

    onSubmit(data);
  }

  function fillDemo(e) {
    e.preventDefault();
    // Demo fill via direct DOM
    document.getElementById('f-gender').value = 'Female';
    document.getElementById('f-age').value = '21';
    document.getElementById('f-internships').value = '1';
    document.getElementById('f-cgpa').value = '7.5';
    document.getElementById('f-hostel').checked = true;
    document.getElementById('f-backlogs').checked = true;
    if (streams.includes('Information Technology')) {
      document.getElementById('f-stream').value = 'Information Technology';
    }
    setSelectedSkills(['Python', 'SQL', 'Git']);
    const role = jobs.find(j => j === 'Data Analyst');
    if (role) document.getElementById('f-desired_role').value = role;
  }

  const selectArrow = (
    <span className="select-arrow">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M6 9l6 6 6-6" /></svg>
    </span>
  );

  return (
    <div style={{ width: '100%', maxWidth: 720, margin: '0 auto', paddingBottom: 32 }}>
      <div className="section-header">
        <h2 className="section-title">Student Assessment</h2>
        <p className="section-subtitle">Enter academic parameters for placement evaluation</p>
      </div>

      <form id="patientForm" onSubmit={handleSubmit}>
        {/* Personal Details */}
        <div className="form-group-container">
          <h3 className="group-title"><span className="group-icon">👤</span> Personal Details</h3>

          <div className="input-row">
            <div className="input-group">
              <label className="input-label">Gender</label>
              <div className="select-wrapper">
                <select id="f-gender" name="gender" required>
                  <option value="">Select gender</option>
                  <option value="Male">Male</option>
                  <option value="Female">Female</option>
                </select>
                {selectArrow}
              </div>
            </div>
            <div className="input-group">
              <label className="input-label">Age (years)</label>
              <input type="number" id="f-age" name="age" placeholder="e.g., 21" min="15" max="50" required />
              <span className="input-hint">15-50 years</span>
            </div>
          </div>

          <div className="input-row">
            <div className="input-group">
              <label className="input-label">Stream</label>
              <div className="select-wrapper">
                <select id="f-stream" name="stream" required>
                  <option value="">Select stream</option>
                  {streams.map(s => <option key={s} value={s}>{s}</option>)}
                </select>
                {selectArrow}
              </div>
            </div>
            <div className="input-group" style={{ position: 'relative' }}>
              <label className="input-label">Target Skills</label>
              <div
                ref={skillsWrapperRef}
                onClick={() => { document.getElementById('skills-input').focus(); setShowDropdown(true); }}
                style={{
                  display: 'flex', flexWrap: 'wrap', gap: 6, padding: '8px 12px',
                  background: 'var(--gray-100)', border: '2px solid transparent',
                  borderRadius: 'var(--radius-md)', minHeight: 52, cursor: 'text', transition: 'var(--transition-base)',
                }}
              >
                {selectedSkills.map((s, i) => (
                  <span key={i} className="skill-tag">
                    {s} <span onClick={e => { e.stopPropagation(); removeSkill(i); }}>×</span>
                  </span>
                ))}
                <input
                  id="skills-input"
                  type="text"
                  value={skillInput}
                  onChange={e => { setSkillInput(e.target.value); setShowDropdown(true); }}
                  onFocus={() => setShowDropdown(true)}
                  onKeyDown={handleSkillKeyDown}
                  placeholder={selectedSkills.length === 0 ? 'Search skills...' : ''}
                  autoComplete="off"
                  style={{ border: 'none', background: 'transparent', color: 'var(--black)', outline: 'none', flex: 1, minWidth: 130, fontFamily: 'var(--font-primary)', fontSize: '1rem' }}
                />
              </div>
              {showDropdown && filteredSkills.length > 0 && (
                <div style={{
                  position: 'absolute', top: 'calc(100% - 4px)', left: 0, right: 0,
                  maxHeight: 200, overflowY: 'auto', background: 'var(--cream)',
                  border: '1px solid var(--gray-300)', borderRadius: 'var(--radius-md)',
                  zIndex: 100, boxShadow: 'var(--shadow-md)',
                }}>
                  {filteredSkills.map(skill => (
                    <div key={skill} className="skills-dropdown-item" onMouseDown={e => { e.preventDefault(); addSkill(skill); }}>
                      {skill}
                    </div>
                  ))}
                </div>
              )}
              <span className="input-hint">Select from available skills</span>
            </div>
          </div>

          <div className="input-row" style={{ gridTemplateColumns: '1fr' }}>
            <div className="input-group">
              <label className="input-label">Desired Role (Dream Job)</label>
              <div className="select-wrapper">
                <select id="f-desired_role" name="desired_role">
                  <option value="">Select Target Job Role</option>
                  {jobs.map(j => <option key={j} value={j}>{j}</option>)}
                </select>
                {selectArrow}
              </div>
              <span className="input-hint">Optional: to compare your skills against a specific job!</span>
            </div>
          </div>

          <div className="input-row" style={{ gridTemplateColumns: '1fr' }}>
            <div className="input-group">
              <label className="input-label">Upload Resume (PDF/TXT)</label>
              <input type="file" accept=".pdf,.txt" onChange={handleResumeChange}
                style={{ background: 'var(--gray-100)', padding: 10, borderRadius: 'var(--radius-md)', width: '100%' }} />
              <span className="input-hint">{resumeStatus}</span>
            </div>
          </div>
        </div>

        {/* Academic Background */}
        <div className="form-group-container">
          <h3 className="group-title"><span className="group-icon">🏛️</span> Academic Background</h3>
          <div className="input-row">
            <div className="input-group">
              <label className="input-label">Hostel</label>
              <div className="checkbox-card">
                <input type="checkbox" id="f-hostel" name="hostel" />
                <label htmlFor="f-hostel" className="checkbox-label">
                  <span className="checkbox-box">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3"><path d="M5 13l4 4L19 7" /></svg>
                  </span>
                  <span className="checkbox-text">
                    <span className="checkbox-title">Hostel Accommodation</span>
                    <span className="checkbox-desc">Currently living in a hostel</span>
                  </span>
                </label>
              </div>
            </div>
            <div className="input-group">
              <label className="input-label">History of Backlogs</label>
              <div className="checkbox-card">
                <input type="checkbox" id="f-backlogs" name="backlogs" />
                <label htmlFor="f-backlogs" className="checkbox-label">
                  <span className="checkbox-box">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3"><path d="M5 13l4 4L19 7" /></svg>
                  </span>
                  <span className="checkbox-text">
                    <span className="checkbox-title">Academic Backlogs</span>
                    <span className="checkbox-desc">Any history of backlogs</span>
                  </span>
                </label>
              </div>
            </div>
          </div>
        </div>

        {/* Performance Metrics */}
        <div className="form-group-container">
          <h3 className="group-title"><span className="group-icon">📈</span> Performance Metrics</h3>
          <div className="input-row">
            <div className="input-group">
              <label className="input-label">Internships</label>
              <div className="input-with-unit">
                <input type="number" id="f-internships" name="internships" placeholder="e.g., 2" step="1" min="0" required />
                <span className="input-unit">count</span>
              </div>
              <span className="input-hint">Number of internships completed</span>
            </div>
            <div className="input-group">
              <label className="input-label">CGPA</label>
              <div className="input-with-unit">
                <input type="number" id="f-cgpa" name="cgpa" placeholder="e.g., 8.5" step="0.01" min="0" max="10" required />
                <span className="input-unit">/10</span>
              </div>
              <span className="input-hint">Cumulative Grade Point Average</span>
            </div>
          </div>
        </div>

        <div style={{ display: 'flex', gap: 12 }}>
          <button type="submit" className="submit-btn" disabled={isLoading} style={{ flex: 1 }}>
            <span className="btn-text">{isLoading ? 'Analyzing...' : 'Let Paati Analyze'}</span>
            <span className="btn-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M5 12h14M12 5l7 7-7 7" /></svg>
            </span>
          </button>
          <button type="button" onClick={fillDemo} style={{
            padding: '0 20px', background: 'var(--black)', color: 'var(--accent-gold)',
            border: 'none', borderRadius: 'var(--radius-lg)', fontWeight: 600, cursor: 'pointer', fontSize: 14, whiteSpace: 'nowrap',
          }}>
            Demo
          </button>
        </div>
      </form>
    </div>
  );
}
