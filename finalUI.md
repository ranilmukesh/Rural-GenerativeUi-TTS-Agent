import React from 'react';
import {
  MessageSquare,
  Home,
  Wrench,
  LayoutTemplate,
  Settings,
  Plus,
  Command,
  Clock,
  CheckCircle2,
  Circle,
  GripVertical,
  ThumbsUp,
  ThumbsDown,
  Copy,
  Mic,
  ArrowRight,
  Sparkles,
  ChevronDown,
  FileText,
  Puzzle,
  Utensils,
  Plane,
  Check,
  MoreHorizontal
} from 'lucide-react';

// --- Reusable Components ---

const Waveform = ({ bars = 20, active = false, colorClass = "bg-indigo-400" }) => (
  <div className="flex items-center gap-[2px] h-6">
    {Array.from({ length: bars }).map((_, i) => {
      // Generate varied heights to look like a sound wave
      const height = active ? Math.random() * 100 : 20 + Math.sin(i) * 20 + Math.random() * 60;
      return (
        <div
          key={i}
          className={`w-[2px] rounded-full opacity-60 ${colorClass}`}
          style={{ height: `${height}%` }}
        />
      );
    })}
  </div>
);

const IconButton = ({ icon: Icon, className = "" }) => (
  <button className={`p-1.5 text-slate-400 hover:text-slate-600 hover:bg-slate-100 rounded-md transition-colors ${className}`}>
    <Icon size={16} strokeWidth={2} />
  </button>
);

// --- Main Layout Sections ---

