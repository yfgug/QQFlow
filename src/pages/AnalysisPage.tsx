import { useState } from 'react'
import { BarChart3, Users, MessageCircle } from 'lucide-react'
import GroupAnalysisTab from './GroupAnalysisTab'
import PrivateAnalysisTab from './PrivateAnalysisTab'
import './AnalysisPage.scss'

type Tab = 'group' | 'private'

function AnalysisPage() {
  const [tab, setTab] = useState<Tab>('group')

  return (
    <div className="analysis-page">
      <div className="page-header">
        <BarChart3 size={24} />
        <h1>聊天分析</h1>
      </div>
      <div className="tab-bar">
        <button className={`tab-btn ${tab === 'group' ? 'active' : ''}`} onClick={() => setTab('group')}>
          <Users size={16} /> 群聊分析
        </button>
        <button className={`tab-btn ${tab === 'private' ? 'active' : ''}`} onClick={() => setTab('private')}>
          <MessageCircle size={16} /> 私聊分析
        </button>
      </div>
      <div className="tab-content">
        {tab === 'group' && <GroupAnalysisTab />}
        {tab === 'private' && <PrivateAnalysisTab />}
      </div>
    </div>
  )
}

export default AnalysisPage
