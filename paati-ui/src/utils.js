/** Format feature name for display (from script.js) */
const NAME_MAP = {
  Age: 'Student Age',
  Gender_Female: 'Gender: Female',
  Gender_Male: 'Gender: Male',
  Internships: 'Number of Internships',
  CGPA: 'Cumulative GPA',
  Hostel: 'Hostel Accommodation',
  HistoryOfBacklogs: 'Academic Backlogs',
};

export function formatFeatureName(name) {
  if (name.startsWith('Stream_')) return 'Stream: ' + name.replace('Stream_', '').replace(/_/g, ' ');
  return NAME_MAP[name] || name.replace(/_/g, ' ').replace(/\b\w/g, l => l.toUpperCase());
}

/** Animate a numeric counter */
export function animateCounter(setter, start, end, duration = 1500) {
  const startTime = performance.now();
  function update(currentTime) {
    const elapsed = currentTime - startTime;
    const progress = Math.min(elapsed / duration, 1);
    const easeOut = 1 - Math.pow(1 - progress, 3);
    setter((start + (end - start) * easeOut).toFixed(1));
    if (progress < 1) requestAnimationFrame(update);
  }
  requestAnimationFrame(update);
}

/** Format markdown-like chat text to HTML */
export function formatChatMarkdown(text) {
  if (!text) return '';
  return text
    // Strip fenced json blocks — these are handled as structured UI (PuzzleCard / MiniGame)
    .replace(/```json[\s\S]*?```/g, '')
    .replace(/```[\s\S]*?```/g, '')
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

/** Play base64 audio */
let currentAudio = null;
export function playAudioBase64(base64String) {
  if (!base64String) return;
  if (currentAudio) { currentAudio.pause(); currentAudio.currentTime = 0; }
  currentAudio = new Audio('data:audio/wav;base64,' + base64String);
  currentAudio.play().catch(e => console.error('Audio play failed:', e));
}

/** Get recommendations based on risk level (from script.js) */
export function getRecommendations(riskLevel, currentPrediction) {
  const jobRec = currentPrediction?.recommended_job ? {
    icon: '🎯', title: 'Career Path',
    text: `Based on your specific skill profile, your ideal career path is <strong style="color:#ffd700;font-size:1.15em;text-decoration:underline;">${currentPrediction.recommended_job}</strong>.`
  } : null;
  const skillsRec = currentPrediction?.missing_skills?.length > 0 ? {
    icon: '🛠️', title: 'Skill Gaps',
    text: `Focus on mastering: ${currentPrediction.missing_skills.join(', ')}`
  } : null;

  let base = [{ icon: '📚', title: 'Continuous Learning', text: 'Keep building projects to stand out to recruiters.' }];
  if (jobRec) base.unshift(jobRec);
  if (skillsRec) base.push(skillsRec);

  if (riskLevel === 'HIGH') return [
    { icon: '🛠️', title: 'Skill Development', text: 'Focus heavily on matching industry required skills.' },
    { icon: '📈', title: 'Improve Academics', text: 'Work on your CGPA and try to secure internships.' },
    ...base,
  ];
  if (riskLevel === 'MEDIUM') return [
    { icon: '🤝', title: 'Networking', text: 'Connect with alumni and professionals in your target field.' },
    ...base,
  ];
  return [
    { icon: '🚀', title: 'Prepare for Interviews', text: 'You are in a great position. Start practicing mock interviews!' },
    ...base,
  ];
}