const SidebarLeft = () => {
  return (
    <aside className="w-[260px] h-full bg-white/50 backdrop-blur-sm border-r border-slate-200/60 flex flex-col p-4 flex-shrink-0">
      {/* Logo Area */}
      <div className="flex items-center gap-3 px-2 mb-8 mt-2">
        <div className="relative w-8 h-8 rounded-full bg-gradient-to-br from-pink-400 via-purple-500 to-indigo-600 flex items-center justify-center shadow-md">
           <div className="absolute top-1/3 left-1/4 w-1 h-1.5 bg-white rounded-full opacity-90 transform -rotate-12"></div>
           <div className="absolute top-1/3 right-1/4 w-1 h-1.5 bg-white rounded-full opacity-90 transform rotate-12"></div>
        </div>
        <span className="font-semibold text-slate-800 text-lg tracking-tight">Smart Assistant</span>
      </div>

      {/* New Chat Button */}
      <button className="flex items-center justify-between w-full p-2.5 px-4 rounded-xl border border-slate-200 bg-white text-slate-700 hover:bg-slate-50 transition-colors shadow-sm mb-6 text-sm font-medium">
        <div className="flex items-center gap-2">
          <Plus size={16} className="text-indigo-600" />
          <span>New Chat</span>
        </div>
        <div className="flex items-center gap-1 text-slate-400 text-xs bg-slate-100 px-1.5 py-0.5 rounded">
          <Command size={12} />
          <span>K</span>
        </div>
      </button>

      {/* Primary Navigation */}
      <nav className="flex flex-col gap-1 mb-8">
        {[
          { icon: Home, label: 'Home' },
          { icon: MessageSquare, label: 'Chats', active: true },
          { icon: Wrench, label: 'Tools' },
          { icon: LayoutTemplate, label: 'Templates' },
          { icon: Settings, label: 'Settings' },
        ].map((item, idx) => (
          <button key={idx} className={`flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors ${item.active ? 'text-indigo-700 bg-indigo-50/80' : 'text-slate-600 hover:bg-slate-100'}`}>
            <item.icon size={18} strokeWidth={item.active ? 2.5 : 2} className={item.active ? 'text-indigo-600' : 'text-slate-400'} />
            {item.label}
          </button>
        ))}
      </nav>

      {/* History Sections */}
      <div className="flex-1 overflow-y-auto overflow-x-hidden scrollbar-hide -mx-2 px-2 flex flex-col gap-6">
        <div>
          <h3 className="text-xs font-semibold text-slate-400 mb-3 px-3">Today</h3>
          <div className="flex flex-col gap-0.5">
            {[
              { icon: MessageSquare, title: 'Quiz: World Capitals', time: '10:24 AM', active: true },
              { icon: FileText, title: 'Puzzle: Arrange Steps', time: '9:40 AM' },
              { icon: Utensils, title: 'Dinner Recipe Help', time: '9:15 AM' },
              { icon: Plane, title: 'Travel Plan to Japan', time: '8:50 AM' },
            ].map((item, idx) => (
              <div key={idx} className={`flex items-center justify-between px-3 py-2 rounded-lg text-sm cursor-pointer transition-colors ${item.active ? 'bg-white shadow-sm border border-slate-100 text-indigo-700' : 'text-slate-600 hover:bg-slate-50'}`}>
                <div className="flex items-center gap-2.5 overflow-hidden">
                  <div className={`p-1 rounded-md ${item.active ? 'bg-indigo-100/50 text-indigo-600' : 'text-slate-400'}`}>
                     <item.icon size={14} />
                  </div>
                  <span className="truncate font-medium">{item.title}</span>
                </div>
                <span className={`text-xs ${item.active ? 'text-indigo-400' : 'text-slate-400'}`}>{item.time}</span>
              </div>
            ))}
          </div>
        </div>

        <div>
          <h3 className="text-xs font-semibold text-slate-400 mb-3 px-3">Yesterday</h3>
          <div className="flex flex-col gap-0.5">
             {[
              { icon: FileText, title: 'Grammar Check', time: 'Yesterday' },
              { icon: Clock, title: 'Workout Plan', time: 'Yesterday' },
            ].map((item, idx) => (
              <div key={idx} className="flex items-center justify-between px-3 py-2 rounded-lg text-sm text-slate-600 hover:bg-slate-50 cursor-pointer transition-colors">
                <div className="flex items-center gap-2.5 overflow-hidden">
                  <div className="p-1 rounded-md text-slate-400">
                     <item.icon size={14} />
                  </div>
                  <span className="truncate font-medium">{item.title}</span>
                </div>
                <span className="text-xs text-slate-400">{item.time}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* User & Upgrade Bottom Area */}
      <div className="mt-4 flex flex-col gap-4">
        {/* User Profile */}
        <div className="flex items-center justify-between px-3 py-2 rounded-xl hover:bg-slate-50 border border-transparent hover:border-slate-100 cursor-pointer">
          <div className="flex items-center gap-3">
            <img src="https://i.pravatar.cc/150?u=a042581f4e29026704d" alt="User" className="w-8 h-8 rounded-full object-cover" />
            <div className="flex flex-col">
              <span className="text-sm font-semibold text-slate-800">Jasmin</span>
              <span className="text-[10px] font-medium text-indigo-600 bg-indigo-50 px-1.5 py-0.5 rounded uppercase tracking-wider inline-block w-max mt-0.5">Free Plan</span>
            </div>
          </div>
          <ChevronDown size={16} className="text-slate-400" />
        </div>

        {/* Upgrade Card */}
        <div className="bg-gradient-to-br from-[#f8f9ff] to-[#f1f4ff] border border-[#e5e9ff] rounded-2xl p-4 flex flex-col relative overflow-hidden">
          <div className="absolute top-0 right-0 w-20 h-20 bg-indigo-200/30 rounded-full blur-2xl -mr-10 -mt-10"></div>
          <div className="flex items-center gap-2 mb-1 z-10">
            <span className="text-lg">⭐</span>
            <span className="text-sm font-semibold text-slate-800">Upgrade to Pro</span>
          </div>
          <p className="text-xs text-slate-500 mb-3 z-10">Unlock more features</p>
          <button className="w-full bg-indigo-500 hover:bg-indigo-600 text-white text-xs font-semibold py-2 rounded-lg transition-colors z-10 shadow-sm shadow-indigo-200">
            Upgrade Now
          </button>
        </div>
      </div>
    </aside>
  );
};

const SidebarRight = () => {
  return (
    <aside className="w-[300px] h-full bg-white/50 backdrop-blur-sm border-l border-slate-200/60 p-5 flex flex-col gap-6 overflow-y-auto">
      
      {/* Voice Assistant Module */}
      <div>
        <h3 className="text-sm font-semibold text-slate-800 mb-4">Voice Assistant</h3>
        <div className="mb-4">
           <span className="text-xs text-indigo-500 font-medium animate-pulse">Listening...</span>
        </div>
        <div className="h-10 mb-2">
           <Waveform bars={35} active={true} />
        </div>
        <div className="text-xs text-slate-500 font-medium">00:07</div>
      </div>

      {/* Thinking Module */}
      <div>
        <div className="flex items-center gap-2 mb-4 text-slate-800">
          <div className="w-6 h-6 rounded-full bg-indigo-50 flex items-center justify-center text-indigo-500">
             <Sparkles size={12} />
          </div>
          <h3 className="text-sm font-semibold">Thinking</h3>
        </div>
        <p className="text-xs text-slate-500 leading-relaxed mb-4 pr-4">Let me analyze this quiz and check the answers...</p>
        
        <div className="flex flex-col gap-0 relative">
          {/* Vertical line connecting status nodes */}
          <div className="absolute left-2.5 top-3 bottom-4 w-0.5 bg-slate-100 z-0"></div>
          
          {[
            { label: 'Understanding the question', status: 'done', icon: CheckCircle2 },
            { label: 'Analyzing matches', status: 'active', icon: Circle },
            { label: 'Verifying answers', status: 'pending', icon: Circle },
            { label: 'Preparing feedback', status: 'pending', icon: Circle },
          ].map((item, idx) => (
            <div key={idx} className="flex items-center gap-3 py-2 z-10">
              <div className="w-5 h-5 flex items-center justify-center bg-white">
                {item.status === 'done' && <CheckCircle2 size={16} className="text-green-500" />}
                {item.status === 'active' && (
                  <div className="relative flex items-center justify-center">
                    <Circle size={16} className="text-indigo-500 fill-indigo-100" />
                    <div className="absolute w-1.5 h-1.5 bg-indigo-500 rounded-full animate-ping"></div>
                  </div>
                )}
                {item.status === 'pending' && <Circle size={14} className="text-slate-200" strokeWidth={3} />}
              </div>
              <span className={`text-sm ${item.status === 'active' ? 'text-slate-800 font-medium' : item.status === 'done' ? 'text-slate-600' : 'text-slate-400'}`}>
                {item.label}
              </span>
              {item.status === 'done' && <Check size={14} className="ml-auto text-slate-300" />}
              {item.status === 'active' && (
                 <div className="ml-auto flex gap-0.5">
                    <div className="w-1 h-1 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }}></div>
                    <div className="w-1 h-1 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }}></div>
                    <div className="w-1 h-1 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }}></div>
                 </div>
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Tools & Actions Module */}
      <div>
        <div className="flex items-center gap-2 mb-4 text-slate-800">
           <Wrench size={16} className="text-slate-400" />
           <h3 className="text-sm font-semibold">Tools & Actions</h3>
        </div>
        <div className="flex flex-col gap-2">
          {[
            { title: 'Quiz Generator', desc: 'Created a matching quiz', icon: FileText, color: 'text-indigo-500', bg: 'bg-indigo-50' },
            { title: 'Validator', desc: 'Checked all answers', icon: CheckCircle2, color: 'text-green-500', bg: 'bg-green-50' },
            { title: 'Explainer', desc: 'Generated explanations', icon: MessageSquare, color: 'text-blue-500', bg: 'bg-blue-50' },
          ].map((tool, idx) => (
            <div key={idx} className="flex items-start gap-3 p-3 rounded-xl border border-slate-100 bg-white shadow-[0_2px_8px_-4px_rgba(0,0,0,0.05)]">
              <div className={`p-2 rounded-lg ${tool.bg} ${tool.color} mt-0.5`}>
                <tool.icon size={16} />
              </div>
              <div>
                <div className="text-sm font-semibold text-slate-800">{tool.title}</div>
                <div className="text-xs text-slate-500 mt-0.5">{tool.desc}</div>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Response Module */}
      <div className="mt-auto">
        <div className="flex items-center gap-2 mb-3 text-slate-800">
           <LayoutTemplate size={16} className="text-slate-400" />
           <h3 className="text-sm font-semibold">Response</h3>
        </div>
        <div className="flex items-center justify-between p-3 rounded-xl border border-slate-100 bg-white shadow-sm">
           <span className="text-sm text-slate-600">All matches are correct!</span>
           <div className="w-5 h-5 rounded-full bg-green-500 flex items-center justify-center text-white">
             <Check size={12} strokeWidth={3} />
           </div>
        </div>
      </div>

      {/* Model Selector */}
      <div className="mt-2">
         <div className="text-[10px] text-slate-400 uppercase tracking-wider mb-1 px-1">Model</div>
         <button className="w-full flex items-center justify-between p-2.5 px-3 rounded-lg border border-slate-200 bg-white text-sm text-slate-600 hover:bg-slate-50 transition-colors">
            <span>GPT-4o Mini</span>
            <ChevronDown size={14} className="text-slate-400" />
         </button>
      </div>

    </aside>
  );
};

const MainContent = () => {
  return (
    <main className="flex-1 flex flex-col h-full overflow-hidden relative">
      {/* Top Header/Greeting */}
      <header className="px-10 pt-10 pb-6 flex items-start justify-between z-10">
        <div>
          <h1 className="text-3xl font-bold text-slate-800 flex items-center gap-3">
            Hi, Jasmin <span className="text-2xl origin-bottom-right animate-wave inline-block">👋</span>
          </h1>
          <p className="text-slate-500 mt-2 text-lg">How may I help you today?</p>
        </div>
        <button className="w-8 h-8 rounded-full border border-slate-200 flex items-center justify-center text-slate-400 hover:bg-slate-50 transition-colors">
          <MoreHorizontal size={16} />
        </button>
      </header>

      {/* Central Interactive Area */}
      <div className="flex-1 overflow-y-auto px-10 pb-32 z-10 flex flex-col items-center">
        
        {/* The Orb */}
        <div className="relative flex flex-col items-center mb-8">
           {/* Orb element */}
           <div className="w-20 h-20 rounded-full bg-gradient-to-br from-[#ffb4c2] via-[#f84d85] to-[#5b4eff] shadow-[0_15px_40px_-5px_rgba(248,77,133,0.5),inset_0_-10px_20px_rgba(0,0,0,0.1)] flex items-center justify-center relative overflow-hidden">
             {/* Glossy top highlight */}
             <div className="absolute top-1 left-2 w-14 h-8 bg-white/30 rounded-full blur-[2px] transform -rotate-12"></div>
             {/* Eyes */}
             <div className="absolute top-[35%] left-[30%] w-2.5 h-3.5 bg-white rounded-full opacity-95 transform -rotate-12 shadow-sm"></div>
             <div className="absolute top-[35%] right-[30%] w-2.5 h-3.5 bg-white rounded-full opacity-95 transform rotate-12 shadow-sm"></div>
           </div>
           
           {/* Speech Bubble */}
           <div className="absolute -top-4 -right-36 bg-white border border-slate-100 shadow-[0_4px_12px_rgba(0,0,0,0.05)] rounded-2xl rounded-bl-sm py-2 px-4">
              <span className="text-xs font-medium text-slate-600">Let's solve this together!</span>
           </div>
        </div>

        {/* Quiz Card */}
        <div className="w-full max-w-3xl bg-white border border-slate-100 rounded-[20px] p-6 shadow-[0_8px_30px_rgb(0,0,0,0.04)] mb-6">
          <div className="flex items-center justify-between mb-2">
            <h2 className="text-lg font-bold text-slate-800">Quiz: World Capitals</h2>
            <div className="flex items-center gap-1.5 bg-indigo-50/80 px-2.5 py-1.5 rounded-lg border border-indigo-100/50">
              <Sparkles size={12} className="text-indigo-600" />
              <span className="text-xs font-semibold text-indigo-700">Generative Quiz</span>
            </div>
          </div>
          <p className="text-sm text-slate-500 mb-8">Match the capital city to its country.</p>

          {/* Grid Container for Matches */}
          <div className="grid grid-cols-2 gap-x-16 gap-y-3 relative mb-8">
            
            {/* --- SVG Connecting Line --- */}
            <svg className="absolute inset-0 w-full h-full pointer-events-none z-0" style={{ overflow: 'visible' }}>
              <path 
                d="M 175 25 C 220 25, 200 85, 270 85" 
                fill="none" 
                stroke="#c7d2fe" /* indigo-200 */
                strokeWidth="2"
                strokeDasharray="4 4"
                strokeLinecap="round"
              />
              {/* Endpoint Dots */}
              <circle cx="175" cy="25" r="3" fill="#a5b4fc" />
              <circle cx="270" cy="85" r="3" fill="#a5b4fc" />
            </svg>

            {/* Left Column: Capitals */}
            <div className="flex flex-col gap-3 z-10">
              <div className="text-xs font-semibold text-slate-400 mb-1 ml-1">Capitals</div>
              
              <div className="flex items-center border border-slate-200 rounded-xl p-3 bg-white hover:border-indigo-300 transition-colors shadow-sm">
                <span className="text-slate-400 text-sm w-6">1.</span>
                <span className="text-slate-700 text-sm font-medium flex-1">Tokyo</span>
                <GripVertical size={16} className="text-slate-300 cursor-grab" />
              </div>
              
              <div className="flex items-center border border-slate-200 rounded-xl p-3 bg-white hover:border-indigo-300 transition-colors shadow-sm">
                <span className="text-slate-400 text-sm w-6">2.</span>
                <span className="text-slate-700 text-sm font-medium flex-1">Paris</span>
                <GripVertical size={16} className="text-slate-300 cursor-grab" />
              </div>

              <div className="flex items-center border border-slate-200 rounded-xl p-3 bg-white hover:border-indigo-300 transition-colors shadow-sm">
                <span className="text-slate-400 text-sm w-6">3.</span>
                <span className="text-slate-700 text-sm font-medium flex-1">Ottawa</span>
                <GripVertical size={16} className="text-slate-300 cursor-grab" />
              </div>

              <div className="flex items-center border border-slate-200 rounded-xl p-3 bg-white hover:border-indigo-300 transition-colors shadow-sm">
                <span className="text-slate-400 text-sm w-6">4.</span>
                <span className="text-slate-700 text-sm font-medium flex-1">Canberra</span>
                <GripVertical size={16} className="text-slate-300 cursor-grab" />
              </div>
            </div>

            {/* Right Column: Countries */}
            <div className="flex flex-col gap-3 z-10 relative">
              <div className="text-xs font-semibold text-slate-400 mb-1 ml-1">Countries</div>
              
              <div className="flex items-center border border-slate-200 rounded-xl p-3 bg-white hover:border-indigo-300 transition-colors shadow-sm">
                <span className="text-slate-400 text-sm w-6">A.</span>
                <span className="text-slate-700 text-sm font-medium flex-1">Australia</span>
              </div>

              {/* Active/Hovered Item */}
              <div className="relative flex items-center border-2 border-indigo-400 rounded-xl p-3 bg-indigo-50/30 shadow-[0_2px_10px_-2px_rgba(99,102,241,0.2)]">
                <span className="text-indigo-500 text-sm font-semibold w-6">B.</span>
                <span className="text-slate-800 text-sm font-medium flex-1">Japan</span>
                
                {/* Fake Mouse Cursor over Japan */}
                <svg className="absolute -bottom-4 right-10 w-5 h-5 text-slate-800 drop-shadow-md z-50 pointer-events-none" viewBox="0 0 24 24" fill="currentColor" stroke="white" strokeWidth="1.5">
                  <path d="M4.5 3L19.5 10.5L12 13L10.5 21L4.5 3Z" />
                </svg>
              </div>

              <div className="flex items-center border border-slate-200 rounded-xl p-3 bg-white hover:border-indigo-300 transition-colors shadow-sm">
                <span className="text-slate-400 text-sm w-6">C.</span>
                <span className="text-slate-700 text-sm font-medium flex-1">France</span>
              </div>

              <div className="flex items-center border border-slate-200 rounded-xl p-3 bg-white hover:border-indigo-300 transition-colors shadow-sm">
                <span className="text-slate-400 text-sm w-6">D.</span>
                <span className="text-slate-700 text-sm font-medium flex-1">Canada</span>
              </div>
            </div>

          </div>

          {/* Action Buttons */}
          <div className="flex justify-end gap-3 pt-2">
            <button className="px-6 py-2.5 rounded-xl border border-slate-200 text-slate-600 font-medium text-sm hover:bg-slate-50 transition-colors">
              Reset
            </button>
            <button className="px-6 py-2.5 rounded-xl bg-[#615EE6] text-white font-medium text-sm hover:bg-indigo-600 transition-colors shadow-md shadow-indigo-200">
              Check Answer
            </button>
          </div>
        </div>

        {/* Feedback Card */}
        <div className="w-full max-w-3xl bg-white border border-slate-100 rounded-[20px] p-6 shadow-[0_8px_30px_rgb(0,0,0,0.04)] relative overflow-hidden">
          <div className="flex justify-between items-start">
            <div className="z-10">
              <div className="flex items-center gap-2 mb-4">
                 <div className="w-6 h-6 rounded-full bg-green-100 border border-green-200 flex items-center justify-center text-green-600">
                    <Check size={14} strokeWidth={3} />
                 </div>
                 <h3 className="text-lg font-bold text-green-600">Great job! <span className="text-xl">🎉</span></h3>
              </div>
              
              <p className="text-sm font-semibold text-slate-800 mb-3 ml-8">All matches are correct.</p>
              
              <ul className="text-sm text-slate-600 space-y-1.5 ml-8 font-medium">
                <li>1 - B (Tokyo - Japan)</li>
                <li>2 - C (Paris - France)</li>
                <li>3 - D (Ottawa - Canada)</li>
                <li>4 - A (Canberra - Australia)</li>
              </ul>
            </div>

            {/* 3D Globe Illustration Simulation */}
            <div className="relative w-32 h-32 mr-4 mt-2 z-10 flex items-center justify-center">
               {/* Globe Base Shadow */}
               <div className="absolute -bottom-2 w-20 h-4 bg-black/10 rounded-[100%] blur-sm"></div>
               
               {/* Stand arc */}
               <div className="absolute bottom-1 w-24 h-16 border-b-4 border-l-4 border-r-4 border-slate-300 rounded-b-full transform rotate-12"></div>
               
               {/* Globe Sphere */}
               <div className="relative w-20 h-20 rounded-full bg-gradient-to-tr from-[#3b82f6] via-[#60a5fa] to-[#bfdbfe] shadow-inner overflow-hidden border border-blue-400/30 shadow-[inset_-10px_-10px_20px_rgba(0,0,0,0.2),0_10px_15px_rgba(0,0,0,0.1)]">
                  {/* Continents overlay (simplified using css shapes and blur) */}
                  <div className="absolute top-2 left-2 w-8 h-8 bg-green-400/80 rounded-full blur-[1px]"></div>
                  <div className="absolute bottom-4 right-1 w-10 h-6 bg-green-500/70 rounded-full blur-[2px] transform -rotate-12"></div>
                  <div className="absolute top-6 right-2 w-5 h-5 bg-green-400/80 rounded-full blur-[1px]"></div>
               </div>

               {/* Location Pin */}
               <div className="absolute -top-2 right-12 z-20 flex flex-col items-center animate-bounce">
                 <div className="w-6 h-6 bg-indigo-500 rounded-t-full rounded-bl-full rounded-br-sm transform -rotate-45 flex items-center justify-center shadow-lg border border-indigo-400">
                    <div className="w-2 h-2 bg-white rounded-full transform rotate-45"></div>
                 </div>
                 <div className="w-1.5 h-1.5 bg-black/20 rounded-full mt-1 blur-[1px]"></div>
               </div>
            </div>
          </div>

          {/* Bottom Actions */}
          <div className="flex gap-1 mt-6 border-t border-slate-100 pt-4">
             <IconButton icon={ThumbsUp} />
             <IconButton icon={ThumbsDown} />
             <IconButton icon={Copy} className="ml-2" />
          </div>
        </div>

      </div>

      {/* Fixed Bottom Input Area */}
      <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-[#f8f9fc] via-[#f8f9fc] to-transparent pt-10 pb-6 px-10 z-20 flex flex-col items-center">
        <div className="w-full max-w-3xl relative">
          <div className="flex items-center bg-white border border-slate-200/80 rounded-2xl p-2.5 shadow-[0_8px_30px_rgb(0,0,0,0.06)]">
            
            {/* Mic Button */}
            <button className="w-11 h-11 rounded-xl bg-indigo-500 hover:bg-indigo-600 text-white flex items-center justify-center transition-colors shadow-sm flex-shrink-0">
              <Mic size={20} />
            </button>

            {/* Input Field & Waveform */}
            <div className="flex-1 px-4 flex items-center relative h-11">
              <input 
                type="text" 
                placeholder="Tap mic and start talking..." 
                className="w-full h-full bg-transparent border-none focus:outline-none text-slate-600 placeholder:text-slate-400 text-sm"
                readOnly // Make it read-only for exact visual replica of listening state
              />
              
              {/* Center faded waveform overlay */}
              <div className="absolute inset-0 flex items-center justify-center pointer-events-none opacity-40">
                 <Waveform bars={40} active={true} colorClass="bg-slate-300" />
              </div>
            </div>

            {/* Send Button */}
            <button className="w-11 h-11 rounded-xl bg-indigo-500 hover:bg-indigo-600 text-white flex items-center justify-center transition-colors shadow-sm flex-shrink-0">
              <ArrowRight size={20} />
            </button>

          </div>
        </div>
        
        <p className="text-[11px] text-slate-400 mt-4 font-medium tracking-wide">
          Smart Assistant can make mistakes. Please verify important information.
        </p>
      </div>

    </main>
  );
};

export default function App() {
  return (
    <div className="flex h-screen w-full bg-[#f8f9fc] font-sans overflow-hidden">
      <style dangerouslySetInnerHTML={{__html: `
        @keyframes wave {
          0%, 100% { transform: rotate(0deg); }
          25% { transform: rotate(-15deg); }
          50% { transform: rotate(10deg); }
          75% { transform: rotate(-5deg); }
        }
        .animate-wave {
          animation: wave 2s ease-in-out infinite;
        }
        /* Hide scrollbar for Chrome, Safari and Opera */
        .scrollbar-hide::-webkit-scrollbar {
          display: none;
        }
        /* Hide scrollbar for IE, Edge and Firefox */
        .scrollbar-hide {
          -ms-overflow-style: none;  /* IE and Edge */
          scrollbar-width: none;  /* Firefox */
        }
      `}} />
      <SidebarLeft />
      <MainContent />
      <SidebarRight />
    </div>
  );
}