import { MessageSquare, Home, Wrench, LayoutTemplate, Settings, Plus, Command, Clock, FileText, Utensils, Plane, ChevronDown } from 'lucide-react';

const navItems = [
  { icon: Home, label: 'Home' },
  { icon: MessageSquare, label: 'Chats', active: true },
  { icon: Wrench, label: 'Tools' },
  { icon: LayoutTemplate, label: 'Templates' },
  { icon: Settings, label: 'Settings' },
];

const todayChats = [
  { icon: MessageSquare, title: 'Placement Analysis', time: 'Now', active: true },
  { icon: FileText, title: 'Skill Gap Review', time: '9:40 AM' },
  { icon: Utensils, title: 'Career Advice', time: '9:15 AM' },
  { icon: Plane, title: 'Resume Feedback', time: '8:50 AM' },
];

const yesterdayChats = [
  { icon: FileText, title: 'CGPA Planning', time: 'Yesterday' },
  { icon: Clock, title: 'Internship Tips', time: 'Yesterday' },
];

export default function SidebarLeft() {
  return (
    <aside style={{
      width: 260, height: '100%', background: 'rgba(255,255,255,0.5)',
      backdropFilter: 'blur(8px)', borderRight: '1px solid rgba(226,232,240,0.6)',
      display: 'flex', flexDirection: 'column', padding: 16, flexShrink: 0,
    }}>
      {/* Logo */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '8px 8px 24px' }}>
        <div style={{
          width: 32, height: 32, borderRadius: '50%',
          background: 'linear-gradient(135deg, #D35400, #F39C12, #27AE60)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontSize: 18, boxShadow: '0 2px 8px rgba(211,84,0,0.3)',
        }}>
          👵
        </div>
        <span style={{ fontFamily: 'Space Grotesk, sans-serif', fontWeight: 700, fontSize: 17, color: '#1e293b' }}>
          Paati-Kural
        </span>
      </div>

      {/* New Chat */}
      <button style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        width: '100%', padding: '10px 16px', borderRadius: 12,
        border: '1px solid #e2e8f0', background: 'white', color: '#334155',
        cursor: 'pointer', boxShadow: '0 1px 3px rgba(0,0,0,0.06)',
        marginBottom: 24, fontSize: 14, fontWeight: 500,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Plus size={16} color="#6366f1" />
          <span>New Chat</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4, color: '#94a3b8', fontSize: 12, background: '#f1f5f9', padding: '2px 6px', borderRadius: 4 }}>
          <Command size={11} /><span>K</span>
        </div>
      </button>

      {/* Nav */}
      <nav style={{ display: 'flex', flexDirection: 'column', gap: 2, marginBottom: 24 }}>
        {navItems.map((item, i) => (
          <button key={i} style={{
            display: 'flex', alignItems: 'center', gap: 12, padding: '10px 12px',
            borderRadius: 10, border: 'none', cursor: 'pointer', fontSize: 14, fontWeight: 500,
            background: item.active ? 'rgba(99,102,241,0.08)' : 'transparent',
            color: item.active ? '#4f46e5' : '#64748b',
            transition: 'all 0.2s',
          }}>
            <item.icon size={18} strokeWidth={item.active ? 2.5 : 2} color={item.active ? '#4f46e5' : '#94a3b8'} />
            {item.label}
          </button>
        ))}
      </nav>

      {/* History */}
      <div style={{ flex: 1, overflowY: 'auto', overflowX: 'hidden', display: 'flex', flexDirection: 'column', gap: 24 }} className="scrollbar-hide">
        <div>
          <h3 style={{ fontSize: 11, fontWeight: 700, color: '#94a3b8', marginBottom: 8, padding: '0 12px', textTransform: 'uppercase', letterSpacing: '0.05em' }}>Today</h3>
          {todayChats.map((item, i) => (
            <div key={i} style={{
              display: 'flex', alignItems: 'center', justifyContent: 'space-between',
              padding: '8px 12px', borderRadius: 10, cursor: 'pointer',
              background: item.active ? 'white' : 'transparent',
              boxShadow: item.active ? '0 1px 3px rgba(0,0,0,0.06)' : 'none',
              border: item.active ? '1px solid #f1f5f9' : '1px solid transparent',
              color: item.active ? '#4f46e5' : '#64748b', marginBottom: 2,
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, overflow: 'hidden' }}>
                <div style={{ padding: 4, borderRadius: 6, background: item.active ? '#ede9fe' : 'transparent', color: item.active ? '#4f46e5' : '#94a3b8' }}>
                  <item.icon size={13} />
                </div>
                <span style={{ fontSize: 13, fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{item.title}</span>
              </div>
              <span style={{ fontSize: 11, color: item.active ? '#a5b4fc' : '#94a3b8', flexShrink: 0 }}>{item.time}</span>
            </div>
          ))}
        </div>
        <div>
          <h3 style={{ fontSize: 11, fontWeight: 700, color: '#94a3b8', marginBottom: 8, padding: '0 12px', textTransform: 'uppercase', letterSpacing: '0.05em' }}>Yesterday</h3>
          {yesterdayChats.map((item, i) => (
            <div key={i} style={{
              display: 'flex', alignItems: 'center', justifyContent: 'space-between',
              padding: '8px 12px', borderRadius: 10, cursor: 'pointer', color: '#64748b',
              border: '1px solid transparent', marginBottom: 2,
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <div style={{ padding: 4, borderRadius: 6, color: '#94a3b8' }}><item.icon size={13} /></div>
                <span style={{ fontSize: 13, fontWeight: 500 }}>{item.title}</span>
              </div>
              <span style={{ fontSize: 11, color: '#94a3b8' }}>{item.time}</span>
            </div>
          ))}
        </div>
      </div>

      {/* User bottom */}
      <div style={{ marginTop: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 12px', borderRadius: 12, cursor: 'pointer', border: '1px solid transparent' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <div style={{ width: 32, height: 32, borderRadius: '50%', background: 'linear-gradient(135deg,#D35400,#F39C12)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 14 }}>👤</div>
            <div>
              <div style={{ fontSize: 13, fontWeight: 700, color: '#1e293b' }}>Student</div>
              <span style={{ fontSize: 10, fontWeight: 600, color: '#4f46e5', background: '#ede9fe', padding: '1px 6px', borderRadius: 4, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Free Plan</span>
            </div>
          </div>
          <ChevronDown size={15} color="#94a3b8" />
        </div>

        {/* Upgrade card */}
        <div style={{ background: 'linear-gradient(135deg, #f8f9ff, #f1f4ff)', border: '1px solid #e5e9ff', borderRadius: 16, padding: 16, position: 'relative', overflow: 'hidden' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
            <span style={{ fontSize: 16 }}>⭐</span>
            <span style={{ fontSize: 13, fontWeight: 700, color: '#1e293b' }}>Upgrade to Pro</span>
          </div>
          <p style={{ fontSize: 11, color: '#64748b', marginBottom: 10 }}>Unlock more features & Paati sessions</p>
          <button style={{ width: '100%', background: '#6366f1', color: 'white', border: 'none', borderRadius: 8, padding: '8px 0', fontSize: 12, fontWeight: 700, cursor: 'pointer' }}>Upgrade Now</button>
        </div>
      </div>
    </aside>
  );
}
